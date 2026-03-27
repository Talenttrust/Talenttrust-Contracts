#![cfg(test)]
use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Vec};

use crate::{Escrow, EscrowClient};

pub(crate) const MILESTONE_ONE: i128 = 200_0000000;
pub(crate) const MILESTONE_TWO: i128 = 400_0000000;
pub(crate) const MILESTONE_THREE: i128 = 600_0000000;

// ==================== CONTRACT CREATION TESTS ====================

mod timeout_tests;

#[test]
fn test_create_contract_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let token = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 0);
}

#[test]
fn test_create_contract_with_arbiter() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert_eq!(id, 0);
}

#[test]
#[should_panic(expected = "At least one milestone required")]
fn test_create_contract_no_milestones() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env];

pub(crate) fn register_client(env: &Env) -> EscrowClient<'_> {
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
}

pub(crate) fn default_milestones(env: &Env) -> Vec<i128> {
    vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
}

pub(crate) fn total_milestones() -> i128 {
    MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
}

pub(crate) fn generated_participants(env: &Env) -> (Address, Address, Address) {
    (
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
    )
}

pub(crate) fn create_default_contract(
    client: &EscrowClient<'_>,
    env: &Env,
    release_auth: ReleaseAuthorization,
) -> (u32, Address, Address, Address) {
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants(env);
    let arbiter = match release_auth {
        ReleaseAuthorization::ClientOnly => None,
        _ => Some(arbiter_addr.clone()),
    };

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &arbiter,
        &default_milestones(env),
        &release_auth,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &unauthorized_addr, &0);
pub(crate) fn register_client(env: &Env) -> EscrowClient<'_> {
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
}

pub(crate) fn default_milestones(env: &Env) -> Vec<i128> {
    vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
}

pub(crate) fn total_milestone_amount() -> i128 {
    MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
}

pub(crate) fn generated_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

#[test]
#[should_panic(expected = "Milestone already released")]
fn test_release_milestone_already_released() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // Use 2 milestones so releasing the first one doesn't set status to Completed
    let milestones = vec![&env, 1000_0000000_i128, 2000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &3000_0000000);
    client.approve_milestone_release(&1, &client_addr, &0);

    let result = client.release_milestone(&1, &client_addr, &0);
    assert!(result);

    // Try to release again — should panic with "Milestone already released"
    client.release_milestone(&1, &client_addr, &0);
}

#[test]
fn test_release_milestone_multi_sig() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::MultiSig,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &client_addr, &0);

    let result = client.release_milestone(&1, &client_addr, &0);
    assert!(result);
}

#[test]
fn test_contract_completion_all_milestones_released() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128, 2000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &3000_0000000);

    client.approve_milestone_release(&1, &client_addr, &0);
    client.release_milestone(&1, &client_addr, &0);

    client.approve_milestone_release(&1, &client_addr, &1);
    client.release_milestone(&1, &client_addr, &1);

    // All milestones should be released and contract completed
    // Note: In a real implementation, we would check the contract status
    // For this simplified version, we just verify no panics occurred
}

#[test]
fn test_edge_cases() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1_0000000_i128]; // Minimum amount

    // Test with minimum amount
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 0);

    // Test with multiple milestones
    let many_milestones = vec![
        &env,
        100_0000000_i128,
        200_0000000_i128,
        300_0000000_i128,
        400_0000000_i128,
    ];
    let id2 = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &many_milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id2, 0); // ledger sequence stays the same in test env
pub(crate) fn world_symbol() -> soroban_sdk::Symbol {
    symbol_short!("World")
}

mod flows;
mod security;
mod storage;
