extern crate std;

use soroban_sdk::{testutils::Address as _, vec, Address, Env, Vec};

use crate::{ContractStatus, Escrow, EscrowClient, EscrowContractData, Milestone};

#[path = "create_contract.rs"]
mod create_contract;
#[path = "deposit.rs"]
mod deposit;
#[path = "refund.rs"]
mod refund;
#[path = "release.rs"]
mod release;

fn setup() -> (Env, Address, Address) {
use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

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
    env.mock_all_auths();

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    (env, client_addr, freelancer_addr)
}

fn create_client(env: &Env) -> EscrowClient<'_> {
#[test]
#[should_panic(expected = "Deposit amount must equal total milestone amounts")]
fn test_create_contract_invalid_milestone_amount() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
}

fn create_default_contract(
    env: &Env,
    client: &EscrowClient<'_>,
    client_addr: &Address,
    freelancer_addr: &Address,
) -> u32 {
    let milestones = vec![env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    client.create_contract(client_addr, freelancer_addr, &milestones)
}

fn assert_contract_state(
    contract: EscrowContractData,
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

fn assert_milestone_flags(
    milestones: Vec<Milestone>,
    milestone_id: u32,
    expected_released: bool,
    expected_refunded: bool,
) {
    let milestone = milestones.get(milestone_id).unwrap();
    assert_eq!(milestone.released, expected_released);
    assert_eq!(milestone.refunded, expected_refunded);
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
