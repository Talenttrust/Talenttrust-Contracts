//! Bounds validation for storage entrypoint inputs.
//! Bounds validation for storage entrypoint inputs.
//!
//! This module extracts numeric and length bound checks for storage-mutating
//! entrypoints into a single source of truth. Each function validates one
//! logical parameter and panics with the appropriate typed [`Error`] /
//! [`EscrowError`] on rejection.
//!
//! All functions are **pure** (no side-effects) and intended to be called at
//! the top of the corresponding entrypoint, before any state mutation occurs.
//!
//! # Compatibility contract
//!
//! Every function in this module forms part of the **public compatibility
//! contract** for its callers. The contract covers:
//!
//! 1. **Accepted range** — values that must never be rejected.
//! 2. **Rejected range** — values that must always panic.
//! 3. **Error identity** — the exact [`Error`] / [`EscrowError`] variant that
//!    must be emitted on rejection. Callers, indexers, and client SDKs that
//!    decode on-chain error codes depend on this identity. Changing the variant
//!    without a migration plan is a **breaking change**.
//! 4. **Upgrade stability** — the constants used to compute bounds
//!    ([`MAX_FEE_BPS`], [`MAX_MILESTONES`], etc.) are referenced by governance
//!    and `get_bounds()`; changing them changes the accepted range and must be
//!    treated as a protocol-level governance action, not a code-only edit.
//!
//! The [`test::storage_validation_compat`] module pins all four properties
//! with typed assertions so any regression is caught at compile-or-test time.
//!
//! # Failure modes and observability
//!
//! All rejections call [`Env::panic_with_error`], which causes Soroban to
//! surface a typed `u32` error code in the transaction result. Callers should
//! never receive a silent `false` or a misleading "success" response for an
//! out-of-bounds input — the transaction is aborted with the error code.
//!
//! This design means:
//! - Partial execution is impossible: a rejection at a validation boundary
//!   rolls back any preceding reads (there are none — validation is pure).
//! - Concurrent retries are safe: a deterministic rejection is idempotent.
//! - Upgrades are safe: adding a new validated field to an entrypoint requires
//!   adding a new `validate_*` call and a corresponding `compat` test.

use crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS;
use crate::milestones_consts::{
    MAX_FEE_BPS, MAX_MILESTONES, MAX_RATING, MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING,
    MAX_REPUTATION_CONFIG_RATING_CEILING, MIN_COMMENT_BYTES, MIN_RATING,
};
use crate::types::{Contract, DataKey, Milestone};
use crate::{Error, EscrowError};
use soroban_sdk::panic_with_error;
use soroban_sdk::{Env, Vec};

// ── validate_escrow_total_cap ────────────────────────────────────────────────

/// Validate the governed total escrow cap in stroops.
///
/// ## Compatibility contract
///
/// | Property        | Value                        |
/// |-----------------|------------------------------|
/// | Accepted range  | `(0, i128::MAX]` (inclusive) |
/// | Rejected range  | `(-∞, 0]`                    |
/// | Error on reject | [`Error::InvalidProtocolParameters`] |
/// | Constant used   | none (hardcoded `> 0` check) |
///
/// The `> 0` invariant is load-bearing: a zero cap would block every call to
/// `create_contract` by making every milestone total exceed the limit.
/// Callers that read back `max_escrow_total_stroops` via `get_governed_parameters`
/// can rely on the returned value always being positive.
///
/// ## Upgrade note
///
/// This check must stay as `<= 0` → reject. If a future version needs to allow
/// zero (e.g., to represent "no limit"), that is a governance-visible protocol
/// change and must update the stored value, the documentation, and the
/// `storage_validation_compat` test that pins this boundary.
///
/// # Panics
/// Panics with [`Error::InvalidProtocolParameters`] when `max_escrow_total_stroops <= 0`.
pub(crate) fn validate_escrow_total_cap(env: &Env, max_escrow_total_stroops: i128) {
    validate_escrow_total_cap_value(max_escrow_total_stroops)
        .unwrap_or_else(|err| env.panic_with_error(err));
}

/// Pure form of [`validate_escrow_total_cap`]. Callers that can recover from
/// invalid configuration should use this form before mutating storage.
pub(crate) fn validate_escrow_total_cap_value(value: i128) -> Result<(), Error> {
    (value > 0)
        .then_some(())
        .ok_or(Error::InvalidProtocolParameters)
}

// ── validate_reputation_config_params ────────────────────────────────────────

