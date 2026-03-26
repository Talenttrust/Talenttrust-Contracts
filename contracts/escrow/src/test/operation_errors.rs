//! Tests that verify error codes surfaced as `Error(Contract, #N)`.
//!
//! Each test exercises a single early-validation path to confirm both the
//! specific error code and that the check fires *before* any stateful work.

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{Escrow, EscrowClient};

// ── deposit_funds errors ──────────────────────────────────────────────────────

/// contract_id = 0 maps to ContractNotFound = 1 because no contract is ever
/// stored at that ID (IDs start at 1).
#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_deposit_fails_for_zero_contract_id() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.deposit_funds(&0, &1_000);
}

/// amount = 0 triggers InvalidAmount = 3.  The amount check happens before the
/// contract is loaded, so the contract not existing at id=1 is irrelevant.
#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_deposit_fails_for_non_positive_amount() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.deposit_funds(&1, &0);
}

// ── release_milestone errors ──────────────────────────────────────────────────

/// contract_id = 0 maps to ContractNotFound = 1.
#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_release_fails_for_zero_contract_id() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.release_milestone(&0, &0);
}

/// milestone_id = u32::MAX is the reserved invalid sentinel; it maps to
/// MilestoneNotFound = 2.  This check precedes the contract load so the
/// contract at id=1 does not need to exist.
#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_release_fails_for_reserved_invalid_milestone_id() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.release_milestone(&1, &u32::MAX);
}

// ── issue_reputation errors ───────────────────────────────────────────────────

/// rating = 0 is below the minimum of 1; maps to InvalidRating = 4.
/// The rating check happens before the contract load, so the contract at id=1
/// does not need to exist.
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_issue_reputation_fails_for_rating_below_range() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.issue_reputation(&1_u32, &0);
}

/// rating = 6 exceeds the maximum of 5; maps to InvalidRating = 4.
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_issue_reputation_fails_for_rating_above_range() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.issue_reputation(&1_u32, &6);
}

// ── deposit_funds on completed contract ──────────────────────────────────────

/// Attempting to deposit into an already-completed contract returns
/// InvalidState (#8).  A contract becomes Completed when all milestones are
/// released.  Using try_deposit_funds avoids a should_panic boundary so that
/// the return-Err branch inside deposit_funds is properly instrumented.
#[test]
fn test_deposit_fails_for_completed_contract() {
    use crate::{Escrow, EscrowClient, EscrowError};
    use soroban_sdk::{testutils::Address as _, vec, Address, Env};

    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    // Single-milestone contract so one release completes it.
    let id = client.create_contract(&client_addr, &freelancer, &vec![&env, 100_i128]);
    client.deposit_funds(&id, &100_i128);
    client.release_milestone(&id, &0);
    // Contract is now Completed; another deposit must be rejected.
    let result = client.try_deposit_funds(&id, &100_i128);
    assert_eq!(result, Err(Ok(EscrowError::InvalidState)));
}
