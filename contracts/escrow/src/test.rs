#![cfg(test)]

mod cancel_contract;
mod dispute;
use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Vec};

use crate::{Contract, ContractStatus, Escrow, EscrowClient, Milestone, ReleaseAuthorization};

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
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    (client_addr, freelancer_addr, contract_id)
// Test helper functions
pub fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    (env, client_addr, freelancer_addr)
}

pub fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

pub fn register_escrow(env: &Env) -> EscrowClient {
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
}

pub fn register_client(env: &Env) -> EscrowClient {
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
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
    let result = client.deposit_funds(&id, &1_000_0000000, &client_addr);
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
    client.deposit_funds(&id, &1_000_0000000, &client_addr);

    // Now release milestone
    let result = client.release_milestone(&id, &0, &client_addr);
    assert!(result);
pub fn generated_participants(env: &Env) -> (Address, Address, Address) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    (client_addr, freelancer_addr, arbiter_addr)
}

pub fn default_milestones(env: &Env) -> Vec<i128> {
    vec![env, 1000_0000000_i128, 2000_0000000_i128, 3000_0000000_i128]
}

pub fn total_milestones() -> i128 {
    6000_0000000_i128
}

pub fn create_client(env: &Env) -> EscrowClient {
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
}

pub fn create_default_contract(
    env: &Env,
    client: &EscrowClient,
    client_addr: &Address,
    freelancer_addr: &Address,
) -> u32 {
    let milestones = vec![env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    client.create_contract(
        client_addr,
        freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    )
}

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    assert!(client.deposit_funds(&contract_id, &1_000_0000000, &client_addr)); // Deposit: 1000
    assert!(client.release_milestone(&contract_id, &0, &client_addr)); // Release: 200
    assert!(client.finalize_contract(&contract_id, &client_addr));
pub fn assert_contract_state(
    contract: Contract,
    expected_status: ContractStatus,
    expected_funded: i128,
    expected_released: i128,
    expected_refunded: i128,
) {
    assert_eq!(contract.status, expected_status);
    assert_eq!(contract.funded_amount, expected_funded);
    assert_eq!(contract.released_amount, expected_released);
    assert_eq!(contract.refunded_amount, expected_refunded);
}

pub fn assert_milestone_flags(
    milestones: Vec<Milestone>,
    index: u32,
    expected_released: bool,
    expected_refunded: bool,
) {
    let milestone = milestones.get(index).unwrap();
    assert_eq!(milestone.released, expected_released);
    assert_eq!(milestone.refunded, expected_refunded);
}

#[test]
fn test_hello() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    assert!(client.deposit_funds(&contract_id, &1_000_0000000, &client_addr));
    assert!(client.release_milestone(&contract_id, &0, &client_addr));

    // Try to withdraw without finalization
    client.withdraw_leftover(&contract_id, &client_addr);
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
    let unauthorized_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    assert!(client.deposit_funds(&contract_id, &1_000_0000000, &client_addr));
    assert!(client.release_milestone(&contract_id, &0, &client_addr));
    assert!(client.finalize_contract(&contract_id, &client_addr));
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);
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

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    assert!(client.deposit_funds(&contract_id, &600_0000000, &client_addr)); // Deposit exactly 600
    assert!(client.release_milestone(&contract_id, &0, &client_addr)); // Release: 200
    assert!(client.release_milestone(&contract_id, &1, &client_addr)); // Release: 400
    assert!(client.finalize_contract(&contract_id, &client_addr));
fn test_deposit_funds() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    let result = client.deposit_funds(&contract_id, &client_addr, &1_000_0000000);
    assert!(result);
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

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    assert!(client.deposit_funds(&contract_id, &1_000_0000000, &client_addr));
    assert!(client.release_milestone(&contract_id, &0, &client_addr));
    assert!(client.finalize_contract(&contract_id, &client_addr));

    // First withdrawal should succeed
    let _withdrawn = client.withdraw_leftover(&contract_id, &client_addr);
fn test_release_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &1_200_0000000_i128));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    let result = client.release_milestone(&contract_id, &client_addr, &0);
    assert!(result);
}

// Include test modules
mod refund;
mod release;
mod deposit;
mod create_contract;
mod access_control;
mod approval_expiry;
mod hello;
mod lifecycle;
mod flows;
mod security;
mod storage;
mod persistence;
mod performance;
mod input_sanitization_amounts;
mod input_sanitization_identities;
mod milestone_schedule;
mod governance;
mod emergency_controls;
mod pause_controls;
mod timeout_tests;
mod mainnet_readiness;
mod client_migration;