/// Validate reputation configuration parameters.
///
/// ## Compatibility contract
///
/// | Parameter         | Accepted range                                  | Error on reject                       |
/// |-------------------|-------------------------------------------------|---------------------------------------|
/// | `min_rating`      | `[MIN_RATING, MAX_REPUTATION_CONFIG_RATING_CEILING]` i.e. `[1, 10]` | [`Error::InvalidProtocolParameters`] |
/// | `max_rating`      | `[min_rating, MAX_REPUTATION_CONFIG_RATING_CEILING]` i.e. `[min_rating, 10]` | [`Error::InvalidProtocolParameters`] |
/// | `max_comment_bytes` | `[MIN_COMMENT_BYTES, MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING]` i.e. `[1, 1_000]` | [`Error::InvalidProtocolParameters`] |
///
/// All three parameters are validated atomically in a single call; the first
/// violation found (checked left-to-right: `min_rating`, `max_rating < min`,
/// `max_rating > ceiling`, `max_comment_bytes < MIN`, `max_comment_bytes > MAX`)
/// determines the error. Future callers that depend on this ordering must be
/// updated if the check order changes.
///
/// The single-error response means callers cannot distinguish which of the
/// three parameters was invalid. This is intentional: it avoids revealing
/// partial configuration to unauthenticated readers. A governance UI should
/// perform local pre-validation before submitting.
///
/// ## Upgrade note
///
/// Increasing `MAX_REPUTATION_CONFIG_RATING_CEILING` or
/// `MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING` widens the accepted range
/// without breaking existing stored values. Decreasing either constant narrows
/// the range and may reject previously accepted stored configurations on their
/// next update — treat as a breaking change.
///
/// # Boundary behavior
/// * `min_rating == max_rating` is accepted (single-value range).
/// * `max_comment_bytes == 1` and `max_comment_bytes == 1_000` are accepted.
/// * `min_rating == 0`, `max_rating < min_rating`, `max_rating > 10`,
///   `max_comment_bytes == 0`, and `max_comment_bytes > 1_000` are rejected.
///
/// # Panics
/// Panics with [`Error::InvalidProtocolParameters`] when any bound is violated.
///
/// # Invariants
/// * `MIN_RATING <= min_rating <= max_rating <= MAX_REPUTATION_CONFIG_RATING_CEILING`,
///   so the accepted rating window is never empty and never exceeds the
///   protocol ceiling.
/// * `MIN_COMMENT_BYTES <= max_comment_bytes <= MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING`.
pub(crate) fn validate_reputation_config_params(
    env: &Env,
    min_rating: u32,
    max_rating: u32,
    max_comment_bytes: u32,
) {
    validate_reputation_config_params_value(min_rating, max_rating, max_comment_bytes)
        .unwrap_or_else(|err| env.panic_with_error(err));
}

pub(crate) fn validate_reputation_config_params_value(
    min_rating: u32,
    max_rating: u32,
    max_comment_bytes: u32,
) -> Result<(), Error> {
    if min_rating < MIN_RATING
        || max_rating < min_rating
        || max_rating > MAX_REPUTATION_CONFIG_RATING_CEILING
        || max_comment_bytes < MIN_COMMENT_BYTES
        || max_comment_bytes > MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING
    {
        return Err(Error::InvalidProtocolParameters);
    }
    Ok(())
}

// ── validate_milestone_count ──────────────────────────────────────────────────

/// Validate the number of milestones for a contract creation call.
///
/// ## Compatibility contract
///
/// | Value           | Result                                          | Error code                           |
/// |-----------------|-------------------------------------------------|--------------------------------------|
/// | `0`             | Rejected                                        | [`EscrowError::EmptyMilestones`]     |
/// | `1`             | Accepted (minimum)                             | —                                    |
/// | `MAX_MILESTONES`| Accepted (maximum, currently 10)               | —                                    |
/// | `> MAX_MILESTONES` | Rejected                                    | [`EscrowError::TooManyMilestones`]   |
/// | `u32::MAX`      | Rejected                                        | [`EscrowError::TooManyMilestones`]   |
///
/// **Two distinct error codes** are used here intentionally. Client code and
/// indexers that discriminate between "caller sent an empty list" and "caller
/// sent too many milestones" depend on this distinction. Both error codes must
/// be preserved through refactors. Merging them into a single
/// `InvalidMilestoneCount` would be a **breaking change**.
///
/// ## Upgrade note
///
/// `MAX_MILESTONES` is `10`. Increasing it (e.g. to `20`) is safe for existing
/// contracts but affects gas / transaction-size budgets. Decreasing it would
/// reject contracts that are valid today. Either direction requires a governance
/// proposal, a `get_bounds()` update, and a corresponding compat test update.
///
/// # Boundary behavior
/// * `1` and `MAX_MILESTONES` are accepted.
/// * `0`, `MAX_MILESTONES + 1`, and `u32::MAX` are rejected.
///
/// # Panics
/// * [`EscrowError::EmptyMilestones`] when `count == 0`.
/// * [`EscrowError::TooManyMilestones`] when `count > MAX_MILESTONES`.
pub(crate) fn validate_milestone_count(env: &Env, count: u32) {
    validate_milestone_count_value(count).unwrap_or_else(|err| env.panic_with_error(err));
}

