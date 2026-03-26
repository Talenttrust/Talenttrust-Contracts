use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient};

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_create_contract_fails_for_same_participants() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let participant = Address::generate(&env);
    let milestones = vec![&env, 200_i128];

    let _ = client.create_contract(&participant, &participant, &milestones);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_create_contract_fails_for_empty_milestones() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let empty_milestones = vec![&env];

    let _ = client.create_contract(&client_addr, &freelancer_addr, &empty_milestones);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_contract_fails_for_non_positive_milestones() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128, 0_i128];

    let _ = client.create_contract(&client_addr, &freelancer_addr, &milestones);
}
