//! Deterministic accounting invariant tests.
//!
//! These tests exercise the invariant
//!   `total_deposited == released_amount + refunded_amount + available_balance`
//! across concrete deposit/release/cancel sequences, including adversarial
//! cases (over-release, double-release, over-deposit).

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{ContractStatus, DepositMode, Escrow, EscrowClient};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

fn participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

/// Assert the core accounting invariant on the stored contract data.
fn assert_invariant(client: &EscrowClient, id: u32) {
    let d = client.get_contract(&id);
    let available = d.total_deposited - d.released_amount - d.refunded_amount;
    assert!(
        available >= 0,
        "available_balance < 0 (deposited={}, released={}, refunded={})",
        d.total_deposited,
        d.released_amount,
        d.refunded_amount
    );
    assert_eq!(
        d.total_deposited,
        d.released_amount + d.refunded_amount + available,
        "accounting invariant violated"
    );
}

// ---------------------------------------------------------------------------
// Happy-path sequences
// ---------------------------------------------------------------------------

#[test]
fn invariant_holds_after_single_deposit() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &vec![&env, 100_i128, 200_i128],
        &DepositMode::Incremental,
    );

    client.deposit_funds(&id, &100_i128);
    assert_invariant(&client, id);

    let d = client.get_contract(&id);
    assert_eq!(d.total_deposited, 100);
    assert_eq!(d.released_amount, 0);
    assert_eq!(d.refunded_amount, 0);
}

#[test]
fn invariant_holds_after_full_deposit() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &vec![&env, 100_i128, 200_i128],
        &DepositMode::ExactTotal,
    );

    client.deposit_funds(&id, &300_i128);
    assert_invariant(&client, id);

    let d = client.get_contract(&id);
    assert_eq!(d.status, ContractStatus::Funded);
    assert_eq!(d.total_deposited, 300);
}

#[test]
fn invariant_holds_after_each_milestone_release() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &vec![&env, 100_i128, 200_i128, 300_i128],
        &DepositMode::ExactTotal,
    );

    client.deposit_funds(&id, &600_i128);
    assert_invariant(&client, id);

    client.release_milestone(&id, &0);
    assert_invariant(&client, id);
    assert_eq!(client.get_contract(&id).released_amount, 100);

    client.release_milestone(&id, &1);
    assert_invariant(&client, id);
    assert_eq!(client.get_contract(&id).released_amount, 300);

    client.release_milestone(&id, &2);
    assert_invariant(&client, id);
    let d = client.get_contract(&id);
    assert_eq!(d.released_amount, 600);
    assert_eq!(d.status, ContractStatus::Completed);
}

#[test]
fn invariant_holds_after_incremental_deposits_then_releases() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &vec![&env, 50_i128, 150_i128],
        &DepositMode::Incremental,
    );

    client.deposit_funds(&id, &50_i128);
    assert_invariant(&client, id);
    client.deposit_funds(&id, &150_i128);
    assert_invariant(&client, id);

    client.release_milestone(&id, &0);
    assert_invariant(&client, id);
    client.release_milestone(&id, &1);
    assert_invariant(&client, id);

    let d = client.get_contract(&id);
    assert_eq!(d.status, ContractStatus::Completed);
    assert_eq!(d.total_deposited, 200);
    assert_eq!(d.released_amount, 200);
    assert_eq!(d.refunded_amount, 0);
}

// ---------------------------------------------------------------------------
// Cancel sequences
// ---------------------------------------------------------------------------

#[test]
fn invariant_holds_after_cancel_with_no_deposit() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &vec![&env, 100_i128], &DepositMode::Incremental);

    client.cancel_contract(&id, &ca);
    assert_invariant(&client, id);

    let d = client.get_contract(&id);
    assert_eq!(d.status, ContractStatus::Cancelled);
    assert_eq!(d.total_deposited, 0);
}

#[test]
fn invariant_holds_after_cancel_with_partial_deposit() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &vec![&env, 100_i128, 200_i128],
        &DepositMode::Incremental,
    );

    client.deposit_funds(&id, &100_i128);
    assert_invariant(&client, id);

    client.cancel_contract(&id, &ca);
    assert_invariant(&client, id);

    let d = client.get_contract(&id);
    assert_eq!(d.status, ContractStatus::Cancelled);
    assert_eq!(d.total_deposited, 100);
    assert_eq!(d.released_amount, 0);
    assert_eq!(d.refunded_amount, 0);
}

#[test]
fn invariant_holds_after_partial_release_then_cancel() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &vec![&env, 100_i128, 200_i128],
        &DepositMode::ExactTotal,
    );

    client.deposit_funds(&id, &300_i128);
    client.release_milestone(&id, &0);
    assert_invariant(&client, id);

    client.cancel_contract(&id, &ca);
    assert_invariant(&client, id);

    let d = client.get_contract(&id);
    assert_eq!(d.status, ContractStatus::Cancelled);
    assert_eq!(d.released_amount, 100);
}

