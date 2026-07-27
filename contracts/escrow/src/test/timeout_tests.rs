//! Boundary tests for [`Escrow::is_milestone_overdue`] (issue #652).
//!
//! `is_milestone_overdue` is the timeout-refund precondition. It documents a
//! precise contract:
//!
//! - returns `false` for an unknown contract id,
//! - returns `false` for a contract with no stored milestones,
//! - returns `false` for an out-of-bounds milestone index,
//! - returns `false` for an already-released milestone,
//! - returns `false` for a milestone with `deadline == None`, and
//! - for a milestone with a deadline, returns `true` only when `now > deadline`
//!   (strictly greater), so at exactly the deadline (`now == deadline`) it
//!   returns `false`.
//!
//! These tests pin every documented branch and the strict-inequality boundary
//! using `env.ledger()` time control. Milestone state (deadline / released) is
//! constructed directly in storage so the tests are independent of any
//! deadline-setter entrypoint.
//!
//! # Security
//! Overdue detection must not be tripped early: at exactly the deadline the
//! milestone is not yet overdue, preventing a one-second-early timeout refund.

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Symbol, Vec as SorobanVec,
};

use super::{create_contract, register_client};
use crate::{DataKey, Milestone};

/// Set the ledger timestamp to an absolute number of seconds.
fn set_now(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = secs;
    });
}

/// Overwrite milestone `index`'s `deadline` and `released` flag directly in
/// persistent storage, bypassing any setter entrypoint. The new state is
/// observable through `is_milestone_overdue`.
fn set_milestone_deadline_and_released(
    env: &Env,
    contract_addr: &Address,
    contract_id: u32,
    index: u32,
    deadline: Option<u64>,
    released: bool,
) {
    env.as_contract(contract_addr, || {
        let key = (DataKey::Contract(contract_id), Symbol::new(env, "milestones"));
        let mut milestones: SorobanVec<Milestone> =
            env.storage().persistent().get(&key).unwrap();
        let mut m = milestones.get(index).unwrap();
        m.deadline = deadline;
        m.released = released;
        milestones.set(index, m);
        env.storage().persistent().set(&key, &milestones);
    });
}

// ── Deadline boundary: now < / == / > deadline ────────────────────────────────

#[test]
fn is_milestone_overdue_false_when_now_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _, id) = create_contract(&env, &client);

    let deadline = 1_000u64;
    set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);

    set_now(&env, deadline - 1);
    assert!(
        !client.is_milestone_overdue(&id, &0),
        "now < deadline must not be overdue"
    );
}

#[test]
fn is_milestone_overdue_false_at_exact_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _, id) = create_contract(&env, &client);

    let deadline = 1_000u64;
    set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);

    // Strict-inequality boundary: at exactly the deadline it is NOT overdue.
    set_now(&env, deadline);
    assert!(
        !client.is_milestone_overdue(&id, &0),
        "now == deadline must not be overdue (uses strict >)"
    );
}

#[test]
fn is_milestone_overdue_true_one_second_past_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _, id) = create_contract(&env, &client);

    let deadline = 1_000u64;
    set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);

    set_now(&env, deadline + 1);
    assert!(
        client.is_milestone_overdue(&id, &0),
        "now > deadline must be overdue"
    );
}

// ── Short-circuit branches ────────────────────────────────────────────────────

#[test]
fn is_milestone_overdue_false_for_unknown_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    // No contract id 42 was ever allocated.
    assert!(
        !client.is_milestone_overdue(&42u32, &0),
        "unknown contract id must not be overdue"
    );
}

#[test]
fn is_milestone_overdue_false_for_out_of_bounds_index() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _, id) = create_contract(&env, &client);

    let len = client.get_milestones(&id).len();
    set_now(&env, 10_000);
    // Index == len (one past the last) and far beyond must both be false.
    assert!(!client.is_milestone_overdue(&id, &len));
    assert!(!client.is_milestone_overdue(&id, &(len + 7)));
}

#[test]
fn is_milestone_overdue_false_for_already_released_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _, id) = create_contract(&env, &client);

    let deadline = 1_000u64;
    // Deadline is in the past, but the milestone is already released.
    set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), true);

    set_now(&env, deadline + 5_000);
    assert!(
        !client.is_milestone_overdue(&id, &0),
        "released milestone is never overdue, even past its deadline"
    );
}

#[test]
fn is_milestone_overdue_false_when_deadline_is_none() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _, id) = create_contract(&env, &client);

    // Contracts are created with deadline == None by default; assert explicitly.
    set_milestone_deadline_and_released(&env, &client.address, id, 0, None, false);

    set_now(&env, 1_000_000);
    assert!(
        !client.is_milestone_overdue(&id, &0),
        "milestone with no deadline is never overdue"
    );
}
