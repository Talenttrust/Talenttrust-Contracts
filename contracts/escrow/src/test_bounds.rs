#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, EscrowError, MAX_MILESTONES, MAX_TOTAL_ESCROW_STROOPS};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    (env, contract_id, client_addr, freelancer_addr)
}

// ─── get_bounds ───────────────────────────────────────────────────────────────

#[test]
fn get_bounds_returns_compile_time_constants() {
    let (env, contract_id, _, _) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let bounds = client.get_bounds();
    assert_eq!(bounds.max_milestones, MAX_MILESTONES);
    assert_eq!(bounds.max_total_escrow_stroops, MAX_TOTAL_ESCROW_STROOPS);
}

// ─── Milestone count — boundary ───────────────────────────────────────────────

#[test]
fn create_contract_accepts_exactly_max_milestones() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    // Build a Vec with exactly MAX_MILESTONES elements (each 1 stroop).
    let mut milestones = vec![&env, 1_i128];
    for _ in 1..MAX_MILESTONES {
        milestones.push_back(1_i128);
    }
    assert_eq!(milestones.len(), MAX_MILESTONES);
    // Should succeed without panicking.
    let _id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
}

#[test]
fn create_contract_rejects_milestone_count_above_max() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    // Build a Vec with MAX_MILESTONES + 1 elements.
    let mut milestones = vec![&env, 1_i128];
    for _ in 1..=MAX_MILESTONES {
        milestones.push_back(1_i128);
    }
    assert_eq!(milestones.len(), MAX_MILESTONES + 1);

    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    assert!(result.is_err(), "expected error for count > MAX_MILESTONES");
}

#[test]
fn create_contract_rejects_count_one_above_max_with_error_code() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let mut milestones = vec![&env, 1_i128];
    for _ in 1..=MAX_MILESTONES {
        milestones.push_back(1_i128);
    }
    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    match result {
        Err(Ok(EscrowError::TooManyMilestones)) => {}
        other => panic!("expected TooManyMilestones, got {:?}", other),
    }
}

// ─── Total cap — boundary ─────────────────────────────────────────────────────

#[test]
fn create_contract_accepts_total_exactly_at_cap() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    // Single milestone equal to the hard cap.
    let milestones = vec![&env, MAX_TOTAL_ESCROW_STROOPS];
    let _id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
}

#[test]
fn create_contract_accepts_total_split_across_milestones_at_cap() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    // Two milestones that sum to exactly the cap.
    let half = MAX_TOTAL_ESCROW_STROOPS / 2;
    let remainder = MAX_TOTAL_ESCROW_STROOPS - half;
    let milestones = vec![&env, half, remainder];
    let _id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
}

#[test]
fn create_contract_rejects_total_one_above_cap() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let milestones = vec![&env, MAX_TOTAL_ESCROW_STROOPS + 1];
    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    match result {
        Err(Ok(EscrowError::TotalCapExceeded)) => {}
        other => panic!("expected TotalCapExceeded, got {:?}", other),
    }
}

#[test]
fn create_contract_rejects_total_above_cap_across_milestones() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    // Two milestones that together exceed the cap by 1.
    let half = MAX_TOTAL_ESCROW_STROOPS / 2 + 1;
    let milestones = vec![&env, half, half];
    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    match result {
        Err(Ok(EscrowError::TotalCapExceeded)) => {}
        other => panic!("expected TotalCapExceeded, got {:?}", other),
    }
}

// ─── Overflow safety ──────────────────────────────────────────────────────────

#[test]
fn create_contract_rejects_i128_max_single_milestone() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    // i128::MAX far exceeds MAX_TOTAL_ESCROW_STROOPS; must not panic with
    // overflow — must surface as TotalCapExceeded.
    let milestones = vec![&env, i128::MAX];
    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    match result {
        Err(Ok(EscrowError::TotalCapExceeded)) => {}
        other => panic!("expected TotalCapExceeded for i128::MAX milestone, got {:?}", other),
    }
}

#[test]
fn create_contract_rejects_two_large_milestones_that_would_overflow_i128() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    // Two values whose sum would overflow i128 — must be caught by checked_add
    // and reported as TotalCapExceeded rather than wrapping silently.
    let large = i128::MAX / 2 + 2;
    let milestones = vec![&env, large, large];
    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    match result {
        Err(Ok(EscrowError::TotalCapExceeded)) => {}
        other => panic!("expected TotalCapExceeded for overflow pair, got {:?}", other),
    }
}

// ─── Combined bounds ──────────────────────────────────────────────────────────

#[test]
fn count_check_fires_before_amount_check() {
    // If both count > MAX_MILESTONES and total > cap, TooManyMilestones is
    // returned (count check runs first in create_contract).
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    // MAX_MILESTONES + 1 milestones, each equal to the cap (total >> cap).
    let mut milestones = vec![&env, MAX_TOTAL_ESCROW_STROOPS];
    for _ in 1..=MAX_MILESTONES {
        milestones.push_back(MAX_TOTAL_ESCROW_STROOPS);
    }
    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    match result {
        Err(Ok(EscrowError::TooManyMilestones)) => {}
        other => panic!("expected TooManyMilestones (count check first), got {:?}", other),
    }
}

// ─── Regression: existing valid inputs still accepted ─────────────────────────

#[test]
fn create_contract_still_accepts_original_three_milestone_example() {
    let (env, contract_id, client_addr, freelancer_addr) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    // Amounts from the original test suite — must not be affected by the new
    // bounds (total = 12 billion stroops, well within 10 trillion cap).
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert_eq!(id, 0);
}
