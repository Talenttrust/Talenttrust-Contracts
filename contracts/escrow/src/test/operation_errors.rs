use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{Escrow, EscrowClient};

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_deposit_fails_for_zero_contract_id() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.deposit_funds(&0, &1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_deposit_fails_for_non_positive_amount() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.deposit_funds(&1, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_release_fails_for_zero_contract_id() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.release_milestone(&0, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_release_fails_for_reserved_invalid_milestone_id() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let _ = client.release_milestone(&1, &u32::MAX);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_issue_reputation_fails_for_rating_below_range() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let freelancer = Address::generate(&env);
    let _ = client.issue_reputation(&freelancer, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_issue_reputation_fails_for_rating_above_range() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let freelancer = Address::generate(&env);
    let _ = client.issue_reputation(&freelancer, &6);
}