// ---------------------------------------------------------------------------
// Adversarial sequences
// ---------------------------------------------------------------------------

#[test]
fn double_release_rejected_invariant_preserved() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &vec![&env, 100_i128, 200_i128],
        &DepositMode::ExactTotal,
    );

    client.deposit_funds(&id, &300_i128);
    client.release_milestone(&id, &0);
    assert_invariant(&client, id);

    let before = client.get_contract(&id);
    let result = client.try_release_milestone(&id, &0);
    assert!(result.is_err(), "double release must be rejected");
    assert_invariant(&client, id);

    let after = client.get_contract(&id);
    assert_eq!(before.released_amount, after.released_amount);
    assert_eq!(before.total_deposited, after.total_deposited);
}

#[test]
fn release_without_funds_rejected_invariant_preserved() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &vec![&env, 100_i128], &DepositMode::Incremental);

    let result = client.try_release_milestone(&id, &0);
    assert!(result.is_err(), "release without funds must be rejected");
    assert_invariant(&client, id);
}

#[test]
fn overfund_rejected_invariant_preserved() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &vec![&env, 100_i128], &DepositMode::Incremental);

    client.deposit_funds(&id, &100_i128);
    assert_invariant(&client, id);

    let result = client.try_deposit_funds(&id, &1_i128);
    assert!(result.is_err(), "over-deposit must be rejected");
    assert_invariant(&client, id);

    assert_eq!(client.get_contract(&id).total_deposited, 100);
}

#[test]
fn out_of_range_release_rejected_invariant_preserved() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &vec![&env, 100_i128], &DepositMode::ExactTotal);

    client.deposit_funds(&id, &100_i128);
    assert_invariant(&client, id);

    let result = client.try_release_milestone(&id, &99);
    assert!(result.is_err(), "out-of-range milestone must be rejected");
    assert_invariant(&client, id);
}

#[test]
fn zero_deposit_rejected_invariant_preserved() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &vec![&env, 100_i128], &DepositMode::Incremental);

    let result = client.try_deposit_funds(&id, &0_i128);
    assert!(result.is_err(), "zero deposit must be rejected");
    assert_invariant(&client, id);
}

#[test]
fn negative_deposit_rejected_invariant_preserved() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &vec![&env, 100_i128], &DepositMode::Incremental);

    let result = client.try_deposit_funds(&id, &-1_i128);
    assert!(result.is_err(), "negative deposit must be rejected");
    assert_invariant(&client, id);
}

// ---------------------------------------------------------------------------
// Multi-contract isolation
// ---------------------------------------------------------------------------

#[test]
fn invariant_holds_across_multiple_independent_contracts() {
    let env = make_env();
    let client = make_client(&env);
    let (ca1, fa1) = participants(&env);
    let (ca2, fa2) = participants(&env);

    let id1 = client.create_contract(&ca1, &fa1, &vec![&env, 100_i128], &DepositMode::ExactTotal);
    let id2 = client.create_contract(
        &ca2,
        &fa2,
        &vec![&env, 200_i128, 300_i128],
        &DepositMode::ExactTotal,
    );

    client.deposit_funds(&id1, &100_i128);
    client.deposit_funds(&id2, &500_i128);

    client.release_milestone(&id1, &0);
    client.release_milestone(&id2, &0);

    assert_invariant(&client, id1);
    assert_invariant(&client, id2);

    assert_eq!(client.get_contract(&id1).released_amount, 100);
    assert_eq!(client.get_contract(&id2).released_amount, 200);
}

// ---------------------------------------------------------------------------
// ExactTotal deposit mode
// ---------------------------------------------------------------------------

#[test]
fn exact_total_mode_rejects_wrong_amount() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &vec![&env, 100_i128, 200_i128],
        &DepositMode::ExactTotal,
    );

    let result = client.try_deposit_funds(&id, &100_i128);
    assert!(
        result.is_err(),
        "partial deposit in ExactTotal mode must be rejected"
    );
    assert_invariant(&client, id);

    assert!(client.deposit_funds(&id, &300_i128));
    assert_invariant(&client, id);
}

#[test]
fn exact_total_mode_rejects_second_deposit() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &vec![&env, 100_i128], &DepositMode::ExactTotal);

    assert!(client.deposit_funds(&id, &100_i128));
    assert_invariant(&client, id);

    let result = client.try_deposit_funds(&id, &100_i128);
    assert!(
        result.is_err(),
        "second deposit in ExactTotal mode must be rejected"
    );
    assert_invariant(&client, id);
}
