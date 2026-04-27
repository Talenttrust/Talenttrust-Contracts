#![cfg(test)]

mod cancel_contract;

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

use crate::{ContractStatus, Escrow, EscrowClient};

mod performance;

fn register_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

fn create_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &None, &None);
    (client_addr, freelancer_addr, contract_id)
}

fn total_milestone_amount() -> i128 {
    200_0000000 + 400_0000000 + 600_0000000
}

#[test]
fn test_hello() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

#[test]
fn test_create_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &None, &None);
    assert_eq!(id, 0);

    // Verify contract was created with correct status
    let contract = client.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Created);
}

#[test]
fn test_deposit_funds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Create a contract first
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &None, &None);

    // Now deposit
    let result = client.deposit_funds(&id, &1_000_0000000);
    assert!(result);
}

#[test]
fn test_release_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Create and fund a contract first
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &None, &None);
    client.deposit_funds(&id, &1_000_0000000);

    // Now release milestone
    let result = client.release_milestone(&id, &0);
    assert!(result);
}

#[test]
fn test_cancel_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];

    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &None, &None);
    
    // Cancel as client
    let result = client.cancel_contract(&id, &client_addr);
    assert!(result);

    let contract = client.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Cancelled);
}
