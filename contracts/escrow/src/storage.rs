//! Centralized storage precondition checks and contract loading helpers.
//! Centralized storage precondition checks and contract loading helpers.
//!
//! This module extracts repeated storage validation patterns into a single source
//! of truth, ensuring consistent error handling and reducing code duplication across
//! entrypoints. All contract loading operations should route through these helpers.
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

use crate::{Contract, DataKey, Error, EscrowError};
use crate::ContractStatus;
use soroban_sdk::{Env, Symbol, Vec};

// ── Initialization guards ─────────────────────────────────────────────────────

/// Deterministically load a contract, retrying transient storage failures.
///
/// Unlike [`load_contract`], this variant does not panic on the first
/// missing read. It retries up to [`MAX_STORAGE_RETRIES`] times and only
/// panics with `ContractNotFound` once all attempts are exhausted. This
/// makes recovery observable and prevents a single transient miss from
/// aborting an otherwise valid operation.
///
/// # Panics
/// - `InvalidContractId` if `contract_id` is 0
/// - `ContractNotFound` if the contract is still missing after all retries
pub(crate) fn load_contract_recoverable(env: &Env, contract_id: u32) -> Contract {
    validate_contract_id_bounds(env, contract_id);
    env.storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
}

/// Deterministically load milestones, retrying transient storage failures.
///
/// Mirrors [`load_milestones`] but retries transient misses before
/// panicking, so recovery is deterministic and observable.
///
/// # Panics
/// - `InvalidContractId` if `contract_id` is 0
/// - `ContractNotFound` if milestones are still missing after all retries
pub(crate) fn load_milestones_recoverable(
    env: &Env,
    contract_id: u32,
) -> Vec<crate::Milestone> {
    validate_contract_id_bounds(env, contract_id);
    let milestone_key = Symbol::new(env, "milestones");
    env.storage()
        .persistent()
        .get(&(DataKey::Contract(contract_id), milestone_key))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
}

/// Validate that contract_id is within numeric bounds (non-zero).
/// Validate that contract_id is within numeric bounds (non-zero).
///
/// This is the **entrypoint preamble** guard: it rejects the reserved id `0` as
/// invalid input with [`Error::InvalidContractId`]. Loaders and predicates that
/// must treat `0` like any other unknown id use [`require_nonzero_contract_id`]
/// instead — see the module-level compatibility contract.
///
/// # Panics
/// - `ContractNotFound` if `contract_id == 0`
pub(crate) fn validate_contract_id_bounds(env: &Env, contract_id: u32) {
    if contract_id == 0 {
        env.panic_with_error(Error::InvalidContractId);
    }
}

/// Validate that a milestone index is within the bounds of the milestone vector.
///
/// Milestone indices are zero-based. This helper centralizes the boundary check
/// so that all callers reject out-of-range indices deterministically rather than
/// relying on `Vec::get` returning `None` and being silently ignored.
///
/// # Panics
/// - `InvalidMilestoneIndex` if `index >= len`
pub(crate) fn validate_milestone_index_bounds(env: &Env, index: u32, len: u32) {
    if index >= len {
        env.panic_with_error(EscrowError::InvalidMilestoneIndex);
    }
}

/// Validate that a milestone amount is strictly positive.
///
/// Zero-amount milestones are rejected because they would allow no-op state
/// transitions and could mask accounting bugs. This is a boundary check applied
/// at the point of milestone creation.
///
/// # Panics
/// - `InvalidMilestoneAmount` if `amount == 0`
pub(crate) fn validate_milestone_amount(env: &Env, amount: i128) {
    if amount <= 0 {
        env.panic_with_error(EscrowError::InvalidMilestoneAmount);
    }
}

/// Validate that a milestone deadline, if present, is strictly in the future.
///
/// A deadline equal to the current ledger timestamp is treated as already
/// expired to avoid a race where a milestone becomes immediately refundable
/// in the same ledger it was created.
///
/// # Panics
/// - `InvalidDeadline` if `deadline <= now`
pub(crate) fn validate_milestone_deadline(env: &Env, deadline: Option<u64>) {
    if let Some(d) = deadline {
        let now = env.ledger().timestamp();
        if d <= now {
            env.panic_with_error(EscrowError::InvalidDeadline);
        }
    }
}

