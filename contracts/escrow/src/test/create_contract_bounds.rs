// Tests for every input-validation guard in `create_contract`.
//
// Guards (in execution order):
//   1. client == freelancer              → InvalidParticipant
//   2. milestone_amounts.is_empty()      → EmptyMilestones
//   3. len > MAX_MILESTONES (10)         → TooManyMilestones
//   4. len == MAX_MILESTONES             → succeeds
//   5. any amount <= 0                   → InvalidMilestoneAmount
//   6. safe_add_amounts overflow         → PotentialOverflow
//   7. total > MAX_TOTAL_ESCROW_STROOPS  → InvalidMilestoneAmount

use soroban_sdk::{testutils::Address as _, vec, Address, Env, Vec};

use crate::{
    DepositMode, Escrow, EscrowClient, EscrowError, MAX_MILESTONES, MAX_TOTAL_ESCROW_STROOPS,
};

// Returns (env, contract_address). Each test creates EscrowClient locally so
// the borrow of `env` stays in the same scope — same pattern as pause_controls.
fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

fn assert_err(
    result: Result<
        Result<u32, soroban_sdk::ConversionError>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
    expected: EscrowError,
) {
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = expected.into();
            assert_eq!(e, want, "wrong error: expected {:?}", expected);
        }
        other => panic!("expected {:?}, got {:?}", expected, other),
    }
}

// guard 1 ─────────────────────────────────────────────────────────────────────

#[test]
fn rejects_same_client_and_freelancer() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let same = Address::generate(&env);
    assert_err(
        client.try_create_contract(&same, &same, &vec![&env, 100_i128], &DepositMode::ExactTotal),
        EscrowError::InvalidParticipant,
    );
}

// guard 2 ─────────────────────────────────────────────────────────────────────

#[test]
fn rejects_empty_milestones() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_err(
        client.try_create_contract(&c, &f, &Vec::new(&env), &DepositMode::ExactTotal),
        EscrowError::EmptyMilestones,
    );
}

// guard 3 ─────────────────────────────────────────────────────────────────────

#[test]
fn rejects_one_over_max_milestones() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let mut amounts: Vec<i128> = Vec::new(&env);
    for _ in 0..=MAX_MILESTONES {
        amounts.push_back(1_i128);
    }
    assert_eq!(amounts.len(), MAX_MILESTONES + 1);
    assert_err(
        client.try_create_contract(&c, &f, &amounts, &DepositMode::ExactTotal),
        EscrowError::TooManyMilestones,
    );
}

// guard 4 — boundary success ──────────────────────────────────────────────────

#[test]
fn accepts_exactly_max_milestones() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let mut amounts: Vec<i128> = Vec::new(&env);
    for _ in 0..MAX_MILESTONES {
        amounts.push_back(1_i128);
    }
    assert_eq!(amounts.len(), MAX_MILESTONES);
    client.create_contract(&c, &f, &amounts, &DepositMode::ExactTotal);
}

// guard 5 ─────────────────────────────────────────────────────────────────────

#[test]
fn rejects_zero_milestone_amount() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_err(
        client.try_create_contract(&c, &f, &vec![&env, 0_i128], &DepositMode::ExactTotal),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn rejects_negative_milestone_amount() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_err(
        client.try_create_contract(&c, &f, &vec![&env, -1_i128], &DepositMode::ExactTotal),
        EscrowError::InvalidMilestoneAmount,
    );
}

// guard 6 — overflow caught before cap check ──────────────────────────────────

#[test]
fn rejects_amounts_that_would_overflow_i128() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    // Both > i128::MAX / 2, so checked_add returns None on the second iteration.
    let large = i128::MAX / 2 + 2;
    assert_err(
        client.try_create_contract(&c, &f, &vec![&env, large, large], &DepositMode::ExactTotal),
        EscrowError::PotentialOverflow,
    );
}

// guard 7 ─────────────────────────────────────────────────────────────────────

#[test]
fn accepts_total_exactly_at_cap() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    client.create_contract(
        &c,
        &f,
        &vec![&env, MAX_TOTAL_ESCROW_STROOPS],
        &DepositMode::ExactTotal,
    );
}

#[test]
fn rejects_total_one_over_cap() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_err(
        client.try_create_contract(
            &c,
            &f,
            &vec![&env, MAX_TOTAL_ESCROW_STROOPS + 1],
            &DepositMode::ExactTotal,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn rejects_multi_milestone_total_over_cap() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let half = MAX_TOTAL_ESCROW_STROOPS / 2 + 1;
    assert_err(
        client.try_create_contract(&c, &f, &vec![&env, half, half], &DepositMode::ExactTotal),
        EscrowError::InvalidMilestoneAmount,
    );
}

// ordering ────────────────────────────────────────────────────────────────────

// When both count > MAX_MILESTONES and total > cap, TooManyMilestones wins
// because the count guard runs first in create_contract.
#[test]
fn count_guard_fires_before_amount_guard() {
    let (env, cid) = setup();
    let client = EscrowClient::new(&env, &cid);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let mut amounts: Vec<i128> = Vec::new(&env);
    for _ in 0..=MAX_MILESTONES {
        amounts.push_back(MAX_TOTAL_ESCROW_STROOPS);
    }
    assert_err(
        client.try_create_contract(&c, &f, &amounts, &DepositMode::ExactTotal),
        EscrowError::TooManyMilestones,
    );
}