pub(crate) fn validate_milestone_count_value(count: u32) -> Result<(), EscrowError> {
    if count == 0 {
        return Err(EscrowError::EmptyMilestones);
    }
    if count > MAX_MILESTONES {
        return Err(EscrowError::TooManyMilestones);
    }
    Ok(())
}

// ── validate_protocol_fee_bps ─────────────────────────────────────────────────

/// Validate a protocol fee basis-points value.
///
/// ## Compatibility contract
///
/// | Value             | Result   | Error code                               |
/// |-------------------|----------|------------------------------------------|
/// | `0`               | Accepted | — (zero fee disables collection)         |
/// | `MAX_FEE_BPS` (10_000) | Accepted | —                                   |
/// | `MAX_FEE_BPS + 1` | Rejected | [`Error::InvalidProtocolParameters`]     |
/// | `u32::MAX`        | Rejected | [`Error::InvalidProtocolParameters`]     |
///
/// Zero is explicitly allowed: it disables protocol-fee collection. This is an
/// intentional governance lever — callers that observe `0` from
/// `get_protocol_fee_bps()` must treat it as a fee-free mode, not an
/// uninitialised value.
///
/// `MAX_FEE_BPS == PROTOCOL_FEE_BPS_DENOMINATOR == 10_000`. The invariant
/// `fee ≤ denominator` ensures the net amount transferred to the freelancer is
/// always non-negative.
///
/// ## Upgrade note
///
/// `MAX_FEE_BPS` equals the basis-point denominator (10_000 = 100%). Raising
/// it above the denominator would allow a fee that exceeds the milestone amount
/// — this is explicitly prohibited. Lowering it (e.g., to cap fees at 50%)
/// would narrow the accepted range, is a breaking governance change, and must
/// come with a migration for stored `ProtocolFeeBps` values that exceed the new
/// maximum.
///
/// # Boundary behavior
/// * `0` and `MAX_FEE_BPS` are accepted.
/// * `MAX_FEE_BPS + 1` and `u32::MAX` are rejected.
///
/// # Panics
/// Panics with [`Error::InvalidProtocolParameters`] when `bps > MAX_FEE_BPS`.
///
/// # Invariants
/// * `bps <= MAX_FEE_BPS`, so fee arithmetic cannot exceed the total amount
///   and the payout invariant `net + fee == gross` holds.
pub(crate) fn validate_protocol_fee_bps(env: &Env, bps: u32) {
    validate_protocol_fee_bps_value(bps).unwrap_or_else(|err| env.panic_with_error(err));
}

pub(crate) fn validate_protocol_fee_bps_value(bps: u32) -> Result<(), Error> {
    (bps <= MAX_FEE_BPS)
        .then_some(())
        .ok_or(Error::InvalidProtocolParameters)
}

// ── validate_stroop_amount ────────────────────────────────────────────────────

/// Validate a single stroop amount for positivity and maximum bounds.
///
/// ## Compatibility contract
///
/// | Value                             | Result   | Error code                               |
/// |-----------------------------------|----------|------------------------------------------|
/// | `1` (1 stroop)                    | Accepted | —                                        |
/// | `MAX_SINGLE_AMOUNT_STROOPS`       | Accepted | —                                        |
/// | `0`                               | Rejected | [`EscrowError::AmountMustBePositive`]    |
/// | `-1`                              | Rejected | [`EscrowError::AmountMustBePositive`]    |
/// | `i128::MIN`                       | Rejected | [`EscrowError::AmountMustBePositive`]    |
/// | `MAX_SINGLE_AMOUNT_STROOPS + 1`   | Rejected | [`EscrowError::InvalidMilestoneAmount`] |
/// | `i128::MAX`                       | Rejected | [`EscrowError::InvalidMilestoneAmount`] |
///
/// **Two distinct error codes** are used here intentionally:
/// - `AmountMustBePositive` — amount is ≤ 0 (caller submitted a non-positive value).
/// - `InvalidMilestoneAmount` — amount is positive but exceeds the per-operation cap.
///
/// Callers and indexers that distinguish between "bad sign" and "too large"
/// depend on this distinction. Merging them would be a **breaking change**.
///
/// ## Upgrade note
///
/// `MAX_SINGLE_AMOUNT_STROOPS` is currently `1_000_000_0000000` (1 M tokens at
/// 7 decimal places). Increasing it widens the accepted range; decreasing it
/// narrows it and may reject deposits that were valid at contract-creation time.
/// Any change to this constant must be coordinated with `get_bounds()` and the
/// governance process.
///
/// # Boundary behavior
/// * `1` and `MAX_SINGLE_AMOUNT_STROOPS` are accepted.
/// * `0`, `-1`, and `MAX_SINGLE_AMOUNT_STROOPS + 1` are rejected.
///
/// # Panics
/// * [`EscrowError::AmountMustBePositive`] when `amount <= 0`.
/// * [`EscrowError::InvalidMilestoneAmount`] when `amount > MAX_SINGLE_AMOUNT_STROOPS`.
pub(crate) fn validate_stroop_amount(env: &Env, amount: i128) {
    validate_stroop_amount_value(amount).unwrap_or_else(|err| env.panic_with_error(err));
}

