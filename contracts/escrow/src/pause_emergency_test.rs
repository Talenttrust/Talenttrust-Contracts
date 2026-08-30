#![cfg(test)]

use crate::types::{ContractStatus, Error, ReleaseAuthorization};
use crate::{Escrow, EscrowClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal, Symbol, Vec,
};

fn setup_escrow_pause_test<'a>(env: &'a Env) -> (EscrowClient<'a>, Address, Address, Address, u32) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);

    let admin = Address::generate(env);
    client.initialize(&admin);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);

    let mut milestones = Vec::new(env);
    milestones.push_back(1_000i128);
    milestones.push_back(2_000i128);

    let c_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    client.deposit_funds(&c_id, &client_addr, &3_000);

    (client, admin, client_addr, freelancer_addr, c_id)
}

#[test]
fn test_paused_rejects_mutating_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, freelancer_addr, c_id) = setup_escrow_pause_test(&env);

    // Pause the contract
    assert!(client.pause(&1u64));
    assert!(client.is_paused());

    // 1. Create contract must fail while paused
    let mut milestones = Vec::new(&env);
    milestones.push_back(500i128);
    let res_create = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(
        res_create.is_err(),
        "create_contract must fail while paused"
    );

    // 2. Deposit funds must fail while paused
    let res_deposit = client.try_deposit_funds(&c_id, &client_addr, &500);
    assert!(res_deposit.is_err(), "deposit_funds must fail while paused");

    // 3. Release milestone must fail while paused
    let res_release = client.try_release_milestone(&c_id, &client_addr, &0);
    assert!(
        res_release.is_err(),
        "release_milestone must fail while paused"
    );

    // 4. Batch release milestone must fail while paused
    let mut batch = Vec::new(&env);
    batch.push_back(0);
    let res_batch = client.try_release_milestone_batch(&c_id, &client_addr, &batch);
    assert!(
        res_batch.is_err(),
        "release_milestone_batch must fail while paused"
    );
}

#[test]
fn test_paused_reads_still_allowed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _client_addr, _freelancer_addr, c_id) = setup_escrow_pause_test(&env);

    // Pause the contract
    client.pause(&1u64);
    assert!(client.is_paused());

    // Readers must succeed and return correct data while paused
    let contract = client.get_contract(&c_id);
    assert_eq!(contract.funded_amount, 3_000);

    let milestones = client.get_milestones(&c_id);
    assert_eq!(milestones.len(), 2);

    let progress = client.get_milestone_progress(&c_id);
    assert_eq!(progress.total, 2);
    assert_eq!(progress.completed, 0);

    let admin = client.get_admin();
    assert!(admin.is_some());
}

#[test]
fn test_unpause_restores_mutation_capabilities() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _freelancer_addr, c_id) = setup_escrow_pause_test(&env);

    // Pause then unpause
    client.pause(&1u64);
    assert!(client.is_paused());

    client.unpause();
    assert!(!client.is_paused());

    // Mutations must now succeed normally
    assert!(client.release_milestone(&c_id, &client_addr, &0));

    let milestones = client.get_milestones(&c_id);
    assert!(milestones.get(0).unwrap().released);
}

#[test]
fn test_pause_and_unpause_emit_distinct_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _client_addr, _freelancer_addr, _c_id) = setup_escrow_pause_test(&env);

    let initial_events = env.events().all().len();

    // Pause emits event
    client.pause(&1u64);
    let events_after_pause = env.events().all().len();
    assert!(events_after_pause > initial_events);

    // Unpause emits event
    client.unpause();
    let events_after_unpause = env.events().all().len();
    assert!(events_after_unpause > events_after_pause);
}