/// Validate that a milestone vector is non-empty and within a sane upper bound.
///
/// Empty milestone sets would allow contracts with no work units, and unbounded
/// sets could exhaust ledger entry limits. Both are rejected deterministically.
///
/// # Panics
/// - `InvalidMilestoneCount` if `len == 0` or `len > MAX_MILESTONES`
pub(crate) fn validate_milestone_count(env: &Env, len: u32) {
    const MAX_MILESTONES: u32 = 100;
    if len == 0 || len > MAX_MILESTONES {
        env.panic_with_error(EscrowError::InvalidMilestoneCount);
    }
}

/// Validate that a milestone has not already been released or refunded.
///
/// This guards against duplicate submissions: a milestone that has already
/// reached a terminal state must not be mutated again.
///
/// # Panics
/// - `MilestoneAlreadyReleased` if `released` is true
/// - `MilestoneAlreadyRefunded` if `refunded` is true
pub(crate) fn validate_milestone_not_terminal(
    env: &Env,
    released: bool,
    refunded: bool,
) {
    if released {
        env.panic_with_error(EscrowError::MilestoneAlreadyReleased);
    }
    if refunded {
        env.panic_with_error(EscrowError::MilestoneAlreadyRefunded);
    }
}

/// Validate that a milestone has not already been funded beyond its amount.
///
/// Prevents over-funding a milestone, which would break the invariant that
/// `funded_amount <= amount` for every milestone.
///
/// # Panics
/// - `MilestoneOverFunded` if `funded_amount > amount`
pub(crate) fn validate_milestone_funding(env: &Env, amount: i128, funded_amount: i128) {
    if funded_amount > amount {
        env.panic_with_error(EscrowError::MilestoneOverFunded);
    }
}

/// Validate that a milestone has not already been funded (duplicate funding guard).
///
/// A milestone may only be funded once. This is the duplicate-submission guard
/// for the funding path.
///
/// # Panics
/// - `MilestoneAlreadyFunded` if `funded_amount > 0`
pub(crate) fn validate_milestone_not_funded(env: &Env, funded_amount: i128) {
    if funded_amount > 0 {
        env.panic_with_error(EscrowError::MilestoneAlreadyFunded);
    }
}

/// Validate that a milestone has been fully funded before release or refund.
///
/// Release and refund operations require the milestone to be fully funded so
/// that accounting remains consistent.
///
/// # Panics
/// - `MilestoneNotFunded` if `funded_amount < amount`
pub(crate) fn validate_milestone_fully_funded(env: &Env, amount: i128, funded_amount: i128) {
    if funded_amount < amount {
        env.panic_with_error(EscrowError::MilestoneNotFunded);
    }
}

/// Validate that a milestone index refers to a milestone that exists in the
/// provided vector, returning the milestone or panicking with a deterministic
/// error.
///
/// This is the canonical lookup helper for milestone operations. It combines
/// the bounds check with the storage read so that callers cannot accidentally
/// skip the boundary validation.
///
/// # Panics
/// - `InvalidMilestoneIndex` if `index >= milestones.len()`
pub(crate) fn load_milestone_at(
    env: &Env,
    milestones: &Vec<crate::Milestone>,
    index: u32,
) -> crate::Milestone {
    validate_milestone_index_bounds(env, index, milestones.len());
    milestones
        .get(index)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidMilestoneIndex))
}

/// Validate that a contract is in a state that permits milestone mutation.
///
/// Milestones may only be mutated while the contract is in `Created` or
/// `Funded` status. Terminal statuses (`Completed`, `Cancelled`, `Disputed`)
/// must not accept further milestone changes.
///
/// # Panics
/// - `InvalidContractStatus` if the status does not permit mutation
pub(crate) fn validate_contract_mutable(env: &Env, status: &crate::ContractStatus) {
    match status {
        crate::ContractStatus::Created | crate::ContractStatus::Funded => {}
        _ => env.panic_with_error(EscrowError::InvalidContractStatus),
    }
}

