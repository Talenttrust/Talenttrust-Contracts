//! Contract-id allocation invariant tests.
//!
//! # Invariants verified
//! 1. Ids are allocated sequentially starting from 1 with no gaps.
//! 2. Each id is unique — no two live contracts share an id.
//! 3. `NextContractId` is advanced by exactly 1 after every successful create.
//! 4. A `ContractIdOverflow` error is returned when the counter is at `u32::MAX`.
//! 5. A `ContractIdCollision` error is returned when the target slot is occupied.
//! 6. Neither overflow nor collision mutates `NextContractId`.
//! 7. `contract_exists` correctly identifies existing and missing contracts.
//! 8. `get_next_contract_id` returns the allocation high-water mark.
//! 9. Existence probes do not extend TTL (security invariant).

use soroban_sdk::testutils::Ledger as _;
use super::{default_milestones, generated_participants, register_client};
use crate::{DataKey, Error, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, Address, Env};

// -----------------------------------------------------------------------
// Helper
// -----------------------------------------------------------------------

fn assert_error<T: core::fmt::Debug>(
    result: Result<
        Result<T, soroban_sdk::ConversionError>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
    expected: Error,
) {
    match result {
        Err(Ok(e)) => {
            let expected_err: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_err);
        }
        other => panic!("expected {:?}, got {:?}", expected, other),
    }
}

/// Read the persisted NextContractId counter directly from storage.
fn read_next_id(env: &Env, escrow_addr: &soroban_sdk::Address) -> u32 {
    env.as_contract(escrow_addr, || {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    })
}

// -----------------------------------------------------------------------
// Sequential / gap-free allocation
// -----------------------------------------------------------------------

/// The first contract ever created must receive id = 1 (the default seed).
#[test]
fn first_contract_id_is_one() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client, freelancer, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert_eq!(id, 1, "first allocated id must be 1");
}

/// Sequential creates must return ids 1, 2, 3, … with no gaps.
#[test]
fn ids_are_sequential_and_gap_free() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let milestones = default_milestones(&env);
    let count: u32 = 10;

    let mut ids: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&env);
    for _ in 0..count {
        let (client, freelancer, _) = generated_participants(&env);
        let id = escrow.create_contract(
            &client,
            &freelancer,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );
        ids.push_back(id);
    }

    for (i, id) in ids.iter().enumerate() {
        assert_eq!(id, (i as u32) + 1, "id at position {i} should be {}", i + 1);
    }
}

/// After N creates the stored counter equals N + 1 (ready for the next create).
#[test]
fn counter_advances_exactly_one_per_create() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let milestones = default_milestones(&env);
    let count: u32 = 5;

    for i in 0..count {
        let (client, freelancer, _) = generated_participants(&env);
        escrow.create_contract(
            &client,
            &freelancer,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );
        let next = read_next_id(&env, &escrow.address);
        assert_eq!(next, i + 2, "counter after {}", i + 1);
    }
}

/// All allocated ids must be unique across many sequential creates.
#[test]
fn all_ids_are_unique() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let milestones = default_milestones(&env);
    let count: u32 = 20;

    let mut seen: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&env);
    for _ in 0..count {
        let (client, freelancer, _) = generated_participants(&env);
        let id = escrow.create_contract(
            &client,
            &freelancer,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );
        // Verify no duplicate
        for existing in seen.iter() {
            assert_ne!(existing, id, "duplicate id {id} detected");
        }
        seen.push_back(id);
    }
    assert_eq!(seen.len(), count);
}

// -----------------------------------------------------------------------
// Overflow protection
// -----------------------------------------------------------------------

/// When `NextContractId` is `u32::MAX` the counter cannot be advanced and
/// `ContractIdOverflow` must be returned.  The counter must remain unchanged.
#[test]
fn next_contract_id_overflow_at_u32_max() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client_addr, freelancer_addr, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &u32::MAX);
    });

    let result = escrow.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_error(result, Error::ContractIdOverflow);

    // Counter must not have moved.
    let after = read_next_id(&env, &escrow.address);
    assert_eq!(after, u32::MAX, "counter must not change on overflow");
}

