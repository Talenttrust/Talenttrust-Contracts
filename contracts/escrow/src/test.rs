#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    vec, Address, Env, IntoVal,
};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

// ---------------------------------------------------------------------------
// Shared test helpers (imported by sub-modules via `super::`)
// ---------------------------------------------------------------------------

pub(crate) fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

pub(crate) fn generated_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

pub(crate) fn world_symbol() -> soroban_sdk::Symbol {
    symbol_short!("World")
}

pub(crate) const MILESTONE_ONE: i128 = 200_0000000_i128;
pub(crate) const MILESTONE_TWO: i128 = 400_0000000_i128;
pub(crate) const MILESTONE_THREE: i128 = 600_0000000_i128;

pub(crate) fn default_milestones(env: &Env) -> soroban_sdk::Vec<i128> {
    vec![env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
}

pub(crate) fn total_milestone_amount() -> i128 {
    MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
}

// ---------------------------------------------------------------------------
// Inline smoke tests
// ---------------------------------------------------------------------------

#[test]
fn test_hello() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

// ==================== CONTRACT CREATION TESTS ====================

#[test]
fn test_create_contract_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );
    assert_eq!(id, 1);
}

#[test]
fn test_create_contract_with_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
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
        &soroban_sdk::Vec::new(&env),
    );
    assert_eq!(id, 1);
}

#[test]
#[should_panic(expected = "At least one milestone required")]
fn test_create_contract_no_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &vec![&env],
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );
}

#[test]
#[should_panic(expected = "Client and freelancer cannot be the same address")]
fn test_create_contract_same_addresses() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &client_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );
}

#[test]
#[should_panic(expected = "Milestone amounts must be positive")]
fn test_create_contract_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &vec![&env, -1000_0000000_i128],
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );
}

// ==================== DEPOSIT FUNDS TESTS ====================

#[test]
fn test_approve_milestone_release_client_only() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );

    client.deposit_funds(&id, &client_addr, &1000_0000000);
    let result = client.approve_milestone_release(&id, &client_addr, &0);
    assert!(result);
}

#[test]
#[should_panic(expected = "Caller not authorized to approve milestone release")]
fn test_approve_milestone_release_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let unauthorized_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );

    client.deposit_funds(&id, &client_addr, &1000_0000000);
    client.approve_milestone_release(&id, &unauthorized_addr, &0);
}

#[test]
#[should_panic(expected = "Milestone already approved by this address")]
fn test_approve_milestone_release_already_approved() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );

    client.deposit_funds(&id, &client_addr, &1000_0000000);
    client.approve_milestone_release(&id, &client_addr, &0);
    // Second approval by the same address should panic.
    client.approve_milestone_release(&id, &client_addr, &0);
}

#[test]
fn test_release_milestone_client_only() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );

    client.deposit_funds(&id, &client_addr, &1000_0000000);
    client.approve_milestone_release(&id, &client_addr, &0);
    let result = client.release_milestone(&id, &client_addr, &0);
    assert!(result);
}

#[test]
fn test_release_milestone_arbiter_only() {
    let env = Env::default();
    env.mock_all_auths();
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
        &ReleaseAuthorization::ArbiterOnly,
        &soroban_sdk::Vec::new(&env),
    );

    client.deposit_funds(&id, &client_addr, &1000_0000000);
    client.approve_milestone_release(&id, &arbiter_addr, &0);
    let result = client.release_milestone(&id, &arbiter_addr, &0);
    assert!(result);
}

#[test]
#[should_panic(expected = "Insufficient approvals for milestone release")]
fn test_release_milestone_no_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );

    client.deposit_funds(&id, &client_addr, &1000_0000000);
    client.release_milestone(&id, &client_addr, &0);
}

#[test]
#[should_panic(expected = "Milestone already released")]
fn test_release_milestone_already_released() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128, 2000_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );

    client.deposit_funds(&id, &client_addr, &3000_0000000);
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    // Try to release again.
    client.release_milestone(&id, &client_addr, &0);
}

#[test]
fn test_contract_completion_all_milestones_released() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128, 2000_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &soroban_sdk::Vec::new(&env),
    );

    client.deposit_funds(&id, &client_addr, &3000_0000000);

    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);

    client.approve_milestone_release(&id, &client_addr, &1);
    client.release_milestone(&id, &client_addr, &1);

    let record = client.get_contract(&id);
    assert_eq!(record.status, crate::ContractStatus::Completed);
}

// ---------------------------------------------------------------------------
// Sub-module declarations
// ---------------------------------------------------------------------------

mod emergency_controls;
mod pause_controls;

/// Dedicated tests for milestone schedule metadata (contracts-13).
mod milestone_schedule;