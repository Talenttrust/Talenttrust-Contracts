#![cfg(test)]

use crate::{DataKey, Escrow, EscrowClient, EscrowError};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn withdraw_protocol_fees_resets_accumulator_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    let destination = Address::generate(&env);
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &684_i128);
    });

    let success = client.withdraw_protocol_fees(&admin, &destination);
    assert!(success);

    env.as_contract(&contract_id, || {
        let remaining: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0_i128);
        assert_eq!(remaining, 0_i128);
    });

    let events = env.events().all();
    assert!(events
        .iter()
        .any(|event| event.0 == soroban_sdk::symbol_short!("fee_wd")));
}

#[test]
fn unauthorized_withdrawal_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    let fake_admin = Address::generate(&env);
    let destination = Address::generate(&env);

    let result = client.try_withdraw_protocol_fees(&fake_admin, &destination);
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
<<<<<<< HEAD
#[should_panic(expected = "HostError: Error(Contract, #24)")] // InsufficientAccumulatedFees
fn test_over_withdrawal() {
=======
fn withdraw_with_zero_accumulator_fails() {
>>>>>>> 30df75a (I've completed this successfully.)
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    let destination = Address::generate(&env);
    let result = client.try_withdraw_protocol_fees(&admin, &destination);
    super::assert_contract_error(result, EscrowError::InsufficientAccumulatedFees);
}

#[test]
fn withdraw_rejects_when_paused() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Paused, &true);
    });

    let destination = Address::generate(&env);
    let result = client.try_withdraw_protocol_fees(&admin, &destination);
    super::assert_contract_error(result, EscrowError::ContractPaused);
}

#[test]
fn withdraw_rejects_when_emergency_active() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Emergency, &true);
    });

    let destination = Address::generate(&env);
    let result = client.try_withdraw_protocol_fees(&admin, &destination);
    super::assert_contract_error(result, EscrowError::EmergencyActive);
}
