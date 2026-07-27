//! Dispute payout arithmetic and final-status helpers.
//!
//! This module is intentionally storage-free. It computes how the currently
//! available escrow balance should be split for a `DisputeResolution` and tells
//! the root dispute entrypoint whether the contract should end as `Completed`
//! or `Refunded`. ABI-compatible wrappers in the crate root delegate here;
//! this module owns dispute authorization, state changes, events, and writes to
//! `DataKey::Contract(contract_id)`.

use soroban_sdk::{symbol_short, Address, Env};

use crate::{
    rollback, safe_add_amounts, ttl, Contract, ContractStatus, DataKey, DisputeConfig,
    DisputeResolution, Error, Escrow,
};

/// Read-only getter for the arbiter dispute-split configuration.
///
/// Returns `None` before any admin call to `set_arbiter_config`; callers
/// should fall back to `DisputeConfig::default()` (30/70 split).
pub fn get_dispute_config(env: &Env) -> Option<DisputeConfig> {
    env.storage().persistent().get(&DataKey::DisputeConfigKey)
}

/// Storage writer for the arbiter dispute-split configuration.
pub fn set_dispute_config(env: &Env, config: DisputeConfig) {
    env.storage()
        .persistent()
        .set(&DataKey::DisputeConfigKey, &config);
}

/// Compute the payout split for a dispute resolution.
///
/// Returns `(client_payout, freelancer_payout)` where both values are non-negative
/// and sum to the available balance. The available balance is computed as:
/// `available = funded_amount - released_amount - refunded_amount`.
///
/// # Errors
/// - `AccountingInvariantViolated` if available would be negative (corrupted state)
/// - `PotentialOverflow` if intermediate calculations overflow
/// - `InvalidDisputeSplit` for Split variant with negative legs or non-conserving sum
pub fn resolution_payouts(
    contract: &Contract,
    resolution: &DisputeResolution,
) -> Result<(i128, i128), Error> {
    let available = contract
        .funded_amount
        .checked_sub(contract.released_amount)
        .and_then(|value| value.checked_sub(contract.refunded_amount))
        .ok_or(Error::AccountingInvariantViolated)?;
    if available < 0 {
        return Err(Error::AccountingInvariantViolated);
    }

    match resolution {
        DisputeResolution::FullRefund => Ok((available, 0)),
        DisputeResolution::PartialRefund => {
            // freelancer gets floor(available * 30 / 100), client gets remainder
            let freelancer_payout = available
                .checked_mul(30)
                .and_then(|value| value.checked_div(100))
                .ok_or(Error::PotentialOverflow)?;
            Ok((available - freelancer_payout, freelancer_payout))
        }
        DisputeResolution::FullPayout => Ok((0, available)),
        DisputeResolution::Split(split) => {
            if split.client_amount < 0 || split.freelancer_amount < 0 {
                return Err(Error::InvalidDisputeSplit);
            }
            // Issue #572: Reject split resolution whose components are individually within but jointly exceed balance
            if split.client_amount > available || split.freelancer_amount > available {
                return Err(Error::InvalidDisputeSplit);
            }
            let total = safe_add_amounts(split.client_amount, split.freelancer_amount)
                .ok_or(Error::PotentialOverflow)?;
            if total > available || total != available {
                return Err(Error::InvalidDisputeSplit);
            }
            Ok((split.client_amount, split.freelancer_amount))
        }
    }
}

/// Determine the final contract status after dispute resolution.
///
/// Returns `Refunded` only when the full deposit has been refunded.
/// Otherwise returns `Completed`.
pub fn final_status_after_resolution(contract: &Contract) -> ContractStatus {
    if contract.refunded_amount == contract.funded_amount {
        ContractStatus::Refunded
    } else {
        ContractStatus::Completed
    }
}

/// Open a dispute after enforcing lifecycle, role, and arbiter guards.
///
/// The public Soroban entrypoint remains on [`Escrow`] so its ABI stays stable;
/// this helper keeps the complete dispute workflow in this module.
pub(crate) fn raise_dispute_impl(env: &Env, contract_id: u32, caller: Address) -> bool {
    Escrow::require_initialized(env);
    Escrow::require_not_paused(env);
    caller.require_auth();

    let mut contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    ttl::extend_contract_ttl(env, contract_id);
    Escrow::require_not_finalized(env, contract_id);

    if caller != contract.client && caller != contract.freelancer {
        env.panic_with_error(Error::UnauthorizedRole);
    }
    if contract.arbiter.is_none() {
        env.panic_with_error(Error::ArbiterRequired);
    }
    match contract.status {
        ContractStatus::Funded | ContractStatus::PartiallyFunded => {}
        _ => env.panic_with_error(Error::InvalidState),
    }

    let milestones = ttl::load_milestones(env, contract_id);
    rollback::store_dispute_rollback(env, contract_id, &contract, &milestones);
    contract.status = ContractStatus::Disputed;
    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), &contract);
    ttl::extend_contract_ttl(env, contract_id);

    env.events().publish(
        (symbol_short!("dispute"), symbol_short!("opened")),
        (contract_id, caller),
    );
    true
}

/// Resolve a dispute after enforcing arbiter authorization and split conservation.
pub(crate) fn resolve_dispute_impl(
    env: &Env,
    contract_id: u32,
    arbiter: Address,
    resolution: DisputeResolution,
) -> bool {
    Escrow::require_initialized(env);
    Escrow::require_not_paused(env);
    arbiter.require_auth();

    let mut contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    ttl::extend_contract_ttl(env, contract_id);
    Escrow::require_not_finalized(env, contract_id);
    if contract.status != ContractStatus::Disputed {
        env.panic_with_error(Error::InvalidStatusTransition);
    }
    match &contract.arbiter {
        Some(contract_arbiter) if *contract_arbiter == arbiter => {}
        _ => env.panic_with_error(Error::UnauthorizedRole),
    }

    let (client_payout, freelancer_payout) =
        resolution_payouts(&contract, &resolution).unwrap_or_else(|e| env.panic_with_error(e));
    contract.refunded_amount += client_payout;
    contract.released_amount += freelancer_payout;
    contract.status = final_status_after_resolution(&contract);
    if contract.status == ContractStatus::Completed {
        Escrow::grant_pending_reputation_credit(env, &contract.freelancer);
    }

    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), &contract);
    rollback::clear_dispute_rollback(env, contract_id);
    ttl::extend_contract_ttl(env, contract_id);
    env.events().publish(
        (symbol_short!("dispute"), symbol_short!("resolved")),
        (contract_id, resolution.code()),
    );
    true
}
