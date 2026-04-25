#![cfg(test)]

mod access_control;
mod cancel_contract;
mod client_migration;
mod emergency_controls;
mod flows;
mod governance;
mod hello;
mod input_sanitization_amounts;
mod input_sanitization_identities;
mod lifecycle;
mod mainnet_readiness;
mod milestone_schedule;
mod pause_controls;
mod performance;
mod persistence;
mod security;
mod storage;
mod timeout_tests;
mod ttl_tests;

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Symbol, Vec};

use crate::{Escrow, EscrowClient, EscrowError};

// ─── Milestone amounts used across tests ─────────────────────────────────────

pub const MILESTONE_ONE: i128 = 200_0000000;
pub const MILESTONE_TWO: i128 = 400_0000000;
pub const MILESTONE_THREE: i128 = 600_0000000;

pub fn total_milestone_amount() -> i128 {
    MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
}

/// Alias used by access_control tests.
pub fn total_milestones() -> i128 {
    total_milestone_amount()
}

// ─── Environment / registration helpers ──────────────────────────────────────

pub fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

/// Alias used by hello / client_migration tests.
pub fn register_escrow(env: &Env) -> EscrowClient {
    register_client(env)
}

pub fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

// ─── Participant helpers ──────────────────────────────────────────────────────

/// Returns (client, freelancer).
pub fn generated_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

// ─── Milestone helpers ────────────────────────────────────────────────────────

pub fn default_milestones(env: &Env) -> Vec<i128> {
    vec![env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
}

// ─── Contract creation helpers ───────────────────────────────────────────────

/// Creates a contract with default milestones; returns (client_addr, freelancer_addr, contract_id).
pub fn create_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = default_milestones(env);
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    (client_addr, freelancer_addr, contract_id)
}

/// Creates and fully funds + releases all milestones; returns (client_addr, freelancer_addr, contract_id).
pub fn complete_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let (client_addr, freelancer_addr, contract_id) = create_contract(env, client);
    client.deposit_funds(&contract_id, &total_milestone_amount());
    client.release_milestone(&contract_id, &0);
    client.release_milestone(&contract_id, &1);
    client.release_milestone(&contract_id, &2);
    (client_addr, freelancer_addr, contract_id)
}

/// Struct used by client_migration tests.
pub struct SampleContractParties {
    pub client: Address,
    pub freelancer: Address,
    pub replacement_client: Address,
}

pub fn create_sample_contract(env: &Env, client: &EscrowClient) -> (SampleContractParties, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let replacement_client = Address::generate(env);
    let milestones = default_milestones(env);
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    (
        SampleContractParties {
            client: client_addr,
            freelancer: freelancer_addr,
            replacement_client,
        },
        contract_id,
    )
}

pub fn full_funding_amount() -> i128 {
    total_milestone_amount()
}

// ─── Symbol helpers ───────────────────────────────────────────────────────────

pub fn world_symbol() -> Symbol {
    symbol_short!("World")
}

// ─── Error assertion helpers ──────────────────────────────────────────────────

pub fn assert_contract_error<T>(
    result: Result<T, Result<EscrowError, soroban_sdk::InvokeError>>,
    expected: EscrowError,
) {
    assert_eq!(result, Err(Ok(expected)));
}

/// Asserts that a closure panics (used by client_migration tests).
pub fn assert_panics<F: FnOnce()>(f: F) {
    extern crate std;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    assert!(result.is_err(), "expected a panic but none occurred");
}