/// Validate that a contract is in a state that permits release of funds.
///
/// Release requires the contract to be `Funded`.
///
/// # Panics
/// - `InvalidContractStatus` if the status is not `Funded`
pub(crate) fn validate_contract_releasable(env: &Env, status: &crate::ContractStatus) {
    if !matches!(status, crate::ContractStatus::Funded) {
        env.panic_with_error(EscrowError::InvalidContractStatus);
    }
}

/// Validate that a contract is in a state that permits refund of funds.
///
/// Refund requires the contract to be `Funded` or `Cancelled`.
///
/// # Panics
/// - `InvalidContractStatus` if the status is not `Funded` or `Cancelled`
pub(crate) fn validate_contract_refundable(env: &Env, status: &crate::ContractStatus) {
    match status {
        crate::ContractStatus::Funded | crate::ContractStatus::Cancelled => {}
        _ => env.panic_with_error(EscrowError::InvalidContractStatus),
    }
}

/// Validate that a contract is in a state that permits finalization.
///
/// Finalization requires the contract to be `Completed` or `Cancelled`.
///
/// # Panics
/// - `InvalidContractStatus` if the status is not terminal
pub(crate) fn validate_contract_finalizable(env: &Env, status: &crate::ContractStatus) {
    match status {
        crate::ContractStatus::Completed | crate::ContractStatus::Cancelled => {}
        _ => env.panic_with_error(EscrowError::InvalidContractStatus),
    }
}

/// Validate that a contract has no outstanding funded milestones before
/// finalization.
///
/// Finalizing a contract with unreleased or unrefunded funds would strand
/// those funds. This is the accounting invariant guard for finalization.
///
/// # Panics
/// - `OutstandingFunds` if any milestone has `funded_amount > 0` and is not
///   released or refunded
pub(crate) fn validate_no_outstanding_funds(
    env: &Env,
    milestones: &Vec<crate::Milestone>,
) {
    for i in 0..milestones.len() {
        let m = milestones.get(i).unwrap();
        if m.funded_amount > 0 && !m.released && !m.refunded {
            env.panic_with_error(EscrowError::OutstandingFunds);
        }
    }
}

/// Validate that the sum of milestone amounts equals the contract's total
/// deposited amount.
///
/// This is the core accounting invariant: the contract's `total_deposited`
/// must equal the sum of all milestone amounts. Any mismatch indicates a
/// bug or corruption and must be rejected.
///
/// # Panics
/// - `AccountingMismatch` if the sums do not match
pub(crate) fn validate_milestone_sum(
    env: &Env,
    milestones: &Vec<crate::Milestone>,
    total_deposited: i128,
) {
    let mut sum: i128 = 0;
    for i in 0..milestones.len() {
        let m = milestones.get(i).unwrap();
        sum = sum.checked_add(m.amount).unwrap_or_else(|| {
            env.panic_with_error(EscrowError::AccountingMismatch)
        });
    }
    if sum != total_deposited {
        env.panic_with_error(EscrowError::AccountingMismatch);
    }
}

/// Validate that a milestone's released and refunded amounts do not exceed
/// its funded amount.
///
/// This prevents double-release or double-refund from inflating the
/// accounting totals.
///
/// # Panics
/// - `AccountingMismatch` if `released_amount + refunded_amount > funded_amount`
pub(crate) fn validate_milestone_accounting(
    env: &Env,
    milestone: &crate::Milestone,
) {
    let released_amount = if milestone.released {
        milestone.funded_amount
    } else {
        0
    };
    let total = released_amount
        .checked_add(milestone.refunded_amount)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::AccountingMismatch));
    if total > milestone.funded_amount {
        env.panic_with_error(EscrowError::AccountingMismatch);
    }
}

/// Validate that a contract's released and refunded amounts do not exceed
/// its total deposited amount.
///
/// # Panics
/// - `AccountingMismatch` if `released_amount + refunded_amount > total_deposited`
pub(crate) fn validate_contract_accounting(env: &Env, contract: &Contract) {
    let total = contract
        .released_amount
        .checked_add(contract.refunded_amount)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::AccountingMismatch));
    if total > contract.total_deposited {
        env.panic_with_error(EscrowError::AccountingMismatch);
    }
}

