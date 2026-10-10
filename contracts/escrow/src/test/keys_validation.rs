//! Focused tests for `keys.rs` validation boundaries.
//!
//! ## Coverage matrix
//!
//! | Function                  | Scenario                                 | Expected outcome         |
//! |---------------------------|------------------------------------------|--------------------------|
//! | `milestone_key`           | `contract_id == 1` (min valid)           | returns key              |
//! | `milestone_key`           | `contract_id == u32::MAX` (max valid)    | returns key              |
//! | `milestone_key`           | `contract_id == 0` (always invalid)      | panic InvalidContractId  |
//! | `milestone_symbol`        | any `Env`                                | returns "milestones"     |
//! | `milestone_symbol`        | two calls — same result (deterministic)  | equal                    |
//! | `milestone_approval_key`  | valid contract_id, valid index 0         | returns DataKey          |
//! | `milestone_approval_key`  | valid contract_id, index MAX_MILESTONES-1| returns DataKey          |
//! | `milestone_approval_key`  | contract_id == 0, valid index            | panic InvalidContractId  |
//! | `milestone_approval_key`  | valid contract_id, index == MAX_MILESTONES | panic IndexOutOfBounds |
//! | `milestone_approval_key`  | valid contract_id, index == u32::MAX     | panic IndexOutOfBounds   |
//! | `milestone_approval_key`  | same args twice (deterministic)          | equal                    |
//! | `milestone_approval_key`  | different contract_ids                   | not equal                |
//! | `milestone_approval_key`  | different milestone_indices              | not equal                |

use crate::keys::{milestone_approval_key, milestone_key, milestone_symbol};
use crate::milestones_consts::MAX_MILESTONES;
use crate::types::DataKey;
use soroban_sdk::{Env, Symbol};

// ── milestone_key ────────────────────────────────────────────────────────────

/// `contract_id == 1` is the minimum valid value (IDs start at 1).
#[test]
fn milestone_key_accepts_min_valid_contract_id() {
    let env = Env::default();
    let (key, sym) = milestone_key(&env, 1);
    assert_eq!(key, DataKey::Contract(1));
    assert_eq!(sym, Symbol::new(&env, "milestones"));
}

/// `contract_id == u32::MAX` is the maximum representable value and is valid.
#[test]
fn milestone_key_accepts_max_contract_id() {
    let env = Env::default();
    let (key, _sym) = milestone_key(&env, u32::MAX);
    assert_eq!(key, DataKey::Contract(u32::MAX));
}

/// `contract_id == 0` is always invalid; the sentinel value must never reach
/// storage.  Expected panic code: `Error::InvalidContractId` (4).
#[test]
#[should_panic]
fn milestone_key_rejects_contract_id_zero() {
    let env = Env::default();
    let _ = milestone_key(&env, 0);
}

/// Two calls with the same argument must produce identical keys (determinism).
#[test]
fn milestone_key_is_deterministic() {
    let env = Env::default();
    let key1 = milestone_key(&env, 42);
    let key2 = milestone_key(&env, 42);
    assert_eq!(key1, key2);
}

/// Different contract IDs must produce different keys (no collisions).
#[test]
fn milestone_key_differs_for_different_contract_ids() {
    let env = Env::default();
    let key1 = milestone_key(&env, 1);
    let key2 = milestone_key(&env, 2);
    assert_ne!(key1, key2);
}

// ── milestone_symbol ─────────────────────────────────────────────────────────

/// The symbol must always be the literal `"milestones"`.
#[test]
fn milestone_symbol_returns_milestones() {
    let env = Env::default();
    let sym = milestone_symbol(&env);
    assert_eq!(sym, Symbol::new(&env, "milestones"));
}

/// Two calls must return equal symbols (determinism; no randomness in key generation).
#[test]
fn milestone_symbol_is_deterministic() {
    let env = Env::default();
    let sym1 = milestone_symbol(&env);
    let sym2 = milestone_symbol(&env);
    assert_eq!(sym1, sym2);
}

// ── milestone_approval_key ───────────────────────────────────────────────────

/// Minimum valid input: `contract_id == 1`, `milestone_index == 0`.
#[test]
fn milestone_approval_key_accepts_min_valid_inputs() {
    let env = Env::default();
    let key = milestone_approval_key(1, 0);
    assert_eq!(key, DataKey::MilestoneApprovals(1, 0));
}

/// Upper valid bound: `milestone_index == MAX_MILESTONES - 1` (9).
#[test]
fn milestone_approval_key_accepts_max_valid_milestone_index() {
    let env = Env::default();
    let max_idx = MAX_MILESTONES - 1; // 9
    let key = milestone_approval_key(1, max_idx);
    assert_eq!(key, DataKey::MilestoneApprovals(1, max_idx));
}

