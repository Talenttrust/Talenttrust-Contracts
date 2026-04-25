#![cfg(test)]

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Symbol, Vec};

use crate::{ContractStatus, Escrow, EscrowClient};

// ─── Sub-modules ──────────────────────────────────────────────────────────────

pub mod access_control;
pub mod cancel_contract;
pub mod client_migration;
pub mod contract_id_allocation;
pub mod emergency_controls;
pub mod flows;
pub mod governance;
pub mod hello;
pub mod input_sanitization_amounts;
pub mod input_sanitization_identities;
pub mod lifecycle;
pub mod mainnet_readiness;
pub mod milestone_schedule;
pub mod pause_controls;
pub mod performance;
pub mod persistence;
pub mod security;
pub mod storage;
pub mod timeout_tests;
pub mod ttl_tests;

// ─── Shared constants ─────────────────────────────────────────────────────────

pub const MILESTONE_ONE: i128 = 200_0000000;
pub const MILESTONE_TWO: i128 = 400_0000000;
pub const MILESTONE_THREE: i128 = 600_0000000;

// ─── Shared helpers ───────────────────────────────────────────────────────────

pub fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

pub fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

/// Alias used by hello.rs and client_migration.rs.
pub fn register_escrow(env: &Env) -> EscrowClient {
    register_client(env)
}

pub fn generated_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

pub fn default_milestones(env: &Env) -> Vec<i128> {
    vec![env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
}

pub fn total_milestone_amount() -> i128 {
    MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
}

/// Alias used by access_control.rs.
pub fn total_milestones() -> i128 {
    total_milestone_amount()
}

pub fn world_symbol() -> Symbol {
    symbol_short!("World")
}

/// Creates a contract and returns (client_addr, freelancer_addr, contract_id).
pub fn create_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let (client_addr, freelancer_addr) = generated_participants(env);
    let milestones = default_milestones(env);
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    (client_addr, freelancer_addr, contract_id)
}

/// Alias used by client_migration.rs.
pub fn create_sample_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    create_contract(env, client)
}

pub fn full_funding_amount() -> i128 {
    total_milestone_amount()
}

/// Creates a contract, fully funds it, and releases all milestones.
pub fn complete_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let (client_addr, freelancer_addr, contract_id) = create_contract(env, client);
    client.deposit_funds(&contract_id, &total_milestone_amount());
    client.release_milestone(&contract_id, &0);
    client.release_milestone(&contract_id, &1);
    client.release_milestone(&contract_id, &2);
    (client_addr, freelancer_addr, contract_id)
}

/// Asserts that a `try_*` result carries the expected `EscrowError`.
pub fn assert_contract_error<T>(
    result: Result<T, Result<crate::EscrowError, soroban_sdk::InvokeError>>,
    expected: crate::EscrowError,
) {
    assert_eq!(result, Err(Ok(expected)));
}

/// Asserts that a closure panics (used by client_migration.rs).
pub fn assert_panics<F: FnOnce() + std::panic::UnwindSafe>(f: F) {
    assert!(std::panic::catch_unwind(f).is_err());
}
