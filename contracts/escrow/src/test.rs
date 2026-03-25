#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    vec, Address, Env,
};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

#[test]
fn test_hello() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // No hello in new lib.rs, but we can add it or just remove this test.
    // I removed it from lib.rs. I'll remove it here too.
}

#[test]
fn test_initialize() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    
    client.initialize(&admin);
}

#[test]
fn test_create_contract_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200, 400, 600];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);
}

mod milestone_amendments;
// mod emergency_controls;
// mod pause_controls;
