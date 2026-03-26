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
