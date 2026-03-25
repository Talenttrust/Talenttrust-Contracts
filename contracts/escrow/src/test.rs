#![cfg(test)]

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Symbol, Vec};

use crate::{Escrow, EscrowClient, EscrowError};

pub const MILESTONE_ONE: i128 = 200_0000000;
pub const MILESTONE_TWO: i128 = 400_0000000;
pub const MILESTONE_THREE: i128 = 600_0000000;

pub fn world_symbol() -> Symbol {
    symbol_short!("World")
}

pub fn total_milestone_amount() -> i128 {
    MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
}

pub fn default_milestones(env: &Env) -> Vec<i128> {
    vec![env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]
}

pub fn generated_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

pub fn register_client<'a>(env: &'a Env) -> EscrowClient<'a> {
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
}

pub fn create_contract<'a>(env: &Env, client: &EscrowClient<'a>) -> (Address, Address, u32) {
    let (client_addr, freelancer_addr) = generated_participants(env);
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(env));
    (client_addr, freelancer_addr, contract_id)
}

pub fn complete_contract<'a>(env: &Env, client: &EscrowClient<'a>) -> (Address, Address, u32) {
    let (client_addr, freelancer_addr, contract_id) = create_contract(env, client);
    assert!(client.deposit_funds(&contract_id, &total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &0));
    assert!(client.release_milestone(&contract_id, &1));
    assert!(client.release_milestone(&contract_id, &2));
    (client_addr, freelancer_addr, contract_id)
}

pub fn assert_contract_error<T>(
    result: Result<
        Result<T, soroban_sdk::ConversionError>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
    expected: EscrowError,
) {
    match result {
        Err(Ok(err)) => {
            assert_eq!(
                err,
                soroban_sdk::Error::from_contract_error(expected as u32)
            );
        }
        _ => panic!("expected contract error"),
    }
}

mod emergency_controls;
mod flows;
mod governance;
mod lifecycle;
mod pause_controls;
mod performance;
mod persistence;
mod security;
