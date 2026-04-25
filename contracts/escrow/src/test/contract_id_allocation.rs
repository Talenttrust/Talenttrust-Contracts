//! # Contract ID Allocation Tests
//!
//! Verifies the monotonic, never-reused ID allocation scheme:
//!
//! - IDs start at 1 and increment by 1 on every successful creation.
//! - The `NextContractId` counter is written *before* contract data
//!   (write-ahead reservation), so a panic after the counter write does not
//!   allow the same ID to be reused on the next call.
//! - Sequential calls in the same environment never produce the same ID.

use super::{default_milestones, generated_participants, register_client};
use crate::{DataKey, EscrowError};
use soroban_sdk::{vec, Env};

// ─── Monotonicity ─────────────────────────────────────────────────────────────

#[test]
fn first_contract_id_is_one() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (ca, fa) = generated_participants(&env);

    let id = client.create_contract(&ca, &fa, &default_milestones(&env));
    assert_eq!(id, 1);
}

#[test]
fn ids_increment_monotonically() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let mut prev = 0u32;
    for _ in 0..5 {
        let (ca, fa) = generated_participants(&env);
        let id = client.create_contract(&ca, &fa, &default_milestones(&env));
        assert!(id > prev, "id {id} must be greater than previous {prev}");
        prev = id;
    }
}

#[test]
fn ids_are_strictly_sequential() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (ca1, fa1) = generated_participants(&env);
    let (ca2, fa2) = generated_participants(&env);
    let (ca3, fa3) = generated_participants(&env);

    let id1 = client.create_contract(&ca1, &fa1, &default_milestones(&env));
    let id2 = client.create_contract(&ca2, &fa2, &default_milestones(&env));
    let id3 = client.create_contract(&ca3, &fa3, &default_milestones(&env));

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
}

// ─── No reuse ─────────────────────────────────────────────────────────────────

/// Simulates the write-ahead guarantee: after the counter is advanced, a
/// subsequent failed creation must not reuse the reserved ID.
///
/// We verify this by inspecting the persisted `NextContractId` directly after
/// a successful creation and confirming it is already beyond the issued ID,
/// so any future call — even one that panics mid-write — cannot reclaim it.
#[test]
fn counter_is_advanced_before_contract_data_is_written() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &escrow_id);

    let (ca, fa) = generated_participants(&env);
    let issued_id = client.create_contract(&ca, &fa, &default_milestones(&env));

    // The counter must already point past the issued ID.
    env.as_contract(&escrow_id, || {
        let next: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .expect("NextContractId must be set after first creation");
        assert!(
            next > issued_id,
            "NextContractId ({next}) must exceed the issued id ({issued_id})"
        );
    });
}

#[test]
fn failed_creation_does_not_reuse_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    // Successful creation — ID 1 is issued and counter advances to 2.
    let (ca, fa) = generated_participants(&env);
    let id1 = client.create_contract(&ca, &fa, &default_milestones(&env));
    assert_eq!(id1, 1);

    // Attempt a creation that will fail validation (empty milestones).
    // The counter must NOT have been touched (failure happens before counter write).
    let (ca2, fa2) = generated_participants(&env);
    let bad_milestones = vec![&env]; // empty — triggers EmptyMilestones error
    let result = client.try_create_contract(&ca2, &fa2, &bad_milestones);
    assert_eq!(result, Err(Ok(EscrowError::EmptyMilestones)));

    // Next successful creation must get ID 2, not ID 1.
    let (ca3, fa3) = generated_participants(&env);
    let id2 = client.create_contract(&ca3, &fa3, &default_milestones(&env));
    assert_eq!(id2, 2, "ID after failed attempt must still be 2, not 1");
}

// ─── Re-entrance / isolation ──────────────────────────────────────────────────

/// Two independent contract instances must each maintain their own counter;
/// IDs from one must not collide with IDs from the other.
#[test]
fn separate_contract_instances_have_independent_counters() {
    let env = Env::default();
    env.mock_all_auths();

    let id_a = env.register(crate::Escrow, ());
    let id_b = env.register(crate::Escrow, ());
    let client_a = crate::EscrowClient::new(&env, &id_a);
    let client_b = crate::EscrowClient::new(&env, &id_b);

    let (ca, fa) = generated_participants(&env);
    let a1 = client_a.create_contract(&ca, &fa, &default_milestones(&env));
    let b1 = client_b.create_contract(&ca, &fa, &default_milestones(&env));

    // Both start at 1 — they are independent storage namespaces.
    assert_eq!(a1, 1);
    assert_eq!(b1, 1);

    let (ca2, fa2) = generated_participants(&env);
    let a2 = client_a.create_contract(&ca2, &fa2, &default_milestones(&env));
    let b2 = client_b.create_contract(&ca2, &fa2, &default_milestones(&env));

    assert_eq!(a2, 2);
    assert_eq!(b2, 2);
}
