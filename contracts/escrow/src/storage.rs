//! Centralized storage precondition checks and contract loading helpers.
//!
//! This module extracts repeated storage validation patterns into a single source
//! of truth, ensuring consistent error handling and reducing code duplication across
//! entrypoints. All contract loading operations should route through these helpers.
//!
//! ## Deterministic failure recovery
//!
//! A persistent read has exactly three outcomes, and every helper here maps
//! them to a fixed, documented contract error instead of letting the host trap:
//!
//! | Outcome                                  | [`StorageRead`] | Error surfaced by loaders            |
//! |------------------------------------------|-----------------|--------------------------------------|
//! | Entry present and decodes as the type    | `Present(v)`    | —                                    |
//! | Entry absent                             | `Missing`       | entity-specific (`ContractNotFound`) |
//! | Entry present but does not decode        | `Corrupt`       | [`Error::StorageInvariantViolated`]  |
//!
//! Soroban storage reads are deterministic within a ledger, so a failed read is
//! never *retried*: retrying would observe the same bytes and only burn budget.
//! Recovery instead means the failure is **typed and non-destructive**:
//!
//! * A plain `.get::<_, T>()` on an entry whose bytes do not decode as `T`
//!   aborts the invocation with a host conversion error that carries no
//!   contract error code. [`read_persistent`] reads the raw `Val` first and
//!   decodes it with `try_from_val`, so a malformed entry surfaces as
//!   `StorageInvariantViolated` — a stable code clients and indexers can match.
//! * No helper ever overwrites, removes, or "repairs" a malformed entry. A
//!   failed invocation rolls back atomically, so the stored bytes (and with
//!   them any user funds accounting) survive untouched for an admin migration
//!   to inspect. Silent defaulting would lose that data.
//! * Safety flags fail **closed**: a malformed `Paused`/`Emergency` flag blocks
//!   money flows, a malformed `Initialized` flag blocks both initialization and
//!   use, and a malformed `AdminNonce` rejects every admin action rather than
//!   restarting the sequence at `0` (which would re-enable replay of every
//!   previously consumed nonce).
//!
//! Non-panicking callers (for example the `simulate_*` dry runs) use the
//! `try_*` variants, which return the same error codes as `Result` values.
//!
//! ## Concurrency and idempotency invariants
//!
//! Soroban smart contracts execute within a single atomic ledger transaction. A
//! given transaction either commits in full or aborts with no state change — there
//! is no partial commit and no interleaving of two concurrent transactions within
//! the same ledger. This means classic "check-then-act" races between two threads
//! are impossible *within* a single invocation, but replay attacks and
//! double-submission at the application layer are real threats.
//!
//! The helpers in this module are therefore hardened against the following adverse
//! patterns:
//!
//! * **Replay attacks (nonce reuse)**: `consume_admin_nonce` stores the *next
//!   expected* nonce immediately after a successful check. A replayed call with
//!   the same nonce will observe the already-incremented value and fail with
//!   [`Error::StaleNonce`]. The stored value is never decremented, so nonces are
//!   strictly monotone.
//!
//! * **Double-initialization**: `require_not_initialized` checks `DataKey::Initialized`
//!   with `.has()` before any write so that a second call to `initialize` from any
//!   code path fails with [`Error::AlreadyInitialized`] regardless of how the check
//!   is reached.
//!
//! * **Double-finalization**: `require_not_finalized` and `is_finalized` are thin
//!   wrappers around a single persistent `.has()` so callers never diverge in how
//!   they interpret the finalization state.
//!
//! * **Pause-then-act gaps**: `load_contract_checked` performs the pause check
//!   *before* loading the contract body. This ensures that no contract data is
//!   visible to the caller when the system is paused, eliminating any ambiguity
//!   about which state the caller should trust.

use crate::{Contract, DataKey, Error, Milestone};
use soroban_sdk::Env;
use soroban_sdk::{IntoVal, TryFromVal, Val, Vec};

// ── Typed persistent reads ────────────────────────────────────────────────────

/// Outcome of a persistent storage read. See the module-level table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StorageRead<T> {
    /// The entry exists and decodes as `T`.
    Present(T),
    /// No entry is stored under the key.
    Missing,
    /// An entry exists but its bytes do not decode as `T`.
    Corrupt,
}

impl<T> StorageRead<T> {
    /// Collapse into a `Result`, reporting `missing` for an absent entry and
    /// [`Error::StorageInvariantViolated`] for a malformed one.
    pub(crate) fn require(self, missing: Error) -> Result<T, Error> {
        match self {
            StorageRead::Present(value) => Ok(value),
            StorageRead::Missing => Err(missing),
            StorageRead::Corrupt => Err(Error::StorageInvariantViolated),
        }
    }

