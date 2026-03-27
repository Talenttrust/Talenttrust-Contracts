#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, EscrowError};

#[test]
fn test_reputation_valid() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128];

    let escrow_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    client.release_milestone(&escrow_id, &0); // sets status to Completed

    let res = client.issue_reputation(&escrow_id, &freelancer_addr, &5);
    assert_eq!(res, true);
}

#[test]
fn test_reputation_invalid_rating() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128];

    let escrow_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    client.release_milestone(&escrow_id, &0); // triggers Completion

    // Try issuing with a 0 rating
    let res_low = client.try_issue_reputation(&escrow_id, &freelancer_addr, &0);
    assert_eq!(res_low, Err(Ok(EscrowError::InvalidRating)));

    // Try issuing with a > 5 rating
    let res_high = client.try_issue_reputation(&escrow_id, &freelancer_addr, &6);
    assert_eq!(res_high, Err(Ok(EscrowError::InvalidRating)));
}

#[test]
fn test_reputation_timing_fail() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128];

    let escrow_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    // Not releasing any milestone here, so contract status is Created or Funded

    let res = client.try_issue_reputation(&escrow_id, &freelancer_addr, &5);
    assert_eq!(res, Err(Ok(EscrowError::NotCompleted)));
}

#[test]
fn test_reputation_duplicate() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128];

    let escrow_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    client.release_milestone(&escrow_id, &0);

    // Initial valid issue
    let res = client.issue_reputation(&escrow_id, &freelancer_addr, &5);
    assert!(res);

    // Secondly issuing -> duplicate error expected
    let res2 = client.try_issue_reputation(&escrow_id, &freelancer_addr, &4);
    assert_eq!(res2, Err(Ok(EscrowError::DuplicateRating)));
}