/// Validate that a contract's client and freelancer addresses are distinct.
///
/// A contract where client == freelancer would allow self-dealing and break
/// the escrow trust model.
///
/// # Panics
/// - `InvalidParties` if `client == freelancer`
pub(crate) fn validate_contract_parties(env: &Env, contract: &Contract) {
    if contract.client == contract.freelancer {
        env.panic_with_error(EscrowError::InvalidParties);
    }
}

/// Validate that a contract's arbiter, if present, is distinct from both
/// the client and the freelancer.
///
/// # Panics
/// - `InvalidParties` if the arbiter equals the client or freelancer
pub(crate) fn validate_contract_arbiter(env: &Env, contract: &Contract) {
    if let Some(ref arbiter) = contract.arbiter {
        if *arbiter == contract.client || *arbiter == contract.freelancer {
            env.panic_with_error(EscrowError::InvalidParties);
        }
    }
}

/// Validate that a contract's funded amount does not exceed its total
/// deposited amount.
///
/// # Panics
/// - `AccountingMismatch` if `funded_amount > total_deposited`
pub(crate) fn validate_contract_funding(env: &Env, contract: &Contract) {
    if contract.funded_amount > contract.total_deposited {
        env.panic_with_error(EscrowError::AccountingMismatch);
    }
}

/// Validate that a contract's released amount does not exceed its funded
/// amount.
///
/// # Panics
/// - `AccountingMismatch` if `released_amount > funded_amount`
pub(crate) fn validate_contract_release_bounds(env: &Env, contract: &Contract) {
    if contract.released_amount > contract.funded_amount {
        env.panic_with_error(EscrowError::AccountingMismatch);
    }
}

/// Validate that a contract's refunded amount does not exceed its funded
/// amount.
///
/// # Panics
/// - `AccountingMismatch` if `refunded_amount > funded_amount`
pub(crate) fn validate_contract_refund_bounds(env: &Env, contract: &Contract) {
    if contract.refunded_amount > contract.funded_amount {
        env.panic_with_error(EscrowError::AccountingMismatch);
    }
}

/// Validate that a contract's reputation has not already been issued.
///
/// Reputation issuance is a one-time operation per contract.
///
/// # Panics
/// - `ReputationAlreadyIssued` if `reputation_issued` is true
pub(crate) fn validate_reputation_not_issued(env: &Env, contract: &Contract) {
    if contract.reputation_issued {
        env.panic_with_error(EscrowError::ReputationAlreadyIssued);
    }
}

/// Validate that a contract's reputation has been issued before it can be
/// finalized.
///
/// # Panics
/// - `ReputationNotIssued` if `reputation_issued` is false
pub(crate) fn validate_reputation_issued(env: &Env, contract: &Contract) {
    if !contract.reputation_issued {
        env.panic_with_error(EscrowError::ReputationNotIssued);
    }
}

/// Validate that a contract's release authorization mode is compatible with
/// the caller's role.
///
/// # Panics
/// - `Unauthorized` if the caller is not permitted to release under the
///   current authorization mode
pub(crate) fn validate_release_authorization(
    env: &Env,
    contract: &Contract,
    caller: &soroban_sdk::Address,
) {
    use crate::ReleaseAuthorization;
    let authorized = match contract.release_authorization {
        ReleaseAuthorization::ClientOnly => caller == &contract.client,
        ReleaseAuthorization::MultiSig => {
            caller == &contract.client || caller == &contract.freelancer
        }
        ReleaseAuthorization::ArbiterOnly => {
            contract.arbiter.as_ref().map_or(false, |a| caller == a)
        }
        ReleaseAuthorization::ClientAndArbiter => {
            caller == &contract.client
                || contract.arbiter.as_ref().map_or(false, |a| caller == a)
        }
    };
    if !authorized {
        env.panic_with_error(Error::Unauthorized);
    }
}

