#![cfg(test)]

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Vec};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

// ─── Submodules ───────────────────────────────────────────────────────────────

mod create_contract_bounds;
mod admin_auth_helper;
mod dispute;
mod emergency_controls;
mod lifecycle;
mod pause_controls;
mod protocol_fees;
mod ttl_tests;

// ─── Shared constants ─────────────────────────────────────────────────────────

#[allow(dead_code)] // shared test fixture; not all test modules use every constant
pub const MILESTONE_ONE: i128 = 200_0000000;
#[allow(dead_code)]
pub const MILESTONE_TWO: i128 = 400_0000000;
#[allow(dead_code)]
pub const MILESTONE_THREE: i128 = 600_0000000;

// ─── Shared helpers ───────────────────────────────────────────────────────────

pub fn register_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

pub fn generated_participants(env: &Env) -> (Address, Address, Address) {
    (
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
    )
}

pub fn default_milestones(env: &Env) -> Vec<i128> {
    vec![env, 1000_0000000_i128, 2000_0000000_i128, 3000_0000000_i128]
}

pub fn create_default_contract(
    env: &Env,
    client: &EscrowClient,
    client_addr: &Address,
    freelancer_addr: &Address,
) -> u32 {
    client.create_contract(
        client_addr,
        freelancer_addr,
        &None,
        &default_milestones(env),
        &ReleaseAuthorization::ClientOnly,
    )
}

// ─── Smoke tests (current contract API) ───────────────────────────────────────

#[test]
fn test_hello() {
    let env = Env::default();
    let client = register_client(&env);
    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

#[test]
fn test_create_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _) = generated_participants(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);
}

#[test]
fn test_deposit_funds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _) = generated_participants(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &1_000_0000000));
}

#[test]
fn test_release_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _) = generated_participants(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &6_000_0000000_i128));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
}