    /// Collapse into a `Result`, using `default` for an absent entry. A
    /// malformed entry is still an error: it is never mistaken for "unset".
    pub(crate) fn or_default(self, default: T) -> Result<T, Error> {
        match self {
            StorageRead::Present(value) => Ok(value),
            StorageRead::Missing => Ok(default),
            StorageRead::Corrupt => Err(Error::StorageInvariantViolated),
        }
    }
}

/// Read and decode a persistent entry without trapping on malformed bytes.
///
/// The raw `Val` is read first and decoded with `try_from_val`, so the three
/// outcomes in [`StorageRead`] are distinguishable. This never writes.
pub(crate) fn read_persistent<K, T>(env: &Env, key: &K) -> StorageRead<T>
where
    K: IntoVal<Env, Val>,
    T: TryFromVal<Env, Val>,
{
    match env.storage().persistent().get::<K, Val>(key) {
        None => StorageRead::Missing,
        Some(raw) => match T::try_from_val(env, &raw) {
            Ok(value) => StorageRead::Present(value),
            Err(_) => StorageRead::Corrupt,
        },
    }
}

/// Read a boolean flag that defaults to `false` when unset.
fn read_flag(env: &Env, key: &DataKey) -> Result<bool, Error> {
    read_persistent::<_, bool>(env, key).or_default(false)
}

// ── Initialization guards ─────────────────────────────────────────────────────

/// Check if the contract system has been initialized.
///
/// Initialization is a prerequisite for all money-flow operations. This check
/// ensures that the admin-controlled safety rails (pause, emergency controls,
/// protocol fees) are always in scope before any funds can move.
///
/// # Panics
/// - `NotInitialized` if initialization has not been completed
/// - `StorageInvariantViolated` if the stored flag is malformed
///
/// # Concurrency invariant
/// This is a read-only guard. The initialization flag is set exactly once by
/// `save_initialized` (see below). A subsequent call to `require_initialized`
/// after initialization will always return `true`.
pub(crate) fn require_initialized(env: &Env) -> bool {
    match read_flag(env, &DataKey::Initialized) {
        Ok(true) => true,
        Ok(false) => env.panic_with_error(Error::NotInitialized),
        Err(error) => env.panic_with_error(error),
    }
}

/// Assert that the contract system has **not** been initialized.
///
/// Call this at the very start of the `initialize` entrypoint to provide a
/// single, consistent double-initialization guard.
///
/// # Panics
/// - `AlreadyInitialized` if `DataKey::Initialized` exists, whatever its value
///
/// # Idempotency invariant
/// The presence check uses `.has()`, so even a malformed flag counts as
/// "initialized": a corrupted entry can never be used to re-run `initialize`
/// and overwrite the stored admin.
pub(crate) fn require_not_initialized(env: &Env) {
    if env.storage().persistent().has(&DataKey::Initialized) {
        env.panic_with_error(Error::AlreadyInitialized);
    }
}

/// Persist the initialized flag and write the admin address in one logical step.
///
/// This helper is the single canonical write path for initialization. Callers
/// MUST call `require_not_initialized` before this function to prevent double
/// writes. Both writes happen in the same invocation, so they commit or roll
/// back together.
///
/// # Invariant
/// After this function returns, `DataKey::Initialized` is `true` and
/// `DataKey::Admin` is `admin`. Both are persistent entries.
pub(crate) fn save_initialized(env: &Env, admin: &crate::Address) {
    env.storage().persistent().set(&DataKey::Initialized, &true);
    env.storage().persistent().set(&DataKey::Admin, admin);
}

// ── Contract ID bounds ────────────────────────────────────────────────────────

/// Validate that `contract_id` is within numeric bounds (non-zero).
///
/// Zero is rejected because contracts are allocated starting from ID 1. Any
/// read or write against ID 0 is a programming error and must fail loudly.
///
/// # Panics
/// - `InvalidContractId` if `contract_id == 0`
pub(crate) fn validate_contract_id_bounds(env: &Env, contract_id: u32) {
    if contract_id == 0 {
        env.panic_with_error(Error::InvalidContractId);
    }
}

// ── Contract loading ──────────────────────────────────────────────────────────

/// Load a contract without panicking.
///
/// # Errors
/// - `InvalidContractId` if `contract_id == 0` (checked before any storage access)
/// - `ContractNotFound` if no contract is stored for this ID
/// - `StorageInvariantViolated` if the stored contract does not decode
pub(crate) fn try_load_contract(env: &Env, contract_id: u32) -> Result<Contract, Error> {
    if contract_id == 0 {
        return Err(Error::InvalidContractId);
    }
    read_persistent(env, &DataKey::Contract(contract_id)).require(Error::ContractNotFound)
}

