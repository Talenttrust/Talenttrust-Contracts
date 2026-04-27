//! Tests for raise_dispute and resolve_dispute.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env};

use crate::{ContractStatus, DisputeResolution, Escrow, EscrowClient};

// ── helpers ──────────────────────────────────────────────────────────────────

fn register(env: &Env) -> EscrowClient {
    EscrowClient::new(env, &env.register(Escrow, ()))
}

fn participants(env: &Env) -> (Address, Address, Address) {
    (
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
    )
}

fn reason_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xabu8; 32])
}

/// Create a funded contract with an arbiter.
fn funded_with_arbiter(env: &Env, escrow: &EscrowClient) -> (Address, Address, Address, u32) {
    let (client_addr, freelancer_addr, arbiter_addr) = participants(env);
    let milestones = vec![env, 100_i128, 200_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
    );
    escrow.deposit_funds(&id, &300_i128);
    (client_addr, freelancer_addr, arbiter_addr, id)
}

// ── raise_dispute happy paths ─────────────────────────────────────────────────

#[test]
fn client_can_raise_dispute_on_funded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);

    assert!(escrow.raise_dispute(&id, &client_addr, &reason_hash(&env)));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Disputed);
}

#[test]
fn freelancer_can_raise_dispute_on_funded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, freelancer_addr, _, id) = funded_with_arbiter(&env, &escrow);

    assert!(escrow.raise_dispute(&id, &freelancer_addr, &reason_hash(&env)));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Disputed);
}

#[test]
fn raise_dispute_stores_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);
    let hash = reason_hash(&env);

    escrow.raise_dispute(&id, &client_addr, &hash);

    let meta = escrow.get_dispute(&id);
    assert_eq!(meta.reason_hash, hash);
    assert_eq!(meta.raised_by, client_addr);
}

// ── raise_dispute error paths ─────────────────────────────────────────────────

#[test]
#[should_panic]
fn arbiter_cannot_raise_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);

    escrow.raise_dispute(&id, &arbiter_addr, &reason_hash(&env));
}

#[test]
#[should_panic]
fn third_party_cannot_raise_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, _, _, id) = funded_with_arbiter(&env, &escrow);
    let outsider = Address::generate(&env);

    escrow.raise_dispute(&id, &outsider, &reason_hash(&env));
}

#[test]
#[should_panic]
fn cannot_raise_dispute_without_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128];
    let id = escrow.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    escrow.deposit_funds(&id, &100_i128);

    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
}

#[test]
#[should_panic]
fn cannot_raise_dispute_on_created_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = participants(&env);
    let milestones = vec![&env, 100_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
    );
    // Not funded — should fail

    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
}

#[test]
#[should_panic]
fn cannot_raise_dispute_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);

    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
    // Already Disputed, not Funded — second call must fail
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
}

// ── resolve_dispute happy paths ───────────────────────────────────────────────

#[test]
fn arbiter_can_resolve_with_release() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    assert!(escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::Release));

    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn arbiter_can_resolve_with_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    assert!(escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::Refund));

    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Refunded);
}

#[test]
fn arbiter_can_resolve_with_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    assert!(escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::Cancel));

    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Cancelled);
}

// ── resolve_dispute error paths ───────────────────────────────────────────────

#[test]
#[should_panic]
fn client_cannot_resolve_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    escrow.resolve_dispute(&id, &client_addr, &DisputeResolution::Release);
}

#[test]
#[should_panic]
fn freelancer_cannot_resolve_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, freelancer_addr, _, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    escrow.resolve_dispute(&id, &freelancer_addr, &DisputeResolution::Release);
}

#[test]
#[should_panic]
fn third_party_cannot_resolve_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));
    let outsider = Address::generate(&env);

    escrow.resolve_dispute(&id, &outsider, &DisputeResolution::Release);
}

#[test]
#[should_panic]
fn cannot_resolve_non_disputed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, _, arbiter_addr, id) = funded_with_arbiter(&env, &escrow);
    // Not disputed yet

    escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::Release);
}

// ── state blocking ────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn release_milestone_blocked_in_disputed_state() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (client_addr, _, _, id) = funded_with_arbiter(&env, &escrow);
    escrow.raise_dispute(&id, &client_addr, &reason_hash(&env));

    escrow.release_milestone(&id, &0);
}

// ── get_dispute error path ────────────────────────────────────────────────────

#[test]
#[should_panic]
fn get_dispute_fails_when_no_dispute_exists() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register(&env);
    let (_, _, _, id) = funded_with_arbiter(&env, &escrow);

    escrow.get_dispute(&id);
}
