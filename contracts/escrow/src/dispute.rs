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
    DisputeInfo, DisputeResolution, Error, Escrow,
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
/// Returns a [`DisputeInfo`] with named fields so callers can reference
/// `client_payout`, `freelancer_payout`, and `available_balance` by name
/// rather than relying on positional tuple index (issue #51).
///
/// The available balance is computed as:
/// `available = funded_amount - released_amount - refunded_amount`.
///
/// # Invariant
/// `result.client_payout + result.freelancer_payout == result.available_balance`
///
/// # Errors
/// - [`Error::AccountingInvariantViolated`] if available would be negative (corrupted state)
/// - [`Error::PotentialOverflow`] if intermediate calculations overflow
/// - [`Error::InvalidDisputeSplit`] for Split variant with negative legs or non-conserving sum
pub fn resolution_payouts(
    contract: &Contract,
    resolution: &DisputeResolution,
) -> Result<DisputeInfo, Error> {
    let available = contract
        .funded_amount
        .checked_sub(contract.released_amount)
        .and_then(|value| value.checked_sub(contract.refunded_amount))
        .ok_or(Error::AccountingInvariantViolated)?;
    if available < 0 {
        return Err(Error::AccountingInvariantViolated);
    }

    match resolution {
        DisputeResolution::FullRefund => Ok(DisputeInfo {
            available_balance: available,
            client_payout: available,
            freelancer_payout: 0,
        }),
        DisputeResolution::PartialRefund => {
            // freelancer gets floor(available * 30 / 100), client gets remainder
            let freelancer_payout = available
                .checked_mul(30)
                .and_then(|value| value.checked_div(100))
                .ok_or(Error::PotentialOverflow)?;
            let client_payout = available - freelancer_payout;
            Ok(DisputeInfo {
                available_balance: available,
                client_payout,
                freelancer_payout,
            })
        }
        DisputeResolution::FullPayout => Ok(DisputeInfo {
            available_balance: available,
            client_payout: 0,
            freelancer_payout: available,
        }),
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
            Ok(DisputeInfo {
                available_balance: available,
                client_payout: split.client_amount,
                freelancer_payout: split.freelancer_amount,
            })
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

    // Named fields instead of opaque tuple index (issue #51).
    let info =
        resolution_payouts(&contract, &resolution).unwrap_or_else(|e| env.panic_with_error(e));
    contract.refunded_amount += info.client_payout;
    contract.released_amount += info.freelancer_payout;
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
