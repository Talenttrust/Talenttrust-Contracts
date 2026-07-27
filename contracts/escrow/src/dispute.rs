//! Dispute payout arithmetic and final-status helpers.
//!
//! This module is intentionally storage-free. It computes how the currently
//! available escrow balance should be split for a `DisputeResolution` and tells
//! the root dispute entrypoint whether the contract should end as `Completed`
//! or `Refunded`. The root entrypoints own authentication, token transfer, event
//! publication, and writes to `DataKey::Contract(contract_id)`.

use soroban_sdk::{Address, BytesN, Env};

use crate::{
    safe_add_amounts, Contract, ContractStatus, DataKey, DisputeConfig, DisputeMetadata,
    DisputeMetadataV0, DisputeResolution, Error, DISPUTE_STORAGE_VERSION,
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

// ---------------------------------------------------------------------------
// Dispute metadata storage helpers
// ---------------------------------------------------------------------------

/// Persist dispute metadata for a contract.
pub fn store_dispute_metadata(env: &Env, contract_id: u32, metadata: &DisputeMetadata) {
    env.storage()
        .persistent()
        .set(&DataKey::Dispute(contract_id), metadata);
}

/// Remove dispute metadata for a contract.
pub fn clear_dispute_metadata(env: &Env, contract_id: u32) {
    env.storage()
        .persistent()
        .remove(&DataKey::Dispute(contract_id));
}

/// Return the schema version of the stored dispute metadata, or 0 if none exists.
pub fn get_dispute_storage_version(env: &Env, contract_id: u32) -> u32 {
    if env
        .storage()
        .persistent()
        .has(&DataKey::Dispute(contract_id))
    {
        DISPUTE_STORAGE_VERSION
    } else {
        0
    }
}

/// Read dispute metadata with automatic v0 → v1 migration.
///
/// Panics with `DisputeNotFound` when no record exists.
pub fn load_dispute_metadata(env: &Env, contract_id: u32) -> DisputeMetadata {
    if let Some(meta) = env
        .storage()
        .persistent()
        .get::<_, DisputeMetadata>(&DataKey::Dispute(contract_id))
    {
        if meta.schema_version > DISPUTE_STORAGE_VERSION {
            env.panic_with_error(Error::UnsupportedDisputeStorageVersion);
        }
        return meta;
    }
    // Try v0 → v1 migration
    if let Some(v0) = env
        .storage()
        .persistent()
        .get::<_, DisputeMetadataV0>(&DataKey::Dispute(contract_id))
    {
        let v1 = migrate_dispute_metadata_v0_to_v1(v0);
        store_dispute_metadata(env, contract_id, &v1);
        return v1;
    }

    env.panic_with_error(Error::DisputeNotFound)
}

/// Migrate a v0 metadata record to the current schema version.
pub fn migrate_dispute_metadata_v0_to_v1(v0: DisputeMetadataV0) -> DisputeMetadata {
    DisputeMetadata {
        schema_version: DISPUTE_STORAGE_VERSION,
        raised_by: v0.raised_by,
        reason_hash: v0.reason_hash,
        raised_at: v0.raised_at,
    }
}

// ---------------------------------------------------------------------------
// raise_dispute / resolve_dispute entrypoints
// ---------------------------------------------------------------------------

// Dispute entrypoints are implemented in `contracts/escrow/src/lib.rs`.
// This module retains dispute-related helpers only.