pub(crate) fn validate_stroop_amount_value(amount: i128) -> Result<(), EscrowError> {
    if amount <= 0 {
        return Err(EscrowError::AmountMustBePositive);
    }
    if amount > MAX_SINGLE_AMOUNT_STROOPS {
        return Err(EscrowError::InvalidMilestoneAmount);
    }
    Ok(())
}

// ── Concurrent-execution hardening (issue #1535) ─────────────────────────────
//
// The functions above validate *inputs* before a storage mutation starts.  The
// section below validates the *persisted state* those mutations produce and
// serialises mutation of a single contract so that re-entrant or interleaved
// execution can never observe or leave behind a partially-applied state.
//
// Soroban runs one transaction at a time and rolls every storage write back when
// a call panics, so a classic data race is impossible.  Two hazards remain:
//
// 1. **Re-entrancy** — a malicious settlement token can call back into the
//    escrow while a `transfer` is in flight.  The mutation lock turns any such
//    re-entrant mutation into a deterministic `ConcurrentMutation` failure
//    instead of letting it operate on half-updated state.
// 2. **Corrupt / stale state** — a persisted record that violates the accounting
//    or milestone invariants would otherwise be read, mutated and written back,
//    laundering the corruption into a record that looks valid.  The checked
//    accessors refuse to load or store such a record.

/// Ledgers a mutation-lock entry survives before the host evicts it.
///
/// One day (17 280 ledgers at ~5 s each).  The lock only ever lives for the
/// duration of a single entrypoint call: the TTL exists purely so an entry
/// orphaned by a host-level failure can never keep a contract permanently
/// unusable, because the *presence* of the entry is what blocks mutation.
///
pub const MUTATION_LOCK_TTL_LEDGERS: u32 = crate::ttl::LEDGERS_PER_DAY;

/// Storage key for the per-contract mutation lock.
pub(crate) fn mutation_lock_key(contract_id: u32) -> DataKey {
    DataKey::ContractMutationLock(contract_id)
}

/// Returns `true` when a storage mutation for `contract_id` is in flight.
pub(crate) fn is_contract_locked(env: &Env, contract_id: u32) -> bool {
    env.storage().persistent().has(&mutation_lock_key(contract_id))
}

/// RAII guard holding the per-contract mutation lock.
///
/// The guard owns a clone of the [`Env`] so the lock is released in [`Drop`] on
/// every exit path, including early returns.  A transaction that panics rolls
/// its storage writes back, so a trapped call cannot leak the lock either.
pub struct ContractMutationGuard {
    env: Env,
    contract_id: u32,
}

impl ContractMutationGuard {
    /// Acquires the mutation lock for `contract_id`.
    ///
    /// # Panics
    /// Panics with [`Error::ConcurrentMutation`] when the lock is already held,
    /// which on Soroban means a re-entrant or interleaved call for the same
    /// contract is already mutating state.
    pub fn acquire(env: &Env, contract_id: u32) -> Self {
        if is_contract_locked(env, contract_id) {
            env.panic_with_error(Error::ConcurrentMutation);
        }

        let key = mutation_lock_key(contract_id);
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, MUTATION_LOCK_TTL_LEDGERS, MUTATION_LOCK_TTL_LEDGERS);

        Self {
            env: env.clone(),
            contract_id,
        }
    }

    /// The contract whose mutation this guard serialises.
    pub fn contract_id(&self) -> u32 {
        self.contract_id
    }
}

/// Acquires the mutation lock for `contract_id` and returns the RAII guard.
///
/// Thin wrapper over [`ContractMutationGuard::acquire`] so entrypoints can bind
/// the guard with `let _mutation_guard = ...;`.
///
/// # Panics
/// Panics with [`Error::ConcurrentMutation`] when the lock is already held.
pub(crate) fn acquire_contract_mutation_lock(
    env: &Env,
    contract_id: u32,
) -> ContractMutationGuard {
    ContractMutationGuard::acquire(env, contract_id)
}

impl Drop for ContractMutationGuard {
    fn drop(&mut self) {
        release_contract_mutation_lock(&self.env, self.contract_id);
    }
}

/// Releases the mutation lock.
///
/// Idempotent: releasing an unlocked (or never locked) contract is a no-op, so
/// a guard that runs after a rolled-back transaction is always safe.
pub(crate) fn release_contract_mutation_lock(env: &Env, contract_id: u32) {
    env.storage()
        .persistent()
        .remove(&mutation_lock_key(contract_id));
}