/// Overflow at `u32::MAX - 1` does not fire; at `u32::MAX` it does.
#[test]
fn overflow_fires_only_at_u32_max_not_before() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client, freelancer, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    // Place counter at u32::MAX - 1; the create should succeed.
    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(u32::MAX - 1));
    });

    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, u32::MAX - 1);

    // Counter is now u32::MAX; next create must overflow.
    let after = read_next_id(&env, &escrow.address);
    assert_eq!(after, u32::MAX);

    let (c2, f2, _) = generated_participants(&env);
    let result = escrow.try_create_contract(
        &c2,
        &f2,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_error(result, Error::ContractIdOverflow);
}

// -----------------------------------------------------------------------
// Collision protection
// -----------------------------------------------------------------------

/// `ContractIdCollision` fires when the target slot is already occupied and
/// `NextContractId` must not change.
#[test]
fn next_contract_id_rejects_occupied_slot() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client_addr, freelancer_addr, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    let existing_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Wind the counter back so the next allocation targets the occupied slot.
    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &existing_id);
    });

    let intruder = Address::generate(&env);
    let result = escrow.try_create_contract(
        &intruder,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_error(result, Error::ContractIdCollision);

    // Counter must not have advanced past the collision point.
    let after = read_next_id(&env, &escrow.address);
    assert_eq!(after, existing_id, "counter must not advance on collision");
}

/// A collision does not corrupt subsequent creates once the counter is fixed.
#[test]
fn allocation_resumes_correctly_after_counter_is_repaired() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client, freelancer, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    // Create id=1, then wind counter back to 1 (simulating corruption).
    escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &1u32);
    });

    // Attempted create collides at id=1.
    let (c2, f2, _) = generated_participants(&env);
    let collision = escrow.try_create_contract(
        &c2,
        &f2,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_error(collision, Error::ContractIdCollision);

    // Repair: advance counter to 2 (what it should have been).
    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &2u32);
    });

    // Next create must succeed at id=2.
    let (c3, f3, _) = generated_participants(&env);
    let id = escrow.create_contract(
        &c3,
        &f3,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 2);
}

// -----------------------------------------------------------------------
// Single-call allocation — no intermediate state exposed
// -----------------------------------------------------------------------

/// Only one contract is stored per `create_contract` call.
/// This guards against the old double-call bug leaving phantom storage entries.
#[test]
fn single_create_stores_exactly_one_contract() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client, freelancer, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Exactly the returned id should be in storage.
    env.as_contract(&escrow.address, || {
        let exists: bool = env
            .storage()
            .persistent()
            .has(&DataKey::Contract(id));
        assert!(exists, "contract {id} should be in storage");

        // No other ids should exist (0, 2, … are all absent after first create).
        let phantom0: bool = env
            .storage()
            .persistent()
            .has(&DataKey::Contract(0));
        assert!(!phantom0, "phantom contract at id 0 must not exist");

        let phantom2: bool = env
            .storage()
            .persistent()
            .has(&DataKey::Contract(id + 1));
        assert!(!phantom2, "phantom contract at id+1 must not exist");
    });
}

// -----------------------------------------------------------------------
// contract_exists and get_next_contract_id readers
// -----------------------------------------------------------------------

/// `contract_exists` returns `true` for an allocated contract and `false` for
/// an ID that was never allocated.
#[test]
fn contract_exists_identifies_allocated_and_missing_ids() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client, freelancer, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    // Before any creates, no contracts exist.
    assert!(!escrow.contract_exists(1), "id 1 should not exist before creation");
    assert!(!escrow.contract_exists(999), "id 999 should not exist");

    // Create one contract.
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);

    // The allocated id exists.
    assert!(escrow.contract_exists(1), "id 1 should exist after creation");

    // Unallocated ids still do not exist.
    assert!(!escrow.contract_exists(2), "id 2 should not exist yet");
    assert!(!escrow.contract_exists(0), "id 0 should never exist");
    assert!(!escrow.contract_exists(1000), "id 1000 should not exist");

    // Create a second contract.
    let (c2, f2, _) = generated_participants(&env);
    let id2 = escrow.create_contract(
        &c2,
        &f2,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id2, 2);

    assert!(escrow.contract_exists(1), "id 1 should still exist");
    assert!(escrow.contract_exists(2), "id 2 should exist after second creation");
    assert!(!escrow.contract_exists(3), "id 3 should not exist yet");
}

