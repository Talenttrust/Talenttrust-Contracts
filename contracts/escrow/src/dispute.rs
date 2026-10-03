//! Dispute payout arithmetic, final-status helpers, and deterministic recovery.
//!
//! This module is intentionally storage-free. It computes how the currently
//! available escrow balance should be split for a `DisputeResolution` and tells
//! the root dispute entrypoint whether the contract should end as `Completed`
//! or `Refunded`. ABI-compatible wrappers in the crate root delegate here;
//! this module owns dispute metadata persistence and payout arithmetic. Dispute
//! authorization, state changes, and events live in the crate root entrypoints
//! that call into these helpers.

use crate::{
    safe_add_amounts, types::DisputeMetadataV0, Contract, ContractStatus, DataKey, DisputeConfig,
    DisputeMetadata, DisputeResolution, Error, DISPUTE_STORAGE_VERSION,
};
use soroban_sdk::Env;

/// Freelancer share of a partial-refund dispute resolution, in percent.
pub const PARTIAL_REFUND_FREELANCER_PERCENT: i128 = 30;
/// Percent base used with [`PARTIAL_REFUND_FREELANCER_PERCENT`].
pub const PARTIAL_REFUND_PERCENT_BASE: i128 = 100;

// ---------------------------------------------------------------------------
// Validation boundaries
// ---------------------------------------------------------------------------

/// Maximum number of bytes accepted in a dispute reason hash.
///
/// Boundary: `0 <= reason_hash.len() <= MAX_DISPUTE_REASON_HASH_LEN`.
/// A zero-length hash is valid (callers may omit the reason); anything longer
/// is rejected as [`Error::InvalidDisputeReason`] to keep storage bounded.
pub const MAX_DISPUTE_REASON_HASH_LEN: u32 = 64;

/// Basis-point denominator used for dispute split configuration.
///
/// Boundary: `partial_refund_freelancer_bps + partial_refund_client_bps == 10_000`.
pub const DISPUTE_BPS_DENOMINATOR: u32 = 10_000;