/// Validates the accounting invariants of a persisted [`Contract`].
///
/// # Invariants
/// * `total_deposited`, `funded_amount`, `released_amount` and
///   `refunded_amount` are all non-negative.
/// * `released_amount + refunded_amount <= funded_amount` — a contract can never
///   have paid out more than it took in.  The sum is checked, so a corrupted
///   pair of maximum `i128`s fails here rather than overflowing.
///
/// # Panics
/// Panics with [`Error::StorageInvariantViolated`] on a violated invariant and
/// [`Error::PotentialOverflow`] when the sum cannot be represented.
pub(crate) fn validate_contract_accounting(env: &Env, contract: &Contract) {
    if contract.total_deposited < 0
        || contract.funded_amount < 0
        || contract.released_amount < 0
        || contract.refunded_amount < 0
    {
        env.panic_with_error(Error::StorageInvariantViolated);
    }

    let settled = contract
        .released_amount
        .checked_add(contract.refunded_amount)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

    if settled > contract.funded_amount {
        env.panic_with_error(Error::StorageInvariantViolated);
    }
}

/// Validates the invariants of a persisted [`Milestone`].
///
/// # Invariants
/// * `amount`, `funded_amount` and `refunded_amount` are all non-negative.
/// * `released` and `refunded` are mutually exclusive — a milestone is settled by
///   exactly one of the two flows.  This is the same rule
///   [`crate::milestone_transitions::MilestoneState::from_milestone`] enforces.
/// * A refunded milestone is refunded in full, so `refunded_amount == amount`.
///
/// # Panics
/// Panics with [`Error::StorageInvariantViolated`] on a violated invariant.
pub(crate) fn validate_milestone_consistency(env: &Env, milestone: &Milestone) {
    if milestone.amount < 0 || milestone.funded_amount < 0 || milestone.refunded_amount < 0 {
        env.panic_with_error(Error::StorageInvariantViolated);
    }

    if milestone.released && milestone.refunded {
        env.panic_with_error(Error::StorageInvariantViolated);
    }

    if milestone.refunded && milestone.refunded_amount != milestone.amount {
        env.panic_with_error(Error::StorageInvariantViolated);
    }
}

/// Validates every milestone in a persisted vector.
///
/// # Panics
/// Panics with [`Error::StorageInvariantViolated`] on the first milestone that
/// violates an invariant.
pub(crate) fn validate_milestones_consistency(env: &Env, milestones: &Vec<Milestone>) {
    for milestone in milestones.iter() {
        validate_milestone_consistency(env, &milestone);
    }
}

/// Loads a contract and validates it before it can be mutated.
///
/// # Panics
/// Panics with [`Error::ContractNotFound`] when the record is absent and with
/// [`Error::StorageInvariantViolated`] when it is present but inconsistent.
pub(crate) fn load_contract_checked(env: &Env, contract_id: u32) -> Contract {
    let contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    validate_contract_accounting(env, &contract);
    contract
}

