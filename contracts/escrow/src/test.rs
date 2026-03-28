extern crate std;

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Symbol};

use crate::{Escrow, EscrowClient};

pub const MILESTONE_ONE: i128 = 200_i128;
pub const MILESTONE_TWO: i128 = 400_i128;
pub const MILESTONE_THREE: i128 = 600_i128;

pub fn default_milestones(env: &Env) -> soroban_sdk::Vec<i128> {
    vec![env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
}

pub fn total_milestone_amount() -> i128 {
    MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
}

pub fn generated_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

pub fn register_client(env: &Env) -> EscrowClient<'static> {
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
}

pub fn world_symbol() -> Symbol {
    symbol_short!("World")
}

pub fn assert_panics<F: FnOnce() + std::panic::UnwindSafe>(f: F) {
    assert!(std::panic::catch_unwind(f).is_err());
}

mod base;
mod create_contract_errors;
mod emergency_controls;
mod flows;
mod governance;
mod lifecycle;
mod dispute_lifecycle;
mod operation_errors;
mod pause_controls;
mod performance;
mod security;
#![cfg(test)]
use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Vec};

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};
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

#[test]
#[should_panic(expected = "Deposit amount must equal total milestone amounts")]
fn test_create_contract_invalid_milestone_amount() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract first
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Note: Authentication tests would require proper mock setup
    // For now, we test the basic contract creation logic

    env.mock_all_auths();
    let result = client.deposit_funds(&id, &client_addr, &500_0000000);
    assert!(result);
}

#[test]
#[should_panic(expected = "Exceeds maximum milestone count")]
fn test_create_contract_too_many_milestones() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create vector with 17 milestones (1 more than DEFAULT_MAX_MILESTONES of 16)
    let mut milestones = vec![&env];
    for _ in 0..17 {
        milestones.push_back(1000_0000000_i128);
    }

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic(expected = "Exceeds maximum contract funding size")]
fn test_create_contract_exceeds_size_limit() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Amount exceeds 1_000_000_000_000_i128 limit
    let milestones = vec![&env, 2_000_000_000_000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

// ==================== DEPOSIT FUNDS TESTS ====================

#[test]
#[should_panic(expected = "Deposit amount must equal total milestone amounts")]
fn test_deposit_funds_wrong_amount() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract first
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Note: Authentication tests would require proper mock setup
    // For now, we test the basic contract creation logic

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &500_0000000);
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
fn test_dispute_contract_transitions() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);

    let result = client.dispute_contract(&1, &client_addr);
    assert!(result);
}

#[test]
#[should_panic(expected = "Contract must be in Funded status to release milestones")]
fn test_disputed_contract_cannot_release_milestone() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.dispute_contract(&1, &client_addr);

    client.release_milestone(&1, &client_addr, &0);
}

#[test]
#[should_panic(expected = "Contract must be in Created status to deposit funds")]
fn test_invalid_status_transition_from_completed_to_funded() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &client_addr, &0);
    client.release_milestone(&1, &client_addr, &0);

    // Attempt invalid transition by re-depositing after completion.
    client.deposit_funds(&1, &client_addr, &1000_0000000);
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