/// Load a contract's milestones without panicking.
///
/// The key is built through [`crate::keys::milestone_key`], the single
/// definition of the composite milestone key, so this read can never drift from
/// the writers in the rest of the crate.
///
/// # Errors
/// - `InvalidContractId` if `contract_id == 0` (checked before any storage access)
/// - `ContractNotFound` if no milestone vector is stored for this contract
/// - `StorageInvariantViolated` if the stored vector, or any milestone in it,
///   does not decode
pub(crate) fn try_load_milestones(env: &Env, contract_id: u32) -> Result<Vec<Milestone>, Error> {
    if contract_id == 0 {
        return Err(Error::InvalidContractId);
    }
    let key = crate::keys::milestone_key(env, contract_id);
    let milestones: Vec<Val> =
        read_persistent::<_, Vec<Val>>(env, &key).require(Error::ContractNotFound)?;

    // `Vec<Milestone>` decodes its elements lazily, so a malformed element
    // would otherwise trap later, at whichever `.get()` first touches it.
    // Decode every element up front so corruption is reported here, once.
    let mut decoded = Vec::new(env);
    for raw in milestones.iter() {
        let milestone =
            Milestone::try_from_val(env, &raw).map_err(|_| Error::StorageInvariantViolated)?;
        decoded.push_back(milestone);
    }
    Ok(decoded)
}

/// Load a contract from persistent storage.
///
/// # Panics
/// - `InvalidContractId` if `contract_id` is 0
/// - `ContractNotFound` if no contract exists for this ID
/// - `StorageInvariantViolated` if the stored contract is malformed
pub(crate) fn load_contract(env: &Env, contract_id: u32) -> Contract {
    try_load_contract(env, contract_id).unwrap_or_else(|error| env.panic_with_error(error))
}

/// Load milestones for a contract from persistent storage.
///
/// # Panics
/// - `InvalidContractId` if `contract_id` is 0
/// - `ContractNotFound` if no milestone vector exists for this contract
/// - `StorageInvariantViolated` if the stored milestones are malformed
pub(crate) fn load_milestones(env: &Env, contract_id: u32) -> Vec<Milestone> {
    try_load_milestones(env, contract_id).unwrap_or_else(|error| env.panic_with_error(error))
}

/// Load a contract, optionally with precondition checks for mutation.
///
/// - `check_paused`: If true, verifies pause/emergency flags are not set
/// - `check_finalized`: If true, verifies the contract has not been finalized
///
/// The pause check is performed **before** the contract is loaded from storage,
/// so callers never receive a contract value while the system is paused.
///
/// # Panics
/// - `InvalidContractId` if `contract_id` is 0
/// - `ContractPaused` / `EmergencyActive` if `check_paused` and a flag is set
/// - `ContractNotFound` if no contract exists for this ID
/// - `StorageInvariantViolated` if a flag or the contract is malformed
/// - `AlreadyFinalized` if `check_finalized` and the contract is finalized
pub(crate) fn load_contract_checked(
    env: &Env,
    contract_id: u32,
    check_paused: bool,
    check_finalized: bool,
) -> Contract {
    validate_contract_id_bounds(env, contract_id);

    if check_paused {
        require_not_paused(env);
    }

    let contract = load_contract(env, contract_id);

    // Checked after the load so a missing contract reports `ContractNotFound`
    // rather than a misleading `AlreadyFinalized`. Storage is consistent for
    // the whole invocation, so a single check is sufficient.
    if check_finalized {
        require_not_finalized(env, contract_id);
    }

    contract
}

// ── Pause and emergency guards ────────────────────────────────────────────────

/// Report whether the legacy pause or the emergency flag blocks money flows.
///
/// # Errors
/// - `ContractPaused` if the pause flag is set
/// - `EmergencyActive` if the emergency flag is set
/// - `StorageInvariantViolated` if either flag is malformed (fail closed)
pub(crate) fn check_not_paused(env: &Env) -> Result<(), Error> {
    if read_flag(env, &DataKey::Paused)? {
        return Err(Error::ContractPaused);
    }
    if read_flag(env, &DataKey::Emergency)? {
        return Err(Error::EmergencyActive);
    }
    Ok(())
}

/// Panicking form of [`check_not_paused`].
///
/// # Idempotency note
/// This function is read-only and has no side effects. Calling it multiple
/// times within the same transaction always observes the same state.
pub(crate) fn require_not_paused(env: &Env) -> bool {
    check_not_paused(env).unwrap_or_else(|error| env.panic_with_error(error));
    true
}