/// Validates a contract and persists it, so an inconsistent record can never be
/// written to storage.
///
/// # Panics
/// Panics with [`Error::StorageInvariantViolated`] when `contract` violates an
/// accounting invariant; storage is left untouched in that case.
pub(crate) fn store_contract_checked(env: &Env, contract_id: u32, contract: &Contract) {
    validate_contract_accounting(env, contract);
    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), contract);
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    fn env() -> Env {
        Env::default()
    }

    // ── validate_escrow_total_cap ────────────────────────────────────────────

    /// Minimum positive value (1 stroop) is accepted.
    #[test]
    fn validate_escrow_total_cap_accepts_1() {
        let e = env();
        validate_escrow_total_cap(&e, 1);
    }

    /// i128::MAX is accepted (no upper bound on cap).
    #[test]
    fn validate_escrow_total_cap_accepts_i128_max() {
        let e = env();
        validate_escrow_total_cap(&e, i128::MAX);
    }

    /// Typical governance value is accepted.
    #[test]
    fn validate_escrow_total_cap_accepts_typical() {
        let e = env();
        validate_escrow_total_cap(&e, 1_000_000_0000000_i128);
    }

    /// Zero cap is rejected (would block every contract creation).
    #[test]
    #[should_panic]
    fn validate_escrow_total_cap_rejects_zero() {
        let e = env();
        validate_escrow_total_cap(&e, 0);
    }

    /// -1 is rejected.
    #[test]
    #[should_panic]
    fn validate_escrow_total_cap_rejects_negative() {
        let e = env();
        validate_escrow_total_cap(&e, -1);
    }

    /// i128::MIN is rejected.
    #[test]
    #[should_panic]
    fn validate_escrow_total_cap_rejects_i128_min() {
        let e = env();
        validate_escrow_total_cap(&e, i128::MIN);
    }

    #[test]
    fn pure_validators_report_recoverable_errors_without_storage_mutation() {
        assert_eq!(
            validate_escrow_total_cap_value(0),
            Err(Error::InvalidProtocolParameters)
        );
        assert_eq!(
            validate_protocol_fee_bps_value(MAX_FEE_BPS + 1),
            Err(Error::InvalidProtocolParameters)
        );
        assert_eq!(
            validate_milestone_count_value(0),
            Err(EscrowError::EmptyMilestones)
        );
        assert_eq!(
            validate_stroop_amount_value(-1),
            Err(EscrowError::AmountMustBePositive)
        );
    }

    // ── validate_reputation_config_params ─────────────────────────────────────

    /// Default config (1, 5, 200) is accepted.
    #[test]
    fn validate_reputation_config_params_accepts_default() {
        let e = env();
        validate_reputation_config_params(&e, 1, 5, 200);
    }

    /// Degenerate range (min == max rating) is accepted.
    #[test]
    fn validate_reputation_config_params_accepts_min_equal_max_rating() {
        let e = env();
        validate_reputation_config_params(&e, 3, 3, 1);
    }

    /// Maximum allowed comment bytes is accepted.
    #[test]
    fn validate_reputation_config_params_accepts_max_comment_1000() {
        let e = env();
        validate_reputation_config_params(&e, 1, 10, 1_000);
    }

    /// max_rating == MAX_REPUTATION_CONFIG_RATING_CEILING is accepted.
    #[test]
    fn validate_reputation_config_params_accepts_max_rating_ceiling() {
        let e = env();
        validate_reputation_config_params(&e, 1, MAX_REPUTATION_CONFIG_RATING_CEILING, 200);
    }

    /// min_rating == 0 is rejected.
    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_zero_min_rating() {
        let e = env();
        validate_reputation_config_params(&e, 0, 5, 200);
    }

    /// max_rating < min_rating is rejected.
    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_max_below_min() {
        let e = env();
        validate_reputation_config_params(&e, 5, 3, 200);
    }

    /// max_rating > MAX_REPUTATION_CONFIG_RATING_CEILING is rejected.
    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_max_rating_over_ceiling() {
        let e = env();
        validate_reputation_config_params(&e, 1, MAX_REPUTATION_CONFIG_RATING_CEILING + 1, 200);
    }

    /// max_comment_bytes == 0 is rejected.
    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_zero_comment_bytes() {
        let e = env();
        validate_reputation_config_params(&e, 1, 5, 0);
    }

    /// max_comment_bytes > MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING is rejected.
    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_comment_over_ceiling() {
        let e = env();
        validate_reputation_config_params(&e, 1, 5, MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING + 1);
    }

    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_min_rating_over_ceiling() {
        let e = env();
        validate_reputation_config_params(&e, 11, 11, 200);
    }

    // ── validate_milestone_count ──────────────────────────────────────────────

    /// Minimum accepted count (1) is accepted.
    #[test]
    fn validate_milestone_count_accepts_1() {
        let e = env();
        validate_milestone_count(&e, 1);
    }

    /// Maximum accepted count is accepted.
    #[test]
    fn validate_milestone_count_accepts_max() {
        let e = env();
        validate_milestone_count(&e, MAX_MILESTONES);
    }

    /// count == 0 is rejected.
    #[test]
    #[should_panic]
    fn validate_milestone_count_rejects_zero() {
        let e = env();
        validate_milestone_count(&e, 0);
    }

    /// count == MAX_MILESTONES + 1 is rejected.
    #[test]
    #[should_panic]
    fn validate_milestone_count_rejects_over_max() {
        let e = env();
        validate_milestone_count(&e, MAX_MILESTONES + 1);
    }

    /// u32::MAX is rejected.
    #[test]
    #[should_panic]
    fn validate_milestone_count_rejects_u32_max() {
        let e = env();
        validate_milestone_count(&e, u32::MAX);
    }

    // ── validate_protocol_fee_bps ─────────────────────────────────────────────

    /// 0 bps (fee disabled) is accepted.
    #[test]
    fn validate_protocol_fee_bps_accepts_zero() {
        let e = env();
        validate_protocol_fee_bps(&e, 0);
    }

    /// Exactly MAX_FEE_BPS is accepted.
    #[test]
    fn validate_protocol_fee_bps_accepts_max() {
        let e = env();
        validate_protocol_fee_bps(&e, MAX_FEE_BPS);
    }

    /// A typical 2.5% fee (250 bps) is accepted.
    #[test]
    fn validate_protocol_fee_bps_accepts_typical() {
        let e = env();
        validate_protocol_fee_bps(&e, 250);
    }

    /// MAX_FEE_BPS + 1 is rejected.
    #[test]
    #[should_panic]
    fn validate_protocol_fee_bps_rejects_over_max() {
        let e = env();
        validate_protocol_fee_bps(&e, MAX_FEE_BPS + 1);
    }

    /// u32::MAX is rejected.
    #[test]
    #[should_panic]
    fn validate_protocol_fee_bps_rejects_u32_max() {
        let e = env();
        validate_protocol_fee_bps(&e, u32::MAX);
    }

    #[test]
    #[should_panic]
    fn validate_protocol_fee_bps_rejects_max_plus_two() {
        let e = env();
        validate_protocol_fee_bps(&e, MAX_FEE_BPS + 2);
    }

    // ── validate_stroop_amount ────────────────────────────────────────────────

    /// 1 stroop (minimum) is accepted.
    #[test]
    fn validate_stroop_amount_accepts_1() {
        let e = env();
        validate_stroop_amount(&e, 1);
    }

    /// MAX_SINGLE_AMOUNT_STROOPS is accepted.
    #[test]
    fn validate_stroop_amount_accepts_max() {
        let e = env();
        validate_stroop_amount(&e, crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS);
    }

    /// A typical milestone amount is accepted.
    #[test]
    fn validate_stroop_amount_accepts_typical() {
        let e = env();
        validate_stroop_amount(&e, 200_0000000_i128);
    }

    /// 0 is rejected.
    #[test]
    #[should_panic]
    fn validate_stroop_amount_rejects_zero() {
        let e = env();
        validate_stroop_amount(&e, 0);
    }

    /// -1 is rejected.
    #[test]
    #[should_panic]
    fn validate_stroop_amount_rejects_negative() {
        let e = env();
        validate_stroop_amount(&e, -1);
    }

    /// i128::MIN is rejected.
    #[test]
    #[should_panic]
    fn validate_stroop_amount_rejects_i128_min() {
        let e = env();
        validate_stroop_amount(&e, i128::MIN);
    }

    /// MAX_SINGLE_AMOUNT_STROOPS + 1 is rejected.
    #[test]
    #[should_panic]
    fn validate_stroop_amount_rejects_over_max() {
        let e = env();
        validate_stroop_amount(&e, crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS + 1);
    }

    /// i128::MAX is rejected.
    #[test]
    #[should_panic]
    fn validate_stroop_amount_rejects_i128_max() {
        let e = env();
        validate_stroop_amount(&e, i128::MAX);
    }
}