/// Validate that a contract's status is consistent with its accounting
/// fields.
///
/// This is a cross-field invariant check that catches corrupted or
/// inconsistent state before it can cause silent data loss.
///
/// # Panics
/// - `InvalidContractStatus` if the status is inconsistent with the
///   accounting fields
pub(crate) fn validate_contract_status_consistency(env: &Env, contract: &Contract) {
    use crate::ContractStatus;
    match contract.status {
        ContractStatus::Created => {
            if contract.funded_amount != 0
                || contract.released_amount != 0
                || contract.refunded_amount != 0
            {
                env.panic_with_error(EscrowError::InvalidContractStatus);
            }
        }
        ContractStatus::Accepted | ContractStatus::PartiallyFunded => {
            if contract.funded_amount == 0 {
                env.panic_with_error(EscrowError::InvalidContractStatus);
            }
        }
        ContractStatus::Funded => {
            if contract.funded_amount == 0 {
                env.panic_with_error(EscrowError::InvalidContractStatus);
            }
        }
        ContractStatus::Completed => {
            if contract.released_amount == 0 && contract.refunded_amount == 0 {
                env.panic_with_error(EscrowError::InvalidContractStatus);
            }
        }
        ContractStatus::Cancelled => {
            if contract.refunded_amount == 0 {
                env.panic_with_error(EscrowError::InvalidContractStatus);
            }
        }
        ContractStatus::Disputed => {}
        ContractStatus::Refunded => {
            if contract.refunded_amount == 0 {
                env.panic_with_error(EscrowError::InvalidContractStatus);
            }
        }
    }
}

/// Validate that a contract's milestone vector is consistent with the
/// contract's accounting fields.
///
/// This is the top-level invariant check that should be called after any
/// milestone mutation to ensure the contract and its milestones remain in
/// agreement.
///
/// # Panics
/// - `AccountingMismatch` if any invariant is violated
pub(crate) fn validate_contract_milestone_consistency(
    env: &Env,
    contract: &Contract,
    milestones: &Vec<crate::Milestone>,
) {
    validate_milestone_count(env, milestones.len());
    validate_milestone_sum(env, milestones, contract.total_deposited);
    for i in 0..milestones.len() {
        let m = milestones.get(i).unwrap();
        validate_milestone_funding(env, m.amount, m.funded_amount);
        validate_milestone_accounting(env, &m);
    }
    validate_contract_accounting(env, contract);
    validate_contract_funding(env, contract);
    validate_contract_release_bounds(env, contract);
    validate_contract_refund_bounds(env, contract);
}

/// Validate that a contract's total deposited amount is non-negative.
///
/// # Panics
/// - `AccountingMismatch` if `total_deposited < 0`
pub(crate) fn validate_total_deposited(env: &Env, total_deposited: i128) {
    if total_deposited < 0 {
        env.panic_with_error(EscrowError::AccountingMismatch);
    }
}

/// Validate that a contract's funded amount is non-negative.
///
/// # Panics
/// - `AccountingMismatch` if `funded_amount < 0`
pub(crate) fn validate_funded_amount(env: &Env, funded_amount: i128) {
    if funded_amount < 0 {
        env.panic_with_error(EscrowError::AccountingMismatch);
    }
}

/// Validate that a contract's released amount is non-negative.
///
/// # Panics
/// - `AccountingMismatch` if `released_amount < 0`

/// Check if the contract system has been initialized.
///
/// Initialization is a prerequisite for all money-flow operations. This check
/// ensures that the admin-controlled safety rails (pause, emergency controls,
/// protocol fees) are always in scope before any funds can move.
///
/// # Arguments
/// * `env` - The contract environment
///
/// # Panics
/// - `NotInitialized` if initialization has not been completed
///
/// # Returns
/// `true` if initialized, or panics with `NotInitialized`
///
/// # Concurrency invariant
/// This is a read-only guard. The initialization flag is set exactly once by
/// `save_initialized` (see below). A subsequent call to `require_initialized`
/// after initialization will always return `true`.
pub(crate) fn require_initialized(env: &Env) -> bool {
    let initialized = env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Initialized)
        .unwrap_or(false);
    if !initialized {
        env.panic_with_error(Error::NotInitialized);
    }
    true
}

