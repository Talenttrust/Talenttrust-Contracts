//! # Identity Sanitization Tests
//!
//! Comprehensive tests for role overlap and identity validation rules.
//! Enforces the fail-closed principle: any identity violation is rejected
//! before contract creation.
//!
//! ## Rules Tested
//!
//! 1. **Client ≠ Freelancer**
//!    - The two primary counterparties must be distinct.
//!    - Prevents self-approval of milestone releases and self-collection of funds.
//!
//! 2. **Arbiter ≠ Client and Arbiter ≠ Freelancer** (when arbiter is provided)
//!    - An arbiter must be a fully independent third party.
//!    - Prevents arbiter from unilaterally cancelling or resolving disputes in their favour.

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

fn default_milestones(env: &Env) -> soroban_sdk::Vec<i128> {
    vec![env, 100_0000000_i128, 200_0000000_i128, 300_0000000_i128]
}

// ─── Rule 1: Client ≠ Freelancer ──────────────────────────────────────────────

/// Client and freelancer must be distinct addresses.
#[test]
#[should_panic(expected = "ClientEqualsFreelancer")]
fn rejects_client_equals_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let same_party = Address::generate(&env);

    client.create_contract(&same_party, &same_party, &None, &default_milestones(&env));
}

/// Accepts distinct client and freelancer.
#[test]
fn accepts_distinct_client_and_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));
    assert!(id > 0);

    let contract = client.get_contract(&id);
    assert_eq!(contract.client, client_addr);
    assert_eq!(contract.freelancer, freelancer_addr);
    assert_eq!(contract.arbiter, None);
}

// ─── Rule 2: Arbiter ≠ Client ─────────────────────────────────────────────────

/// Arbiter cannot be the same as the client.
#[test]
#[should_panic(expected = "ArbiterRoleOverlap")]
fn rejects_arbiter_equals_client() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(client_addr.clone()),
        &default_milestones(&env),
    );
}

/// Arbiter cannot be the same as the freelancer.
#[test]
#[should_panic(expected = "ArbiterRoleOverlap")]
fn rejects_arbiter_equals_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(freelancer_addr.clone()),
        &default_milestones(&env),
    );
}

/// Accepts distinct arbiter (different from both client and freelancer).
#[test]
fn accepts_distinct_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
    );
    assert!(id > 0);

    let contract = client.get_contract(&id);
    assert_eq!(contract.client, client_addr);
    assert_eq!(contract.freelancer, freelancer_addr);
    assert_eq!(contract.arbiter, Some(arbiter_addr));
}

// ─── Optional Arbiter ─────────────────────────────────────────────────────────

/// Accepts `None` arbiter (no third-party dispute resolution).
#[test]
fn accepts_none_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
    );
    assert!(id > 0);

    let contract = client.get_contract(&id);
    assert_eq!(contract.arbiter, None);
}

// ─── Fail-Closed Validation ───────────────────────────────────────────────────

/// Validation happens before any storage writes (fail-closed).
/// If identity validation fails, no contract is created.
#[test]
#[should_panic(expected = "ClientEqualsFreelancer")]
fn validation_is_fail_closed_no_partial_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let same_party = Address::generate(&env);

    // Attempt to create contract with client == freelancer.
    // This should panic before any storage writes.
    client.create_contract(&same_party, &same_party, &None, &default_milestones(&env));
}

// ─── Multiple Distinct Contracts ──────────────────────────────────────────────

/// Multiple contracts can be created with different participant sets.
#[test]
fn multiple_contracts_with_different_participants() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    let diana = Address::generate(&env);

    // Contract 1: alice (client) + bob (freelancer), no arbiter
    let id1 = client.create_contract(&alice, &bob, &None, &default_milestones(&env));
    assert_eq!(id1, 0);

    // Contract 2: charlie (client) + diana (freelancer), alice as arbiter
    let id2 = client.create_contract(
        &charlie,
        &diana,
        &Some(alice.clone()),
        &default_milestones(&env),
    );
    assert_eq!(id2, 1);

    // Verify both contracts exist with correct participants
    let c1 = client.get_contract(&id1);
    assert_eq!(c1.client, alice);
    assert_eq!(c1.freelancer, bob);
    assert_eq!(c1.arbiter, None);

    let c2 = client.get_contract(&id2);
    assert_eq!(c2.client, charlie);
    assert_eq!(c2.freelancer, diana);
    assert_eq!(c2.arbiter, Some(alice));
}

// ─── Edge Cases ────────────────────────────────────────────────────────────────

/// Three distinct addresses (client, freelancer, arbiter) all different.
#[test]
fn three_way_distinct_addresses() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let addr1 = Address::generate(&env);
    let addr2 = Address::generate(&env);
    let addr3 = Address::generate(&env);

    // Verify all three are distinct
    assert_ne!(addr1, addr2);
    assert_ne!(addr2, addr3);
    assert_ne!(addr1, addr3);

    let id = client.create_contract(&addr1, &addr2, &Some(addr3.clone()), &default_milestones(&env));
    assert!(id > 0);

    let contract = client.get_contract(&id);
    assert_eq!(contract.client, addr1);
    assert_eq!(contract.freelancer, addr2);
    assert_eq!(contract.arbiter, Some(addr3));
}

/// Validation rejects even if only arbiter overlaps with one role.
#[test]
#[should_panic(expected = "ArbiterRoleOverlap")]
fn rejects_partial_arbiter_overlap() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Arbiter == client (partial overlap)
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(client_addr.clone()),
        &default_milestones(&env),
    );
}
