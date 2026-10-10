//! Centralized storage key definitions and constructors for escrow milestones.
//!
//! # Key Uniqueness Invariants
//!
//! The key space is partitioned so that **no two distinct logical objects share
//! the same storage key**:
//!
//! - `(DataKey::Contract(id), Symbol("milestones"))` — milestone vector for
//!   contract `id`.  Unique because `DataKey::Contract(id)` is unique per `id`
//!   and the symbol is fixed.
//! - `DataKey::MilestoneApprovals(contract_id, milestone_index)` — temporary
//!   approval record for a specific (contract, milestone) pair.
//!
//! # Concurrency Safety
//!
//! Soroban smart contracts execute inside a single ledger transaction, so true
//! OS-level concurrent writes cannot occur within a single invocation. However,
//! **duplicate invocations** and **retry storms** (e.g., a client submitting
//! the same approval twice due to a network timeout) *can* reach the contract on
//! separate ledger transactions.  The key constructors in this module are the
//! first defensive line:
//!
//! 1. **Determinism** — calling `milestone_key(env, id)` or
//!    `milestone_approval_key(id, idx)` with identical arguments **always**
//!    produces an identical key, so reads and writes are idempotent with
//!    respect to the storage address.
//!
//! 2. **Zero-ID Guard** — `contract_id == 0` is an illegal sentinel value
//!    (the counter starts at 1 in `create_contract`).  Any call with `id == 0`
//!    panics with `EscrowError::InvalidContractId` before a key is constructed,
//!    preventing silent writes to the zero-slot that could mask contract-not-
//!    found bugs.
//!
//! 3. **Index bound** — `milestone_index` is not range-checked here because the
//!    caller must load the milestone vector first; the check belongs there.
//!    This keeps the key constructor pure (no storage reads).
//!
//! # Change Safety
//!
//! Do not change the structure of any key variant without a storage migration:
//! existing ledger entries use the serialized form of these keys and would
//! become unreachable if the format changes.

use crate::milestones_consts::MAX_MILESTONES;
use crate::types::DataKey;
use soroban_sdk::{Env, Symbol};

use crate::EscrowError;

// ── Key constructors ─────────────────────────────────────────────────────────

/// Returns the persistent storage key for a contract's milestones vector:
/// `(DataKey::Contract(contract_id), Symbol::new(env, "milestones"))`.
///
/// # Panics
/// Panics with `InvalidContractId` if `contract_id == 0`.  The counter in
/// `create_contract` starts at 1, so zero is never a valid contract ID.
///
/// # Idempotency
/// Calling this function multiple times with the same `contract_id` always
/// returns the same key.  It is safe to call from any execution path; the
/// result is deterministic.
#[inline]
pub fn milestone_key(env: &Env, contract_id: u32) -> (DataKey, Symbol) {
    require_valid_contract_id(env, contract_id);
    (DataKey::Contract(contract_id), milestone_symbol(env))
}

/// Returns the `Symbol` key component for milestones: `"milestones"`.
///
/// Extracted so callers that need only the symbol (e.g. TTL helpers) do not
/// have to reconstruct it independently.
#[inline]
pub fn milestone_symbol(env: &Env) -> Symbol {

    Symbol::new(env, "milestones")

}

/// Returns the temporary storage key for milestone release approvals:

/// `DataKey::MilestoneApprovals(contract_id, milestone_index)`.
///
/// # Design note
/// This constructor takes **no `&Env`** because `DataKey::MilestoneApprovals`
/// is a plain enum variant and requires no SDK allocation.  The key is
/// constructed deterministically from its two integer arguments.
///
/// `contract_id == 0` is not validated here because `milestone_approval_key`
/// is only called after the contract has already been loaded from storage —
/// at that point a zero-id would have already panicked in `load_contract`.
/// If you call this before loading the contract, ensure the caller validates
/// `contract_id` beforehand.
///
/// # Idempotency
/// Identical arguments always produce an identical key.
#[inline]
pub fn milestone_approval_key(contract_id: u32, milestone_index: u32) -> DataKey {
    DataKey::MilestoneApprovals(contract_id, milestone_index)
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Panic with `InvalidContractId` if `contract_id` is the sentinel zero.
///
/// Extracted so future key constructors that also need this guard can reuse
/// the same call site rather than duplicating the condition.
///
/// # Panics
/// Panics with `EscrowError::InvalidContractId` when `contract_id == 0`.
#[inline]
fn require_valid_contract_id(env: &Env, contract_id: u32) {
    if contract_id == 0 {
        env.panic_with_error(EscrowError::InvalidContractId);
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DataKey;
    use soroban_sdk::Env;

    // ── Determinism: same input → same key ───────────────────────────────────

    #[test]
    fn milestone_key_is_deterministic() {
        let env = Env::default();
        assert_eq!(milestone_key(&env, 1), milestone_key(&env, 1));
        assert_eq!(milestone_key(&env, 42), milestone_key(&env, 42));
        assert_eq!(milestone_key(&env, u32::MAX), milestone_key(&env, u32::MAX));
    }

    #[test]
    fn milestone_symbol_is_deterministic() {
        let env = Env::default();
        assert_eq!(milestone_symbol(&env), milestone_symbol(&env));
    }

    #[test]
    fn milestone_approval_key_is_deterministic() {
        assert_eq!(milestone_approval_key(1, 0), milestone_approval_key(1, 0));
        assert_eq!(
            milestone_approval_key(99, 7),
            milestone_approval_key(99, 7)
        );
        assert_eq!(
            milestone_approval_key(u32::MAX, u32::MAX),
            milestone_approval_key(u32::MAX, u32::MAX)
        );
    }

    // ── Uniqueness: different inputs → different keys ─────────────────────────

    #[test]
    fn milestone_keys_are_unique_across_contract_ids() {
        let env = Env::default();
        assert_ne!(milestone_key(&env, 1), milestone_key(&env, 2));
        assert_ne!(milestone_key(&env, 1), milestone_key(&env, u32::MAX));
    }

    #[test]
    fn approval_keys_are_unique_across_contract_ids() {
        // Same milestone index, different contract IDs
        assert_ne!(milestone_approval_key(1, 0), milestone_approval_key(2, 0));
    }

    #[test]
    fn approval_keys_are_unique_across_milestone_indices() {
        // Same contract ID, different milestone indices
        assert_ne!(milestone_approval_key(1, 0), milestone_approval_key(1, 1));
        assert_ne!(
            milestone_approval_key(1, 0),
            milestone_approval_key(1, u32::MAX)
        );
    }

    #[test]
    fn approval_key_does_not_collide_with_released_key() {
        // DataKey::MilestoneApprovals must not equal DataKey::MilestoneReleased
        // for the same (contract_id, milestone_index) pair.
        let approval = DataKey::MilestoneApprovals(1, 0);
        let released = DataKey::MilestoneReleased(1, 0);
        assert_ne!(approval, released);
    }

    #[test]
    fn milestone_storage_key_does_not_collide_with_contract_data_key() {
        // The milestone vector key is a tuple (DataKey::Contract(id), Symbol)
        // while the contract data key is just DataKey::Contract(id).
        // Soroban serialises these differently; the tuple form includes the
        // Symbol and is therefore distinguishable.  This test verifies that
        // the DataKey variants we compose are not themselves equal.
        let data_key = DataKey::Contract(42);
        let approval = DataKey::MilestoneApprovals(42, 0);
        assert_ne!(data_key, approval);
    }

    // ── Zero-ID guard ─────────────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn milestone_key_panics_on_zero_contract_id() {
        let env = Env::default();
        let _ = milestone_key(&env, 0);
    }

    #[test]
    fn milestone_key_accepts_one_as_smallest_valid_id() {
        let env = Env::default();
        // Should not panic
        let _ = milestone_key(&env, 1);
    }

    #[test]
    fn milestone_key_accepts_max_u32() {
        let env = Env::default();
        // Should not panic
        let _ = milestone_key(&env, u32::MAX);
    }

    // ── Approval key boundary tests ───────────────────────────────────────────

    #[test]
    fn approval_key_boundary_contract_and_milestone_zero_index() {
        // contract_id=1 (smallest valid), milestone_index=0 (first milestone)
        let key = milestone_approval_key(1, 0);
        assert_eq!(key, DataKey::MilestoneApprovals(1, 0));
    }

    #[test]
    fn approval_key_boundary_max_values() {
        let key = milestone_approval_key(u32::MAX, u32::MAX);
        assert_eq!(key, DataKey::MilestoneApprovals(u32::MAX, u32::MAX));
    }
}