/// Assert that the contract system has **not** been initialized.
///
/// Call this at the very start of the `initialize` entrypoint to provide a
/// single, consistent double-initialization guard. Every code path that might
/// call into initialization logic should route through this helper rather than
/// performing an inline `.has()` check.
///
/// # Panics
/// - `AlreadyInitialized` if `DataKey::Initialized` is already set to `true`
///
/// # Idempotency invariant
/// Once `save_initialized` has written `DataKey::Initialized = true`, every
/// subsequent call to `require_not_initialized` will panic. There is no
/// operation that clears the initialized flag, so initialization is
/// permanently one-shot.
pub(crate) fn require_not_initialized(env: &Env) {
    // Use `.has()` instead of `.get()` for the presence check so we avoid
    // deserializing the value; the mere existence of the key is sufficient.
    if env.storage().persistent().has(&DataKey::Initialized) {
        env.panic_with_error(Error::AlreadyInitialized);
    }
}

/// Persist the initialized flag and write the admin address in one logical step.
///
/// This helper is the single canonical write path for initialization. Callers
/// MUST call `require_not_initialized` before this function to prevent double
/// writes. The two-step pattern (check then write) is safe within Soroban
/// because a ledger transaction is fully atomic: if the check passes, no other
/// transaction can have set the flag between the check and the write in the
/// same transaction context.
///
/// # Arguments
/// * `env`   - The contract environment
/// * `admin` - The admin address to record under `DataKey::Admin`
///
/// # Invariant
/// After this function returns, `DataKey::Initialized` is `true` and
/// `DataKey::Admin` is `admin`. Both are persistent entries.
pub(crate) fn save_initialized(env: &Env, admin: &crate::Address) {
    env.storage().persistent().set(&DataKey::Initialized, &true);
    env.storage().persistent().set(&DataKey::Admin, admin);
}

// ── Contract loading ──────────────────────────────────────────────────────────

/// Load a contract from persistent storage.
///
/// This is the canonical pattern for retrieving a contract. It handles the
/// storage read with consistent error reporting and bounds checking.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID to load
///
/// # Panics
/// - `ContractNotFound` if `contract_id` is 0
/// - `ContractNotFound` if no contract exists for this ID
///
/// # Returns
/// The loaded `Contract` or panics with `ContractNotFound`
pub(crate) fn load_contract(env: &Env, contract_id: u32) -> Contract {
    // Invariant 2: reject the zero ID before any storage access so that a
    // missing record and an invalid key are distinguishable in tests and
    // logs.
    validate_contract_id_bounds(env, contract_id);
    env.storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
}

/// Load milestones for a contract from persistent storage.
///
/// Milestones are stored under a composite key combining the contract ID
/// and a "milestones" symbol. This helper centralizes the retrieval pattern.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID whose milestones to load
///
/// # Panics
/// - `ContractNotFound` if `contract_id` is 0
/// - `ContractNotFound` if no milestone vector exists for this contract
///
/// # Returns
/// The loaded milestone vector or panics with `ContractNotFound`
///
/// The key is built through [`crate::keys::milestone_key`], the single
/// definition of the composite milestone key, so this read can never drift from
/// the writers in the rest of the crate.
pub(crate) fn load_milestones(env: &Env, contract_id: u32) -> Vec<crate::Milestone> {
    // Invariant 2: same zero-ID guard as load_contract. Milestones are keyed
    // by the same contract ID, so an invalid ID must never reach storage.
    validate_contract_id_bounds(env, contract_id);
    let milestone_key = crate::keys::milestone_key(env, contract_id);
    env.storage()
        .persistent()
        .get(&milestone_key)
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
}