/// Validate a dispute reason hash length against [`MAX_DISPUTE_REASON_HASH_LEN`].
///
/// Returns `Ok(())` for lengths in `[0, MAX_DISPUTE_REASON_HASH_LEN]` and
/// [`Error::InvalidDisputeReason`] otherwise. This is a pure boundary check so
/// it can be reused by entrypoints before any storage mutation occurs.
pub fn validate_dispute_reason_hash_len(len: u32) -> Result<(), Error> {
    if len > MAX_DISPUTE_REASON_HASH_LEN {
        return Err(Error::InvalidDisputeReason);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// DisputeConfig default basis-point constants
// ---------------------------------------------------------------------------

/// Default freelancer share of a partial-refund dispute resolution, in basis points.
///
/// `3_000 bps = 30 %`. This is stored in [`DisputeConfig::partial_refund_freelancer_bps`]
/// when no explicit arbiter configuration has been set via `set_arbiter_config`. The
/// counterpart (client share) is [`DEFAULT_DISPUTE_CLIENT_BPS`] = 7_000 bps = 70 %.
pub const DEFAULT_DISPUTE_FREELANCER_BPS: u32 = 3_000;

/// Default client share of a partial-refund dispute resolution, in basis points.
///
/// `7_000 bps = 70 %`. The pair `(DEFAULT_DISPUTE_FREELANCER_BPS, DEFAULT_DISPUTE_CLIENT_BPS)`
/// must sum to `10_000 bps (100 %)`. This constant is used as the default value of
/// [`DisputeConfig::partial_refund_client_bps`] when the arbiter has not explicitly
/// configured a dispute split via `set_arbiter_config`.
pub const DEFAULT_DISPUTE_CLIENT_BPS: u32 = 7_000;

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeInfo {
    pub available_balance: i128,
    pub client_payout: i128,
    pub freelancer_payout: i128,
}

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

/// Validate a [`DisputeConfig`] split before persisting it.
///
/// Boundaries enforced:
/// - `partial_refund_freelancer_bps + partial_refund_client_bps == DISPUTE_BPS_DENOMINATOR`
/// - each leg is `<= DISPUTE_BPS_DENOMINATOR`
///
/// Returns [`Error::InvalidDisputeSplit`] on any violation so that
/// `set_arbiter_config` cannot persist a non-conserving split.
pub fn validate_dispute_config(config: &DisputeConfig) -> Result<(), Error> {
    let total = config
        .partial_refund_freelancer_bps
        .checked_add(config.partial_refund_client_bps)
        .ok_or(Error::InvalidDisputeSplit)?;
    if total != DISPUTE_BPS_DENOMINATOR {
        return Err(Error::InvalidDisputeSplit);
    }
    Ok(())
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
/// - [`Error::InvalidDisputeSplit`] if `available == 0` (nothing left to distribute)
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
    if available == 0 {
        return Err(Error::InvalidDisputeSplit);
    }

    match resolution {
        DisputeResolution::FullRefund => Ok(DisputeInfo {
            available_balance: available,
            client_payout: available,
            freelancer_payout: 0,
        }),
        DisputeResolution::PartialRefund => {
            // freelancer gets floor(available * PARTIAL_REFUND_FREELANCER_PERCENT / 100),
            // client gets remainder
            let freelancer_payout = available
                .checked_mul(PARTIAL_REFUND_FREELANCER_PERCENT)
                .and_then(|value| value.checked_div(PARTIAL_REFUND_PERCENT_BASE))
                .ok_or(Error::PotentialOverflow)?;
            let client_payout = available
                .checked_sub(freelancer_payout)
                .ok_or(Error::PotentialOverflow)?;
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
///
/// Boundary: `refunded_amount == funded_amount` yields `Refunded`; any other
/// combination (including `refunded_amount > funded_amount`, which is a
/// corrupted state) yields `Completed` so callers can detect the anomaly.
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
///
/// Overwrites any existing record for `contract_id`. Callers that need
/// idempotency must check [`get_dispute_storage_version`] first.
pub fn store_dispute_metadata(env: &Env, contract_id: u32, metadata: &DisputeMetadata) {
    if metadata.schema_version != DISPUTE_STORAGE_VERSION {
        env.panic_with_error(Error::InvalidState);
    }
    validate_dispute_reason_hash_len(metadata.reason_hash.len())
        .unwrap_or_else(|error| env.panic_with_error(error));
    let storage = env.storage().persistent();
    storage.set(&DataKey::Dispute(contract_id), metadata);
    storage.set(
        &DataKey::DisputeStorageVersion(contract_id),
        &DISPUTE_STORAGE_VERSION,
    );
}

/// Remove dispute metadata for a contract.
///
/// Safe to call when no record exists; the operation is a no-op in that case.
pub fn clear_dispute_metadata(env: &Env, contract_id: u32) {
    let storage = env.storage().persistent();
    storage.remove(&DataKey::Dispute(contract_id));
    storage.remove(&DataKey::DisputeStorageVersion(contract_id));
}

/// Return the schema version of the stored dispute metadata, or 0 if none exists.
///
/// Returns `0` when no record is present, and [`DISPUTE_STORAGE_VERSION`]
/// otherwise. This is the canonical way to detect the presence of a dispute.
pub fn get_dispute_storage_version(env: &Env, contract_id: u32) -> u32 {
    let storage = env.storage().persistent();
    let stored_version: Option<u32> = storage.get(&DataKey::DisputeStorageVersion(contract_id));
    if let Some(version) = stored_version {
        return version;
    }
    if let Some(metadata) = storage.get::<_, DisputeMetadata>(&DataKey::Dispute(contract_id)) {
        return metadata.schema_version;
    }
    // A v0 payload has no version marker and must remain detectable as legacy.
    if storage
        .get::<_, DisputeMetadataV0>(&DataKey::Dispute(contract_id))
        .is_some()
    {
        return 0;
    }
    0
}

/// Read dispute metadata with automatic v0 → v1 migration.
///
/// Panics with `DisputeNotFound` when no record exists.
///
/// Boundary: a stored `schema_version` strictly greater than
/// [`DISPUTE_STORAGE_VERSION`] is rejected with [`Error::InvalidState`] rather
/// than silently downgraded, so forward-incompatible records cannot be
/// misinterpreted.
pub fn load_dispute_metadata(env: &Env, contract_id: u32) -> DisputeMetadata {
    if get_dispute_storage_version(env, contract_id) > DISPUTE_STORAGE_VERSION {
        env.panic_with_error(Error::InvalidState);
    }
    if let Some(meta) = env
        .storage()
        .persistent()
        .get::<_, DisputeMetadata>(&DataKey::Dispute(contract_id))
    {
        let marker: Option<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::DisputeStorageVersion(contract_id));
        if marker.is_some_and(|version| version != meta.schema_version) {
            env.panic_with_error(Error::InvalidState);
        }
        if meta.schema_version != DISPUTE_STORAGE_VERSION {
            env.panic_with_error(Error::InvalidState);
        }
        validate_dispute_reason_hash_len(meta.reason_hash.len())
            .unwrap_or_else(|error| env.panic_with_error(error));
        return meta;
    }
    // Try v0 → v1 migration
    if let Some(v0) = env
        .storage()
        .persistent()
        .get::<_, DisputeMetadataV0>(&DataKey::Dispute(contract_id))
    {
        let marker: Option<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::DisputeStorageVersion(contract_id));
        if marker.is_some_and(|version| version != 0) {
            env.panic_with_error(Error::InvalidState);
        }
        let v1 = migrate_dispute_metadata_v0_to_v1(v0);
        store_dispute_metadata(env, contract_id, &v1);
        return v1;
    }

    env.panic_with_error(Error::DisputeNotFound)
}

/// Migrate a v0 metadata record to the current schema version.
///
/// Boundary: `v0.raised_at` and `v0.reason_hash` are preserved verbatim; only
/// `schema_version` is rewritten to [`DISPUTE_STORAGE_VERSION`]. Callers must
/// validate `reason_hash` length via [`validate_dispute_reason_hash_len`]
/// before invoking this migration.
pub fn migrate_dispute_metadata_v0_to_v1(v0: DisputeMetadataV0) -> DisputeMetadata {
    DisputeMetadata {
        schema_version: DISPUTE_STORAGE_VERSION,
        raised_by: v0.raised_by,
        reason_hash: v0.reason_hash,
        raised_at: v0.raised_at,
    }
}
