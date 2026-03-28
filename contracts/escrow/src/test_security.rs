#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};
use crate::{Escrow, EscrowClient, ReleaseAuthorization};

#[test]
#[should_panic(expected = "Only client can deposit funds")]
fn test_unauthorized_deposit() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let malicious_actor = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    // Malicious actor tries to deposit
    client.deposit_funds(&1, &malicious_actor, &1000_0000000);
}

#[test]
#[should_panic(expected = "Contract must be in Created status to deposit funds")]
fn test_deposit_wrong_status_funded() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    // Double funding attempt
    client.deposit_funds(&1, &client_addr, &1000_0000000);
}

#[test]
#[should_panic(expected = "Contract must be in Funded status to approve milestones")]
fn test_approve_wrong_status_created() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    // Try to approve before funding
    client.approve_milestone_release(&1, &client_addr, &0);
}

#[test]
#[should_panic(expected = "Contract must be in Funded status to release milestones")]
fn test_release_wrong_status_created() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    // Try to release before funding
    client.release_milestone(&1, &client_addr, &0);
}

#[test]
#[should_panic(expected = "Caller not authorized to approve milestone release")]
fn test_approve_unauthorized_arbiter_in_clientonly() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    // Arbiter tries to approve in a ClientOnly auth scheme
    client.approve_milestone_release(&1, &arbiter_addr, &0);
}
