#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{Escrow, EscrowClient, Error};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

#[test]
fn default_max_milestones_is_compile_time_default() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    // Default should be the compile-time constant
    assert_eq!(client.get_max_milestones(), crate::MAX_MILESTONES);
}

#[test]
fn admin_can_set_in_bounds() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    assert!(client.set_max_milestones(&admin, &20u32));
    assert_eq!(client.get_max_milestones(), 20u32);
}

#[test]
fn reject_over_bounds_value() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let too_large = crate::MAX_MAX_MILESTONES.checked_add(1).unwrap_or(u32::MAX);
    let result = client.try_set_max_milestones(&admin, &too_large);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);
}

#[test]
fn non_admin_cannot_set() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fake_admin = Address::generate(&env);

    client.initialize(&admin);

    let result = client.try_set_max_milestones(&fake_admin, &10u32);
    super::assert_contract_error(result, crate::EscrowError::UnauthorizedRole);
}
