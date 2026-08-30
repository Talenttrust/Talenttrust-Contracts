#![cfg(test)]

use crate::types::{DataKey, Error, ReleaseAuthorization};
use crate::{Escrow, EscrowClient, MAX_MILESTONES};
use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal, Symbol, Vec,
};

#[test]
fn test_error_already_initialized_on_double_init() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));

    // Second initialization attempt must fail
    let res = client.try_initialize(&admin);
    assert!(res.is_err());
}

#[test]
fn test_error_contract_not_found_on_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let caller = Address::generate(&env);
    // Non-existent contract ID 9999
    let res = client.try_release_milestone(&9999, &caller, &0);
    assert!(res.is_err());
}

#[test]
fn test_error_invalid_participants_same_client_and_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let same_addr = Address::generate(&env);
    let mut milestones = Vec::new(&env);
    milestones.push_back(1_000i128);

    // Client == Freelancer should fail
    let res = client.try_create_contract(
        &same_addr,
        &same_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(res.is_err());
}

#[test]
fn test_error_empty_milestones_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let empty_milestones = Vec::new(&env);

    let res = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &empty_milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(res.is_err());
}

#[test]
fn test_error_invalid_milestone_amount_negative_or_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let mut zero_milestones = Vec::new(&env);
    zero_milestones.push_back(0i128);

    let res_zero = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &zero_milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(res_zero.is_err());

    let mut neg_milestones = Vec::new(&env);
    neg_milestones.push_back(-500i128);

    let res_neg = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &neg_milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(res_neg.is_err());
}

#[test]
fn test_error_too_many_milestones_exceeds_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let mut too_many = Vec::new(&env);
    for _ in 0..=(MAX_MILESTONES + 1) {
        too_many.push_back(100i128);
    }

    let res = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &too_many,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(res.is_err());
}

#[test]
fn test_error_unauthorized_role_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let attacker = Address::generate(&env);

    let mut milestones = Vec::new(&env);
    milestones.push_back(1_000i128);

    let c_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Attacker tries to deposit
    let res = client.try_deposit_funds(&c_id, &attacker, &1_000);
    assert!(res.is_err());
}

#[test]
fn test_error_index_out_of_bounds_on_milestone_release() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let mut milestones = Vec::new(&env);
    milestones.push_back(1_000i128);

    let c_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&c_id, &client_addr, &1_000);

    // Release index 10 (only index 0 exists)
    let res = client.try_release_milestone(&c_id, &client_addr, &10);
    assert!(res.is_err());
}

#[test]
fn test_error_invalid_protocol_parameters_out_of_bounds_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Fee > 10,000 bps (100%)
    let res = client.try_set_protocol_fee_bps(&10_001, &1u64);
    assert!(res.is_err());
}
