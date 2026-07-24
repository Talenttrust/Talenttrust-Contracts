//! Boundary and rejection tests for escrow contracts (issue #781).
//! Tests edge cases: exact boundaries, one-over, unauthorized callers.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

fn make_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

// ─── Deposit boundaries ────────────────────────────────────────────────────

#[test]
fn test_deposit_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = make_client(&env);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let ms = vec![&env, 200_0000000_i128];
    let id = client.create_contract(&ca, &fa, &None, &ms, &ReleaseAuthorization::ClientOnly);
    client.deposit_funds(&id, &ca, &100_0000000_i128);

    let result = client.try_deposit_funds(&id, &ca, &0);
    assert!(result.is_err());
}

#[test]
fn test_deposit_on_nonexistent_contract_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = make_client(&env);
    let caller = Address::generate(&env);
    let result = client.try_deposit_funds(&999_u32, &caller, &1000);
    assert!(result.is_err());
}

// ─── Milestone boundaries ──────────────────────────────────────────────────

#[test]
fn test_milestones_one_over_max_rejected() {
    let env = Env::default();
    let client = make_client(&env);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let mut ms = soroban_sdk::Vec::new(&env);
    for _ in 0..crate::MAX_MILESTONES + 1 {
        ms.push_back(100_0000000_i128);
    }
    let result =
        client.try_create_contract(&ca, &fa, &None, &ms, &ReleaseAuthorization::ClientOnly);
    assert!(result.is_err());
}

#[test]
fn test_milestone_zero_amount_rejected() {
    let env = Env::default();
    let client = make_client(&env);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let ms = vec![&env, 0_i128];
    let result =
        client.try_create_contract(&ca, &fa, &None, &ms, &ReleaseAuthorization::ClientOnly);
    assert!(result.is_err());
}

// ─── Release boundaries ────────────────────────────────────────────────────

#[test]
fn test_release_nonexistent_contract_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = make_client(&env);
    let result = client.try_release_milestone(&999_u32, &Address::generate(&env), &0_u32);
    assert!(result.is_err());
}

#[test]
fn test_release_by_stranger_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = make_client(&env);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let ms = vec![&env, 200_0000000_i128];
    let id = client.create_contract(&ca, &fa, &None, &ms, &ReleaseAuthorization::ClientOnly);
    client.deposit_funds(&id, &ca, &200_0000000_i128);

    let stranger = Address::generate(&env);
    let result = client.try_release_milestone(&id, &stranger, &0_u32);
    assert!(result.is_err());
}

#[test]
fn test_double_release_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = make_client(&env);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let ms = vec![&env, 200_0000000_i128];
    let id = client.create_contract(&ca, &fa, &None, &ms, &ReleaseAuthorization::ClientOnly);
    client.deposit_funds(&id, &ca, &200_0000000_i128);

    client.approve_milestone_release(&id, &ca, &0_u32);
    client.release_milestone(&id, &ca, &0_u32);

    let result = client.try_release_milestone(&id, &ca, &0_u32);
    assert!(result.is_err());
}

// ─── Initialize boundaries ─────────────────────────────────────────────────

#[test]
fn test_double_initialize_rejected() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let result = client.try_initialize(&admin);
    assert!(result.is_err());
}