/// Check that the given [`PauseTarget`] is not blocked by an active scoped pause.
///
/// This is the entrypoint-facing guard used by payout and dispute operations.
/// If a [`PauseScope`] is stored, its target is compared against the requested
/// operation. A `Global` scope blocks everything; `Payout` blocks release,
/// refund, cancel; `Dispute` blocks raise, resolve, rollback.
///
/// The legacy bare `bool` under `DataKey::Paused` is also checked for backward
/// compatibility — it acts as a `Global` pause.
///
/// # Panics
/// - `ContractPaused`, `EmergencyActive` as for [`require_not_paused`]
/// - `PauseScopeActive` if the stored scope overlaps `target`
/// - `StorageInvariantViolated` if a flag or the stored scope is malformed
///
/// [`PauseTarget`]: crate::PauseTarget
/// [`PauseScope`]: crate::PauseScope
pub(crate) fn require_pause_scope(env: &Env, target: &crate::PauseTarget) {
    // Same precedence as `require_not_paused`: legacy bool == Global, then
    // emergency, then the scoped pause compared against the requested target.
    check_not_paused(env).unwrap_or_else(|error| env.panic_with_error(error));

    let scope = match read_persistent::<_, crate::PauseScope>(env, &DataKey::PauseScope) {
        StorageRead::Present(scope) => scope,
        StorageRead::Missing => return,
        StorageRead::Corrupt => env.panic_with_error(Error::StorageInvariantViolated),
    };

    match (&scope.target, target) {
        (crate::PauseTarget::Global, _)
        | (_, crate::PauseTarget::Global)
        | (crate::PauseTarget::Payout, crate::PauseTarget::Payout)
        | (crate::PauseTarget::Dispute, crate::PauseTarget::Dispute) => {
            env.panic_with_error(Error::PauseScopeActive);
        }
        _ => {} // Non-overlapping scope: allow
    }
}

// ── Admin nonce ───────────────────────────────────────────────────────────────

/// Consume the next expected admin nonce, rejecting stale or future values.
///
/// The nonce is a strictly monotone `u64` counter stored under
/// [`DataKey::AdminNonce`]. On the first call the expected nonce is `1`
/// (an absent counter means "never consumed").
///
/// # Invariants
/// * The stored counter never decreases, so a successful call can never be
///   replayed: re-submitting an already-consumed nonce is rejected.
/// * `current + 1` is computed with [`u64::checked_add`]. If the counter is
///   already [`u64::MAX`] the call fails closed with [`Error::PotentialOverflow`]
///   and storage is left unchanged, instead of wrapping back to an accept-all
///   `0`.
/// * A malformed counter fails closed with `StorageInvariantViolated`. It is
///   never treated as absent, because restarting at `0` would accept every
///   nonce consumed so far a second time.
/// * The comparison and the write happen inside the same contract invocation, so
///   a rejected nonce performs no partial write (a panic aborts the invocation).
///
/// # Panics
/// - `StaleNonce` if `provided_nonce != current + 1`
/// - `PotentialOverflow` if `current == u64::MAX`
/// - `StorageInvariantViolated` if the stored counter is malformed
pub(crate) fn consume_admin_nonce(env: &Env, provided_nonce: u64) {
    let current: u64 = read_persistent(env, &DataKey::AdminNonce)
        .or_default(0u64)
        .unwrap_or_else(|error| env.panic_with_error(error));

    let expected = current
        .checked_add(1)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

    if provided_nonce != expected {
        env.panic_with_error(Error::StaleNonce);
    }

    env.storage()
        .persistent()
        .set(&DataKey::AdminNonce, &expected);
}

// ── Finalization guards ───────────────────────────────────────────────────────

/// Check if a contract has been finalized.
///
/// # Panics
/// - `InvalidContractId` if `contract_id` is 0 (reserved sentinel)
pub(crate) fn is_finalized(env: &Env, contract_id: u32) -> bool {
    validate_contract_id_bounds(env, contract_id);
    env.storage()
        .persistent()
        .has(&DataKey::Finalization(contract_id))
}

/// Require that a contract has not been finalized.
///
/// # Panics
/// - `InvalidContractId` if `contract_id` is 0
/// - `AlreadyFinalized` if the contract has been finalized
///
/// # Idempotency note
/// Once a finalization record is written, this function will always panic for
/// that contract ID. There is no operation that removes a finalization record.
pub(crate) fn require_not_finalized(env: &Env, contract_id: u32) -> bool {
    if is_finalized(env, contract_id) {
        env.panic_with_error(Error::AlreadyFinalized);
    }
    true
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// Persistent-storage calls must run inside a registered contract context
// (`env.as_contract(..)`), which needs `Escrow` registered. The tests for these
// helpers therefore live in `src/test/storage_recovery.rs`.