/// Tests for the concurrent-execution hardening (issue #1535): the per-contract
/// mutation lock plus the checked load/store accessors.
#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use crate::types::{ContractStatus, ReleaseAuthorization};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Address;

    /// A contract record satisfying every accounting invariant.
    fn healthy_contract(env: &Env) -> Contract {
        Contract {
            client: Address::generate(env),
            freelancer: Address::generate(env),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1_000,
            funded_amount: 1_000,
            released_amount: 400,
            refunded_amount: 100,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        }
    }

    /// An unreleased, unrefunded milestone.
    fn pending_milestone() -> Milestone {
        Milestone {
            amount: 500,
            funded_amount: 0,
            released: false,
            refunded: false,
            work_evidence: None,
            refunded_amount: 0,
            deadline: None,
        }
    }

    /// A registered escrow contract address, so storage handles are valid.
    fn registered(env: &Env) -> Address {
        env.register(crate::Escrow, ())
    }

    // ── mutation lock ────────────────────────────────────────────────────────

    #[test]
    fn lock_is_not_held_before_acquire() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            assert!(!is_contract_locked(&env, 7));
        });
    }

    #[test]
    fn guard_releases_lock_when_dropped() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            {
                let guard = ContractMutationGuard::acquire(&env, 7);
                assert_eq!(guard.contract_id(), 7);
                assert!(is_contract_locked(&env, 7));
            }
            // Drop ran, so a later mutation can acquire the lock again.
            assert!(!is_contract_locked(&env, 7));
            let _second = ContractMutationGuard::acquire(&env, 7);
        });
    }

    #[test]
    #[should_panic]
    fn reentrant_acquire_is_rejected() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            let _first = ContractMutationGuard::acquire(&env, 7);
            // Simulates a token callback re-entering the escrow mid-transfer.
            let _reentrant = ContractMutationGuard::acquire(&env, 7);
        });
    }

    #[test]
    fn locks_are_scoped_per_contract() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            let _one = ContractMutationGuard::acquire(&env, 1);
            // A different contract is not blocked by the first contract's lock.
            let _two = ContractMutationGuard::acquire(&env, 2);
            assert!(is_contract_locked(&env, 1));
            assert!(is_contract_locked(&env, 2));
        });
    }

    #[test]
    fn release_is_idempotent() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            release_contract_mutation_lock(&env, 9);
            release_contract_mutation_lock(&env, 9);
            assert!(!is_contract_locked(&env, 9));
        });
    }

    // ── contract accounting invariants ───────────────────────────────────────

    #[test]
    fn validate_contract_accounting_accepts_healthy() {
        let env = Env::default();
        validate_contract_accounting(&env, &healthy_contract(&env));
    }

    #[test]
    fn validate_contract_accounting_accepts_fully_settled() {
        let env = Env::default();
        let mut contract = healthy_contract(&env);
        contract.released_amount = 900;
        contract.refunded_amount = 100;
        validate_contract_accounting(&env, &contract);
    }

    #[test]
    #[should_panic]
    fn validate_contract_accounting_rejects_oversettled() {
        let env = Env::default();
        let mut contract = healthy_contract(&env);
        contract.funded_amount = 100;
        contract.released_amount = 90;
        contract.refunded_amount = 20;
        validate_contract_accounting(&env, &contract);
    }

    #[test]
    #[should_panic]
    fn validate_contract_accounting_rejects_negative_amount() {
        let env = Env::default();
        let mut contract = healthy_contract(&env);
        contract.refunded_amount = -1;
        validate_contract_accounting(&env, &contract);
    }

    #[test]
    #[should_panic]
    fn validate_contract_accounting_rejects_overflowing_settlement() {
        let env = Env::default();
        let mut contract = healthy_contract(&env);
        contract.funded_amount = i128::MAX;
        contract.released_amount = i128::MAX;
        contract.refunded_amount = i128::MAX;
        validate_contract_accounting(&env, &contract);
    }

    // ── milestone invariants ─────────────────────────────────────────────────

    #[test]
    fn validate_milestone_consistency_accepts_pending() {
        let env = Env::default();
        validate_milestone_consistency(&env, &pending_milestone());
    }

    #[test]
    fn validate_milestone_consistency_accepts_released() {
        let env = Env::default();
        let mut milestone = pending_milestone();
        milestone.released = true;
        milestone.funded_amount = milestone.amount;
        validate_milestone_consistency(&env, &milestone);
    }

    #[test]
    fn validate_milestone_consistency_accepts_refunded() {
        let env = Env::default();
        let mut milestone = pending_milestone();
        milestone.refunded = true;
        milestone.refunded_amount = milestone.amount;
        validate_milestone_consistency(&env, &milestone);
    }

    #[test]
    #[should_panic]
    fn validate_milestone_consistency_rejects_both_flags() {
        let env = Env::default();
        let mut milestone = pending_milestone();
        milestone.released = true;
        milestone.funded_amount = milestone.amount;
        milestone.refunded = true;
        milestone.refunded_amount = milestone.amount;
        validate_milestone_consistency(&env, &milestone);
    }

    #[test]
    #[should_panic]
    fn validate_milestone_consistency_rejects_partial_refund() {
        let env = Env::default();
        let mut milestone = pending_milestone();
        milestone.refunded = true;
        milestone.refunded_amount = milestone.amount - 1;
        validate_milestone_consistency(&env, &milestone);
    }

    #[test]
    #[should_panic]
    fn validate_milestone_consistency_rejects_negative_amount() {
        let env = Env::default();
        let mut milestone = pending_milestone();
        milestone.amount = -1;
        validate_milestone_consistency(&env, &milestone);
    }

    #[test]
    #[should_panic]
    fn validate_milestones_consistency_rejects_any_bad_entry() {
        let env = Env::default();
        let mut milestones = Vec::new(&env);
        milestones.push_back(pending_milestone());
        let mut corrupt = pending_milestone();
        corrupt.released = true;
        corrupt.funded_amount = corrupt.amount;
        corrupt.refunded = true;
        corrupt.refunded_amount = corrupt.amount;
        milestones.push_back(corrupt);
        validate_milestones_consistency(&env, &milestones);
    }

    #[test]
    fn validate_milestones_consistency_accepts_healthy_vector() {
        let env = Env::default();
        let mut milestones = Vec::new(&env);
        milestones.push_back(pending_milestone());
        let mut released = pending_milestone();
        released.released = true;
        released.funded_amount = released.amount;
        milestones.push_back(released);
        validate_milestones_consistency(&env, &milestones);
    }

    // ── checked accessors ────────────────────────────────────────────────────

    #[test]
    fn load_contract_checked_accepts_healthy() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(7), &healthy_contract(&env));
            let loaded = load_contract_checked(&env, 7);
            assert_eq!(loaded.funded_amount, 1_000);
        });
    }

    #[test]
    #[should_panic]
    fn load_contract_checked_rejects_corrupt_record() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            let mut corrupt = healthy_contract(&env);
            corrupt.funded_amount = 10;
            corrupt.released_amount = 1_000;
            env.storage()
                .persistent()
                .set(&DataKey::Contract(7), &corrupt);
            let _ = load_contract_checked(&env, 7);
        });
    }

    #[test]
    #[should_panic]
    fn load_contract_checked_rejects_absent_record() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            let _ = load_contract_checked(&env, 7);
        });
    }

    #[test]
    fn store_contract_checked_persists_healthy_record() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            store_contract_checked(&env, 7, &healthy_contract(&env));
            assert!(env.storage().persistent().has(&DataKey::Contract(7)));
        });
    }

    #[test]
    #[should_panic]
    fn store_contract_checked_rejects_corrupt_record() {
        let env = Env::default();
        let id = registered(&env);
        env.as_contract(&id, || {
            let mut corrupt = healthy_contract(&env);
            corrupt.refunded_amount = corrupt.funded_amount + 1;
            // Validation runs before the write, so the panic must happen and the
            // corrupt record must never reach storage.
            store_contract_checked(&env, 7, &corrupt);
        });
    }
}