/// `get_next_contract_id` returns the allocation high-water mark.
#[test]
fn get_next_contract_id_returns_high_water_mark() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client, freelancer, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    // Before initialization, the default is 1.
    // (In practice callers should initialize first, but the reader is safe.)
    let before_init = escrow.get_next_contract_id();
    assert_eq!(before_init, 1, "default next id before init should be 1");

    // Initialize the contract.
    let admin = Address::generate(&env);
    escrow.initialize(&admin);

    // After initialization, next id is 1.
    assert_eq!(escrow.get_next_contract_id(), 1);

    // After creating one contract, next id is 2.
    escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(escrow.get_next_contract_id(), 2);

    // After creating three total, next id is 4.
    for _ in 0..2 {
        let (c, f, _) = generated_participants(&env);
        escrow.create_contract(
            &c,
            &f,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );
    }
    assert_eq!(escrow.get_next_contract_id(), 4);
}

/// `contract_exists` does not panic on missing IDs (unlike `get_contract`).
#[test]
fn contract_exists_does_not_panic_on_missing_id() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client, freelancer, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // These must all return false without panicking.
    assert!(!escrow.contract_exists(0));
    assert!(!escrow.contract_exists(2));
    assert!(!escrow.contract_exists(100));
    assert!(!escrow.contract_exists(u32::MAX));
}

/// `contract_exists` combined with `get_next_contract_id` gives indexers a
/// safe iteration pattern: `[1, next_id - 1]`.
#[test]
fn indexer_iteration_pattern_with_contract_exists_and_next_id() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let milestones = default_milestones(&env);

    // Create contracts with ids 1, 2, 3 (skip id 4 by winding counter).
    for i in 0..3 {
        let (c, f, _) = generated_participants(&env);
        escrow.create_contract(
            &c,
            &f,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );
    }

    // Wind counter to 5 so id 4 is skipped (simulating a gap).
    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &5u32);
    });

    let next_id = escrow.get_next_contract_id();
    assert_eq!(next_id, 5);

    // Collect all existing ids via the safe probe.
    let mut found: Vec<u32> = Vec::new(&env);
    for id in 1..next_id {
        if escrow.contract_exists(id) {
            found.push_back(id);
        }
    }

    // Only ids 1, 2, 3 should be found; id 4 is a gap.
    assert_eq!(found.len(), 3);
    assert_eq!(found.get(0), Some(&1));
    assert_eq!(found.get(1), Some(&2));
    assert_eq!(found.get(2), Some(&3));
}

/// `contract_exists` is a read-only probe and must not extend the contract TTL.
/// We verify this by checking that the contract's TTL is unchanged after probing.
#[test]
fn contract_exists_does_not_extend_ttl() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client, freelancer, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Read the TTL before probing.
    let ttl_before: u32 = env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::Contract(1))
            .unwrap()
    });

    // Probe existence — this must NOT extend TTL.
    let exists = escrow.contract_exists(1);
    assert!(exists);

    // Read the TTL after probing.
    let ttl_after: u32 = env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::Contract(1))
            .unwrap()
    });

    // TTL must be unchanged.
    assert_eq!(
        ttl_before, ttl_after,
        "contract_exists must not extend TTL (before={}, after={})",
        ttl_before, ttl_after
    );
}

/// `get_next_contract_id` is also read-only and must not extend any TTL.
#[test]
fn get_next_contract_id_does_not_extend_ttl() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client, freelancer, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Read the contract TTL before calling get_next_contract_id.
    let ttl_before: u32 = env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::Contract(1))
            .unwrap()
    });

    // Call the reader.
    let _next = escrow.get_next_contract_id();

    // Read the contract TTL after.
    let ttl_after: u32 = env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::Contract(1))
            .unwrap()
    });

    assert_eq!(
        ttl_before, ttl_after,
        "get_next_contract_id must not extend contract TTL (before={}, after={})",
        ttl_before, ttl_after
    );

    // Also verify NextContractId TTL is unchanged.
    let next_ttl_before: u32 = env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::NextContractId)
            .unwrap()
    });
    let _next2 = escrow.get_next_contract_id();
    let next_ttl_after: u32 = env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::NextContractId)
            .unwrap()
    });
    assert_eq!(
        next_ttl_before, next_ttl_after,
        "get_next_contract_id must not extend NextContractId TTL"
    );
}
