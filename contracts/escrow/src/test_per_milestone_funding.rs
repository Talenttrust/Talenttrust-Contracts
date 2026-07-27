#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient};

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    (env, client_addr, freelancer_addr)
}

fn create_client(env: &Env) -> EscrowClient {
    let contract_id = env.register(Escrow, ());
    EscrowClient::new(env, &contract_id)
}

fn create_contract(
    env: &Env,
    client: &EscrowClient,
    client_addr: &Address,
    freelancer_addr: &Address,
) -> u32 {
    let milestones = vec![env, 100_i128, 200_i128, 300_i128];
    client.create_contract(client_addr, freelancer_addr, &None, &milestones, &None, &None)
}

#[test]
fn test_partial_funding_single_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit partial funds
    assert!(client.deposit_funds(&contract_id, &150_i128));

    // Fund only first milestone partially
    assert!(client.set_milestone_funded(&contract_id, &0, &100_i128));
    assert!(client.set_milestone_funded(&contract_id, &1, &50_i128));

    // Verify funding amounts
    assert_eq!(client.get_milestone_funded(&contract_id, &0), 100_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &1), 50_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &2), 0_i128);
}

#[test]
fn test_mixed_funding_multiple_milestones() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit full funds
    assert!(client.deposit_funds(&contract_id, &600_i128));

    // Fund milestones with different amounts
    assert!(client.set_milestone_funded(&contract_id, &0, &100_i128));
    assert!(client.set_milestone_funded(&contract_id, &1, &200_i128));
    assert!(client.set_milestone_funded(&contract_id, &2, &300_i128));

    // Verify all funding amounts
    assert_eq!(client.get_milestone_funded(&contract_id, &0), 100_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &1), 200_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &2), 300_i128);
}

#[test]
fn test_partial_release_with_per_milestone_funding() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit and fund
    assert!(client.deposit_funds(&contract_id, &600_i128));
    assert!(client.set_milestone_funded(&contract_id, &0, &100_i128));
    assert!(client.set_milestone_funded(&contract_id, &1, &200_i128));
    assert!(client.set_milestone_funded(&contract_id, &2, &300_i128));

    // Release first milestone
    assert!(client.release_milestone(&contract_id, &0));

    // Release second milestone
    assert!(client.release_milestone(&contract_id, &1));

    // Verify funding still tracked
    assert_eq!(client.get_milestone_funded(&contract_id, &0), 100_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &1), 200_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &2), 300_i128);
}

#[test]
#[should_panic]
fn test_release_without_sufficient_milestone_funding() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit funds
    assert!(client.deposit_funds(&contract_id, &600_i128));

    // Fund milestone with insufficient amount
    assert!(client.set_milestone_funded(&contract_id, &0, &50_i128));

    // Try to release - should panic due to insufficient funding
    client.release_milestone(&contract_id, &0);
}

#[test]
fn test_incremental_funding_per_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit funds in stages
    assert!(client.deposit_funds(&contract_id, &300_i128));

    // Fund first milestone
    assert!(client.set_milestone_funded(&contract_id, &0, &100_i128));
    assert_eq!(client.get_milestone_funded(&contract_id, &0), 100_i128);

    // Deposit more funds
    assert!(client.deposit_funds(&contract_id, &300_i128));

    // Fund remaining milestones
    assert!(client.set_milestone_funded(&contract_id, &1, &200_i128));
    assert!(client.set_milestone_funded(&contract_id, &2, &300_i128));

    // Verify all funding
    assert_eq!(client.get_milestone_funded(&contract_id, &0), 100_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &1), 200_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &2), 300_i128);
}

#[test]
fn test_update_milestone_funding() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit funds
    assert!(client.deposit_funds(&contract_id, &600_i128));

    // Initial funding
    assert!(client.set_milestone_funded(&contract_id, &0, &50_i128));
    assert_eq!(client.get_milestone_funded(&contract_id, &0), 50_i128);

    // Update funding
    assert!(client.set_milestone_funded(&contract_id, &0, &100_i128));
    assert_eq!(client.get_milestone_funded(&contract_id, &0), 100_i128);
}

#[test]
fn test_zero_funding_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit funds
    assert!(client.deposit_funds(&contract_id, &600_i128));

    // Set zero funding for a milestone
    assert!(client.set_milestone_funded(&contract_id, &0, &0_i128));
    assert_eq!(client.get_milestone_funded(&contract_id, &0), 0_i128);
}

#[test]
#[should_panic]
fn test_release_unfunded_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit funds
    assert!(client.deposit_funds(&contract_id, &600_i128));

    // Don't fund any milestone, try to release
    client.release_milestone(&contract_id, &0);
}

#[test]
fn test_multiple_contracts_independent_funding() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);

    // Create first contract
    let contract1 = create_contract(&env, &client, &client_addr, &freelancer_addr);
    assert!(client.deposit_funds(&contract1, &600_i128));
    assert!(client.set_milestone_funded(&contract1, &0, &100_i128));

    // Create second contract
    let contract2 = create_contract(&env, &client, &client_addr, &freelancer_addr);
    assert!(client.deposit_funds(&contract2, &600_i128));
    assert!(client.set_milestone_funded(&contract2, &0, &150_i128));

    // Verify independent funding
    assert_eq!(client.get_milestone_funded(&contract1, &0), 100_i128);
    assert_eq!(client.get_milestone_funded(&contract2, &0), 150_i128);
}

#[test]
fn test_full_lifecycle_with_per_milestone_funding() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit full funds
    assert!(client.deposit_funds(&contract_id, &600_i128));

    // Fund all milestones
    assert!(client.set_milestone_funded(&contract_id, &0, &100_i128));
    assert!(client.set_milestone_funded(&contract_id, &1, &200_i128));
    assert!(client.set_milestone_funded(&contract_id, &2, &300_i128));

    // Release all milestones
    assert!(client.release_milestone(&contract_id, &0));
    assert!(client.release_milestone(&contract_id, &1));
    assert!(client.release_milestone(&contract_id, &2));

    // Verify funding persists after release
    assert_eq!(client.get_milestone_funded(&contract_id, &0), 100_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &1), 200_i128);
    assert_eq!(client.get_milestone_funded(&contract_id, &2), 300_i128);
}
