#![cfg(test)]

mod cancel_contract;

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

use crate::{ContractStatus, Escrow, EscrowClient};

mod performance;

fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

fn create_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    (client_addr, freelancer_addr, contract_id)
}

fn total_milestone_amount() -> i128 {
    200_0000000 + 400_0000000 + 600_0000000
}

mod ttl_tests;

mod milestone_schedule_migration;

#[test]
fn test_hello() {
    let env = new_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

#[test]
fn test_create_contract() {
    let env = new_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    assert_eq!(id, 0);

    // Verify contract was created with correct status
    let contract = client.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Created);
}

#[test]
fn test_deposit_funds() {
    let env = new_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Create a contract first
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);

    // Now deposit
    let result = client.deposit_funds(&id, &1_000_0000000);
    assert!(result);
}

#[test]
fn test_release_milestone() {
    let env = new_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Create and fund a contract first
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    client.deposit_funds(&id, &1_000_0000000);

    // Now release milestone
    let result = client.release_milestone(&id, &0);
    assert!(result);
}

#[test]
fn test_withdraw_leftover_success() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128]; // Total: 600

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert!(client.deposit_funds(&contract_id, &1_000_0000000, &client_addr)); // Deposit: 1000
    assert!(client.release_milestone(&contract_id, &0, &client_addr)); // Release: 200
    assert!(client.finalize_contract(&contract_id, &client_addr));

    // Leftover should be: 1000 - 200 = 800
    let withdrawn = client.withdraw_leftover(&contract_id, &client_addr);
    assert_eq!(withdrawn, 800_0000000);
}

#[test]
#[should_panic]
fn test_withdraw_leftover_before_finalization() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert!(client.deposit_funds(&contract_id, &1_000_0000000, &client_addr));
    assert!(client.release_milestone(&contract_id, &0, &client_addr));

    // Try to withdraw without finalization
    client.withdraw_leftover(&contract_id, &client_addr);
}

#[test]
#[should_panic]
fn test_withdraw_leftover_unauthorized() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let unauthorized_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert!(client.deposit_funds(&contract_id, &1_000_0000000, &client_addr));
    assert!(client.release_milestone(&contract_id, &0, &client_addr));
    assert!(client.finalize_contract(&contract_id, &client_addr));

    // Try to withdraw as unauthorized user
    client.withdraw_leftover(&contract_id, &unauthorized_addr);
}

#[test]
#[should_panic]
fn test_withdraw_leftover_no_funds() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128]; // Total: 600

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert!(client.deposit_funds(&contract_id, &600_0000000, &client_addr)); // Deposit exactly 600
    assert!(client.release_milestone(&contract_id, &0, &client_addr)); // Release: 200
    assert!(client.release_milestone(&contract_id, &1, &client_addr)); // Release: 400
    assert!(client.finalize_contract(&contract_id, &client_addr));

    // No leftover should remain
    client.withdraw_leftover(&contract_id, &client_addr);
}

#[test]
#[should_panic]
fn test_withdraw_leftover_double_withdraw() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert!(client.deposit_funds(&contract_id, &1_000_0000000, &client_addr));
    assert!(client.release_milestone(&contract_id, &0, &client_addr));
    assert!(client.finalize_contract(&contract_id, &client_addr));

    // First withdrawal should succeed
    let _withdrawn = client.withdraw_leftover(&contract_id, &client_addr);

    // Second withdrawal should fail
    client.withdraw_leftover(&contract_id, &client_addr);
}
