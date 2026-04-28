use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Symbol};

use crate::{Escrow, EscrowClient, EscrowError};

// ─── Submodules ───────────────────────────────────────────────────────────────

mod pause_controls;
mod emergency_controls;
mod mainnet_readiness;

// ─── Shared helpers ───────────────────────────────────────────────────────────

pub fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

pub fn world_symbol() -> Symbol {
    symbol_short!("World")
}

pub fn default_milestones(env: &Env) -> soroban_sdk::Vec<i128> {
    vec![env, 100_0000000_i128, 200_0000000_i128, 300_0000000_i128]
}

pub fn total_milestone_amount() -> i128 {
    100_0000000 + 200_0000000 + 300_0000000
}

pub fn generated_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

pub fn create_contract(
    env: &Env,
    client: &EscrowClient,
) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = default_milestones(env);
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    (client_addr, freelancer_addr, contract_id)
}

pub fn complete_contract(
    env: &Env,
    client: &EscrowClient,
) -> (Address, Address, u32) {
    let (client_addr, freelancer_addr, contract_id) = create_contract(env, client);
    client.deposit_funds(&contract_id, &total_milestone_amount());
    client.release_milestone(&contract_id, &0);
    client.release_milestone(&contract_id, &1);
    client.release_milestone(&contract_id, &2);
    (client_addr, freelancer_addr, contract_id)
}

/// Assert that a `try_*` call returned the expected contract error.
pub fn assert_contract_error<T: core::fmt::Debug, IE: core::fmt::Debug>(
    result: Result<T, Result<soroban_sdk::Error, IE>>,
    expected: EscrowError,
) {
    match result {
        Err(Ok(e)) => {
            let expected_err: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_err);
        }
        other => panic!("expected contract error {:?}, got {:?}", expected, other),
    }
}
