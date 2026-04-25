//! Tests for the milestone evidence hash feature.
//!
//! Covers authorization, write-once immutability, bounds checking, and the
//! happy-path read/write flow.

use super::{create_contract, register_client, total_milestone_amount};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xab; 32])
}

fn other_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xcd; 32])
}

// ── happy path ────────────────────────────────────────────────────────────────

/// `get_milestone_evidence_hash` returns `None` before any hash is set.
#[test]
fn get_returns_none_before_set() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    assert!(client.get_milestone_evidence_hash(&contract_id, &0).is_none());
}

/// Client can set a hash; it is retrievable afterwards.
#[test]
fn client_can_set_and_retrieve_hash() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    let hash = dummy_hash(&env);
    assert!(client.set_milestone_evidence_hash(&contract_id, &0, &hash, &client_addr));

    let stored = client
        .get_milestone_evidence_hash(&contract_id, &0)
        .expect("hash should be stored");
    assert_eq!(stored, hash);
}

/// Freelancer can also set a hash.
#[test]
fn freelancer_can_set_hash() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, freelancer_addr, contract_id) = create_contract(&env, &client);

    let hash = dummy_hash(&env);
    assert!(client.set_milestone_evidence_hash(&contract_id, &0, &hash, &freelancer_addr));
    assert_eq!(
        client.get_milestone_evidence_hash(&contract_id, &0).unwrap(),
        hash
    );
}

/// Hashes for different milestones are stored independently.
#[test]
fn hashes_are_isolated_per_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    let h0 = dummy_hash(&env);
    let h1 = other_hash(&env);

    client.set_milestone_evidence_hash(&contract_id, &0, &h0, &client_addr);
    client.set_milestone_evidence_hash(&contract_id, &1, &h1, &client_addr);

    assert_eq!(client.get_milestone_evidence_hash(&contract_id, &0).unwrap(), h0);
    assert_eq!(client.get_milestone_evidence_hash(&contract_id, &1).unwrap(), h1);
    assert!(client.get_milestone_evidence_hash(&contract_id, &2).is_none());
}

/// A hash set on one contract does not appear on another.
#[test]
fn hashes_are_isolated_across_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr_a, _, id_a) = create_contract(&env, &client);
    let (client_addr_b, _, id_b) = create_contract(&env, &client);

    client.set_milestone_evidence_hash(&id_a, &0, &dummy_hash(&env), &client_addr_a);

    assert!(client.get_milestone_evidence_hash(&id_a, &0).is_some());
    assert!(client.get_milestone_evidence_hash(&id_b, &0).is_none());
    let _ = client_addr_b; // suppress unused warning
}

// ── immutability ──────────────────────────────────────────────────────────────

/// A second `set_milestone_evidence_hash` call for the same milestone panics.
#[test]
#[should_panic]
fn overwrite_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    client.set_milestone_evidence_hash(&contract_id, &0, &dummy_hash(&env), &client_addr);
    // Second call — must panic with EvidenceHashAlreadySet.
    client.set_milestone_evidence_hash(&contract_id, &0, &other_hash(&env), &client_addr);
}

/// Even the same hash value cannot be re-submitted (write-once, not idempotent).
#[test]
#[should_panic]
fn resubmitting_same_hash_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    let hash = dummy_hash(&env);
    client.set_milestone_evidence_hash(&contract_id, &0, &hash.clone(), &client_addr);
    client.set_milestone_evidence_hash(&contract_id, &0, &hash, &client_addr);
}

// ── authorization ─────────────────────────────────────────────────────────────

/// A third-party address (not client or freelancer) is rejected.
#[test]
#[should_panic]
fn unauthorized_caller_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let attacker = Address::generate(&env);
    client.set_milestone_evidence_hash(&contract_id, &0, &dummy_hash(&env), &attacker);
}

/// The arbiter (if present) cannot set evidence — only client/freelancer can.
#[test]
#[should_panic]
fn arbiter_cannot_set_hash() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    // create_contract with arbiter — call the contract directly with arbiter param
    let contract_id = escrow.create_contract(&client_addr, &freelancer_addr, &Some(arbiter_addr.clone()), &milestones);

    escrow.set_milestone_evidence_hash(&contract_id, &0, &dummy_hash(&env), &arbiter_addr);
}

// ── bounds ────────────────────────────────────────────────────────────────────

/// An out-of-range milestone index is rejected.
#[test]
#[should_panic]
fn out_of_range_index_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    client.set_milestone_evidence_hash(&contract_id, &99, &dummy_hash(&env), &client_addr);
}

/// Setting a hash on a non-existent contract panics.
#[test]
#[should_panic]
fn missing_contract_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    client.set_milestone_evidence_hash(&999, &0, &dummy_hash(&env), &caller);
}

// ── integration ───────────────────────────────────────────────────────────────

/// Evidence hash survives a full release cycle — it remains readable after the
/// milestone is released and the contract completes.
#[test]
fn hash_persists_after_milestone_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    let hash = dummy_hash(&env);
    client.set_milestone_evidence_hash(&contract_id, &0, &hash.clone(), &client_addr);

    // Fund and release milestone 0.
    client.deposit_funds(&contract_id, &total_milestone_amount());
    client.release_milestone(&contract_id, &0);

    // Hash must still be readable.
    assert_eq!(
        client.get_milestone_evidence_hash(&contract_id, &0).unwrap(),
        hash
    );
}