/// Load a contract, optionally with precondition checks for mutation.
///
/// This is the primary helper for loading contracts with optional safety guards:
/// - `check_paused`: If true, verifies pause/emergency flags are not set
/// - `check_finalized`: If true, verifies the contract has not been finalized
///
/// The pause check is performed **before** the contract is loaded from storage.
/// This ordering is intentional: it means callers never receive a contract
/// value in a state where the system is paused, which closes a potential
/// check-then-use ambiguity when the returned value is stored in a local
/// variable and the pause state changes conceptually between the load and the
/// mutation.
///
/// Within a single Soroban transaction the storage is consistent throughout,
/// but this ordering also makes the control flow easier to reason about during
/// code review.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID to load
/// * `check_paused` - Whether to verify pause/emergency states
/// * `check_finalized` - Whether to verify finalization state
///
/// # Panics
/// - `ContractNotFound` if `contract_id` is 0
/// - `ContractPaused` if `check_paused` is true and pause flag is set
/// - `EmergencyActive` if `check_paused` is true and emergency flag is set
/// - `AlreadyFinalized` if `check_finalized` is true and contract is finalized
///
/// # Returns
/// The loaded `Contract` if all preconditions pass
///
/// # Concurrency
/// All guards are evaluated against the current stored state in this call. The
/// returned contract is the value observed at load time; callers must not cache
/// it across invocations.
pub(crate) fn load_contract_checked(
    env: &Env,
    contract_id: u32,
    check_paused: bool,
    check_finalized: bool,
) -> Contract {
    // Validate bounds first — this rejects the degenerate zero ID immediately
    // before incurring any storage reads.
    validate_contract_id_bounds(env, contract_id);

    // Pause check happens before the contract load (see doc comment above).
    if check_paused {
        require_not_paused(env);
    }

    // Load the contract body.
    let contract = load_contract(env, contract_id);

    // Finalization check follows the load because the finalization record is
    // stored under a separate key from the contract body. Both are read in the
    // same transaction, so this is consistent. Checking finalization *after*
    // confirming the contract exists avoids a misleading `AlreadyFinalized` on
    // a non-existent contract.
    if check_finalized {
        require_not_finalized(env, contract_id);
        // Re-check after load to close the race window where a concurrent
        // finalize could have committed between the load and the guard.
        require_not_finalized(env, contract_id);
    }

    contract
}

// ── Pause and emergency guards ────────────────────────────────────────────────

/// Check if the contract system is paused or in emergency mode.
///
/// # Arguments
/// * `env` - The contract environment
///
/// # Panics
/// - `ContractPaused` if the pause flag is set
/// - `EmergencyActive` if the emergency flag is set
///
/// # Returns
/// `true` if neither pause nor emergency is active, or panics
///
/// # Idempotency note
/// This function is read-only and has no side effects. Calling it multiple
/// times within the same transaction always observes the same state.
pub(crate) fn require_not_paused(env: &Env) -> bool {
    // Invariant 3: emergency takes precedence over the legacy boolean pause
    // so that operators can escalate without first clearing the pause flag.
    // Both are read fresh from persistent storage on every call.
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        env.panic_with_error(Error::ContractPaused);
    }
    // Emergency always blocks everything.
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Emergency)
        .unwrap_or(false)
    {
        env.panic_with_error(Error::EmergencyActive);
    }
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
pub(crate) fn require_pause_scope(env: &Env, target: &crate::PauseTarget) {
    // Invariant 3: the precedence here must match require_not_paused.
    // Legacy bool == Global, emergency == Global, then scoped pause is
    // compared against the requested target.
    // Legacy boolean pause acts as Global
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        env.panic_with_error(Error::ContractPaused);
    }

    // Emergency always blocks everything.
    // Emergency always blocks everything
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Emergency)
        .unwrap_or(false)
    {
        env.panic_with_error(Error::EmergencyActive);
    }

    // Scoped pause.
    // Scoped pause
    if let Some(scope) = env
        .storage()
        .persistent()
        .get::<_, crate::PauseScope>(&DataKey::PauseScope)
    {
        match (&scope.target, target) {
            (crate::PauseTarget::Global, _) | (_, crate::PauseTarget::Global) => {
                env.panic_with_error(Error::PauseScopeActive);
            }
            (crate::PauseTarget::Payout, crate::PauseTarget::Payout) => {
                env.panic_with_error(Error::PauseScopeActive);
            }
            (crate::PauseTarget::Dispute, crate::PauseTarget::Dispute) => {
                env.panic_with_error(Error::PauseScopeActive);
            }
            _ => {} // Non-overlapping scope: allow
        }
    }
}

// ── Admin nonce ───────────────────────────────────────────────────────────────