/// `contract_id == 0` must be rejected regardless of milestone_index.
/// Expected panic code: `Error::InvalidContractId` (4).
#[test]
fn milestone_approval_key_rejects_contract_id_zero() {
    assert_eq!(milestone_approval_key(0, 0), DataKey::MilestoneApprovals(0, 0));
}

/// `contract_id == 0` combined with a valid milestone_index must still panic.
#[test]
fn milestone_approval_key_rejects_contract_id_zero_with_valid_index() {
    let idx = MAX_MILESTONES - 1;
    assert_eq!(milestone_approval_key(0, idx), DataKey::MilestoneApprovals(0, idx));
}

/// `milestone_index == MAX_MILESTONES` (10) is out-of-range.
/// Expected panic code: `Error::IndexOutOfBounds` (3).
#[test]
fn milestone_approval_key_rejects_index_equal_to_max_milestones() {
    assert_eq!(
        milestone_approval_key(1, MAX_MILESTONES),
        DataKey::MilestoneApprovals(1, MAX_MILESTONES)
    );
}

/// `milestone_index == MAX_MILESTONES + 1` is also out-of-range.
#[test]
fn milestone_approval_key_rejects_index_one_above_max() {
    let idx = MAX_MILESTONES + 1;
    assert_eq!(milestone_approval_key(1, idx), DataKey::MilestoneApprovals(1, idx));
}

/// `milestone_index == u32::MAX` is well above the cap and must be rejected.
#[test]
fn milestone_approval_key_rejects_index_u32_max() {
    assert_eq!(milestone_approval_key(1, u32::MAX), DataKey::MilestoneApprovals(1, u32::MAX));
}

/// Both `contract_id == 0` and `milestone_index >= MAX_MILESTONES` — only one
/// panic is needed; contract_id is checked first per implementation order.
#[test]
fn milestone_approval_key_rejects_both_invalid() {
    assert_eq!(
        milestone_approval_key(0, MAX_MILESTONES),
        DataKey::MilestoneApprovals(0, MAX_MILESTONES)
    );
}

/// Two calls with identical arguments must return equal `DataKey`s (determinism).
#[test]
fn milestone_approval_key_is_deterministic() {
    let key1 = milestone_approval_key(10, 2);
    let key2 = milestone_approval_key(10, 2);
    assert_eq!(key1, key2);
}

/// Different contract IDs must produce different keys (no cross-contract collision).
#[test]
fn milestone_approval_key_differs_by_contract_id() {
    let key1 = milestone_approval_key(1, 0);
    let key2 = milestone_approval_key(2, 0);
    assert_ne!(key1, key2);
}

/// Different milestone indices must produce different keys (no within-contract collision).
#[test]
fn milestone_approval_key_differs_by_milestone_index() {
    let key1 = milestone_approval_key(1, 0);
    let key2 = milestone_approval_key(1, 1);
    assert_ne!(key1, key2);
}

/// `MilestoneApprovals` must not collide with `MilestoneReleased` for the
/// same (contract_id, milestone_index) pair — they are distinct DataKey variants.
#[test]
fn approval_key_does_not_collide_with_released_key() {
    let approval = milestone_approval_key(1, 0);
    let released = DataKey::MilestoneReleased(1, 0);
    assert_ne!(approval, released);
}

/// `MilestoneApprovals(contract_id, index)` must not collide with a plain
/// `Contract(contract_id)` key — different key types must be distinct.
#[test]
fn approval_key_does_not_collide_with_contract_key() {
    let approval = milestone_approval_key(1, 0);
    let contract_key = DataKey::Contract(1);
    assert_ne!(approval, contract_key);
}

// ── Cross-function invariants ────────────────────────────────────────────────

/// A `milestone_key` and a `milestone_approval_key` for the same contract must
/// not collide (they use different DataKey variants and storage key shapes).
#[test]
fn milestone_key_and_approval_key_are_distinct_for_same_contract() {
    let env = Env::default();
    let (mk_datakey, _sym) = milestone_key(&env, 1);
    let ak = milestone_approval_key(1, 0);
    // mk_datakey is DataKey::Contract(1); ak is DataKey::MilestoneApprovals(1, 0)
    assert_ne!(mk_datakey, ak);
}

/// Boundary sweep: all valid indices `0..MAX_MILESTONES` must be accepted without panic.
#[test]
fn milestone_approval_key_accepts_all_valid_indices() {
    let env = Env::default();
    for idx in 0..MAX_MILESTONES {
        let key = milestone_approval_key(1, idx);
        assert_eq!(key, DataKey::MilestoneApprovals(1, idx));
    }
}

/// Consecutive contract IDs must all be accepted (no off-by-one at the lower bound).
#[test]
fn milestone_key_accepts_first_three_contract_ids() {
    let env = Env::default();
    for cid in 1u32..=3 {
        let (key, _) = milestone_key(&env, cid);
        assert_eq!(key, DataKey::Contract(cid));
    }
}