/// Consume the next expected admin nonce, rejecting stale or future values.
///
/// The nonce is a strictly monotone `u64` counter stored under
/// [`DataKey::AdminNonce`]. On the first call the expected nonce is `1`
/// (zero means "never consumed").
///
/// # Atomicity invariant
/// The read, compare, and increment are performed within a single Soroban
/// transaction. Soroban's ledger guarantees that no other transaction can
/// observe or modify `DataKey::AdminNonce` between the `.get` and the `.set`
/// within the same invocation. This makes the combined read-validate-write
/// effectively atomic.
///
/// A replay of the same call in a later transaction will read the incremented
/// value and immediately fail with [`Error::StaleNonce`].
///
/// # Overflow guard
/// If `current + 1` would overflow `u64`, the function panics with
/// [`Error::PotentialOverflow`]. At one nonce per admin operation, the 2^64
/// ceiling is not reachable in practice, but the check is present to satisfy
/// formal correctness requirements and to make the contract provably panic-safe.
///
/// # Arguments
/// * `env`            - The contract environment
/// * `provided_nonce` - The nonce value supplied by the caller
///
/// # Invariants
/// * The stored counter never decreases, so a successful call can never be
///   replayed: re-submitting an already-consumed nonce is rejected.
/// * `current + 1` is computed with [`u64::checked_add`]. If the counter is
///   already [`u64::MAX`] the call fails closed with [`Error::PotentialOverflow`]
///   and storage is left unchanged, instead of wrapping back to an accept-all
///   `0`. This keeps the nonce safe under adversarial replay of admin actions.
/// * The comparison and the write happen inside the same contract invocation, so
///   a rejected nonce performs no partial write (a panic aborts the invocation).
///
/// # Panics
/// - `StaleNonce` if `provided_nonce != current + 1`
/// - `PotentialOverflow` if `current == u64::MAX`
pub(crate) fn consume_admin_nonce(env: &Env, provided_nonce: u64) {
    // Invariant 5: monotonic nonce. `current == 0` means uninitialized, so
    // the first accepted value is 1. Any other value (stale or future) is
    // rejected before the write, so a failed call cannot advance the nonce.
    // On success the increment is persisted atomically with the check.
    let current: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::AdminNonce)
        .unwrap_or(0u64);

    // Guard against nonce counter overflow (defensive; 2^64 is unreachable
    // in any realistic deployment timeline).
    let expected = current
        .checked_add(1)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

    if provided_nonce != expected {
        env.panic_with_error(Error::StaleNonce);
    }

    // Commit the incremented nonce atomically within this transaction.
    // Any replay using the same `provided_nonce` in a future transaction
    // will find `current = expected` and compute `expected_new = expected + 1`,
    // causing the equality check to fail.
    env.storage()
        .persistent()
        .set(&DataKey::AdminNonce, &expected);
}

// ── Finalization guards ───────────────────────────────────────────────────────

/// Check if a contract has been finalized.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID to check
///
/// # Panics
/// - `ContractNotFound` if `contract_id` is 0 (reserved sentinel)
///
/// # Returns
/// `true` if the contract is finalized
pub(crate) fn is_finalized(env: &Env, contract_id: u32) -> bool {
    // Invariant 2: zero ID is never a valid finalization key.
    validate_contract_id_bounds(env, contract_id);
    env.storage()
        .persistent()
        .has(&DataKey::Finalization(contract_id))
}

/// Require that a contract has not been finalized.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID to check
///
/// # Panics
/// - `ContractNotFound` if `contract_id` is 0
/// - `AlreadyFinalized` if the contract has been finalized
///
/// # Returns
/// `true` if not finalized, or panics
///
/// # Idempotency note
/// Once a finalization record is written, this function will always panic for
/// that contract ID. There is no operation that removes a finalization record.
pub(crate) fn require_not_finalized(env: &Env, contract_id: u32) -> bool {
    // Invariant 2 + 4: bounds check first, then the terminal-state check.
    validate_contract_id_bounds(env, contract_id);
    if is_finalized(env, contract_id) {
        env.panic_with_error(Error::AlreadyFinalized);
    }
    true
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// Unit tests for the storage helpers are located in `src/test/storage_helpers.rs`
// rather than as inline `#[cfg(test)]` tests here. This is necessary because
// Soroban's SDK requires all persistent-storage calls to execute within an active
// contract context (`env.as_contract(&contract_address, || { ... })`), which in
// turn requires a registered contract instance via `env.register(Escrow, ())`.
//
// Registering an `Escrow` from within storage.rs would create a circular
// module dependency. Moving the tests to the `test/` module, which already
// imports `Escrow` and `EscrowClient`, resolves this cleanly.

