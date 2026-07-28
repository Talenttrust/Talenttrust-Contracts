//! Round-trip and TTL-bump tests for the milestone-vector accessors
//! introduced in issue #701 (`load_milestones`, `try_load_milestones`,
//! `store_milestones`, `milestone_storage_key`).
//!
//! These tests lock in the contract surface:
//!
//!  * `load_milestones` and `try_load_milestones` return the canonical
//!    `Vec<Milestone>` from `(DataKey::Contract(id), Symbol("milestones"))`
//!    and bump the persistent TTL on success.
//!  * `load_milestones` panics with `Error::ContractNotFound` on a missing
//!    vector; `try_load_milestones` returns `None`.
//!  * `store_milestones` persists under the same composite key and bumps
//!    the TTL atomically with the write.
//!  * `milestone_storage_key` is the single source of the composite key.

use super::{create_contract, default_milestones, register_client, total_milestone_amount};
use crate::{ttl, Error, Milestone};
use soroban_sdk::{
    testutils::{storage::Persistent, Ledger},
    Vec,
};

fn setup_long_ttl_env() -> soroban_sdk::Env {
    let env = soroban_sdk::Env::default();
    env.ledger().with_mut(|li| {
        li.max_entry_ttl = ttl::LEDGERS_PER_DAY * 60;
        li.min_persistent_entry_ttl = ttl::LEDGERS_PER_DAY * 60;
        li.sequence_number = 1_000;
    });
    env.mock_all_auths();
    env
}

// ─── load_milestones: panic on missing ────────────────────────────────────

/// `load_milestones` panics with `Error::ContractNotFound` when called
/// against a contract id that has no persisted milestone vector.
#[test]
#[should_panic(expected = "ContractNotFound")]
fn load_milestones_panics_for_unknown_contract() {
    let env = setup_long_ttl_env();
    let _client = register_client(&env);
    crate::load_milestones(&env, 9_999);
}

// ─── load_milestones: success ──────────────────────────────────────────────

/// After `create_contract` the milestone vector can be loaded and its
/// initial state matches the input amounts/flags.
#[test]
fn load_milestones_returns_initial_vector() {
    let env = setup_long_ttl_env();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let loaded = crate::load_milestones(&env, contract_id);
    let expected = total_milestone_amount();
    let sum: i128 = loaded.iter().map(|m| m.amount).sum();
    assert_eq!(sum, expected);
    for m in loaded.iter() {
        assert!(!m.released);
        assert!(!m.refunded);
        assert_eq!(m.funded_amount, 0);
        assert_eq!(m.refunded_amount, 0);
        assert!(m.work_evidence.is_none());
    }
}

// ─── try_load_milestones: None for missing ─────────────────────────────────

/// `try_load_milestones` returns `None` for a contract id that has no
/// persisted milestone vector — distinct from the panic semantics of
/// `load_milestones`.
#[test]
fn try_load_milestones_returns_none_for_unknown_contract() {
    let env = setup_long_ttl_env();
    let _client = register_client(&env);
    let result = crate::try_load_milestones(&env, 9_999);
    assert!(result.is_none());
}

/// `try_load_milestones` returns `Some(Vec<Milestone>)` for an
/// existing contract.
#[test]
fn try_load_milestones_returns_some_for_existing_contract() {
    let env = setup_long_ttl_env();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let result = crate::try_load_milestones(&env, contract_id);
    let loaded = result.expect("milestone vector should exist for created contract");
    assert!(!loaded.is_empty());
    assert_eq!(loaded.len(), default_milestones(&env).len());
}

// ─── store_milestones: round-trip ──────────────────────────────────────────

/// Round-trip: load → mutate → store → load again yields the mutated vector.
#[test]
fn store_milestones_round_trips_mutations() {
    let env = setup_long_ttl_env();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let mut milestones: Vec<Milestone> = crate::load_milestones(&env, contract_id);
    let mut modified = milestones.get(0).unwrap();
    modified.refunded = true;
    modified.refunded_amount = modified.amount;
    milestones.set(0, modified);

    crate::store_milestones(&env, contract_id, &milestones);

    let reloaded = crate::load_milestones(&env, contract_id);
    let first = reloaded.get(0).unwrap();
    assert!(
        first.refunded,
        "milestone.refunded should be true after store"
    );
    assert_eq!(first.refunded_amount, first.amount);
    for i in 1..reloaded.len() {
        let m = reloaded.get(i).unwrap();
        assert!(!m.refunded);
        assert_eq!(m.refunded_amount, 0);
    }
}

// ─── store_milestones: empty vector edge case ──────────────────────────────

/// Edge case: `store_milestones` accepts an empty vector and a subsequent
/// `load_milestones` returns the same empty vector.
#[test]
fn store_milestones_round_trips_empty_vector() {
    let env = setup_long_ttl_env();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let empty: Vec<Milestone> = Vec::new(&env);
    crate::store_milestones(&env, contract_id, &empty);

    let loaded = crate::load_milestones(&env, contract_id);
    assert_eq!(loaded.len(), 0);
}

// ─── store_milestones: large vector edge case ──────────────────────────────

/// Edge case: `store_milestones` handles the maximum-milestones vector
/// unchanged (covers the bound at `MAX_MILESTONES = 10`).
#[test]
fn store_milestones_round_trips_max_size_vector() {
    let env = setup_long_ttl_env();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let mut maxed: Vec<Milestone> = Vec::new(&env);
    for _ in 0..crate::MAX_MILESTONES {
        maxed.push_back(Milestone {
            amount: 100_i128,
            funded_amount: 0,
            released: false,
            refunded: false,
            work_evidence: None,
            refunded_amount: 0,
            deadline: None,
        });
    }
    crate::store_milestones(&env, contract_id, &maxed);

    let loaded = crate::load_milestones(&env, contract_id);
    assert_eq!(loaded.len() as u32, crate::MAX_MILESTONES);
    for i in 0..crate::MAX_MILESTONES {
        let m = loaded.get(i).unwrap();
        assert_eq!(m.amount, 100_i128);
        assert!(!m.released);
        assert!(!m.refunded);
    }
}

// ─── TTL-bump invariants ───────────────────────────────────────────────────

/// `load_milestones` extends the persistent TTL on a hit.
#[test]
fn load_milestones_bumps_persistent_ttl() {
    let env = setup_long_ttl_env();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let bump_threshold = ttl::PERSISTENT_BUMP_THRESHOLD as u32;
    let extension = ttl::PERSISTENT_TTL_LEDGERS as u32;

    let initial_ttl: u32 = env.as_contract(&client.address, || {
        let key = crate::milestone_storage_key(&env, contract_id);
        env.storage().persistent().get_ttl(&key)
    });
    env.ledger().with_mut(|li| {
        li.sequence_number = li
            .sequence_number
            .saturating_add(initial_ttl.saturating_sub(bump_threshold) + 1);
    });

    let _loaded = crate::load_milestones(&env, contract_id);

    let ttl_after: u32 = env.as_contract(&client.address, || {
        let key = crate::milestone_storage_key(&env, contract_id);
        env.storage().persistent().get_ttl(&key)
    });
    assert!(
        ttl_after >= bump_threshold,
        "load_milestones must extend TTL to at least the bump threshold (got {})",
        ttl_after
    );

    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number.saturating_add(extension - 1);
    });
    let _still_live = crate::load_milestones(&env, contract_id);
}

/// `store_milestones` extends the persistent TTL atomically with the write.
#[test]
fn store_milestones_bumps_persistent_ttl() {
    let env = setup_long_ttl_env();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let bump_threshold = ttl::PERSISTENT_BUMP_THRESHOLD as u32;
    let extension = ttl::PERSISTENT_TTL_LEDGERS as u32;

    let initial_ttl: u32 = env.as_contract(&client.address, || {
        let key = crate::milestone_storage_key(&env, contract_id);
        env.storage().persistent().get_ttl(&key)
    });
    env.ledger().with_mut(|li| {
        li.sequence_number = li
            .sequence_number
            .saturating_add(initial_ttl.saturating_sub(bump_threshold) + 1);
    });

    let milestones = crate::load_milestones(&env, contract_id);
    crate::store_milestones(&env, contract_id, &milestones);

    let ttl_after: u32 = env.as_contract(&client.address, || {
        let key = crate::milestone_storage_key(&env, contract_id);
        env.storage().persistent().get_ttl(&key)
    });
    assert!(
        ttl_after >= bump_threshold,
        "store_milestones must extend TTL to at least the bump threshold (got {})",
        ttl_after
    );

    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number.saturating_add(extension - 1);
    });
    let _still_live = crate::load_milestones(&env, contract_id);
}

// ─── milestone_storage_key invariants ──────────────────────────────────────

/// The composite key returned by `milestone_storage_key` must be exactly
/// `(DataKey::Contract(id), Symbol("milestones"))`.
#[test]
fn milestone_storage_key_returns_canonical_tuple() {
    let env = setup_long_ttl_env();
    let key = crate::milestone_storage_key(&env, 42);
    assert!(matches!(key.0, crate::DataKey::Contract(42)));
    let expected = soroban_sdk::Symbol::new(&env, "milestones");
    assert_eq!(key.1, expected);
}

// ─── Re-export semantics ───────────────────────────────────────────────────

/// The top-level `crate::load_milestones` / `crate::store_milestones` /
/// `crate::try_load_milestones` / `crate::milestone_storage_key` re-exports
/// resolve to the canonical implementations in `ttl`.
#[test]
fn re_exported_helpers_resolve() {
    let env = setup_long_ttl_env();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let direct: Vec<Milestone> = crate::load_milestones(&env, contract_id);
    let via_ttl: Vec<Milestone> = ttl::load_milestones(&env, contract_id);

    assert_eq!(direct.len(), via_ttl.len());
    for i in 0..direct.len() {
        assert_eq!(direct.get(i).unwrap(), via_ttl.get(i).unwrap());
    }
}

// ─── Composite-key store consistency ───────────────────────────────────────

/// Storing milestones through `store_milestones` then probing the same
/// composite key via the `Env` storage API directly returns the same value.
#[test]
fn store_milestones_writes_under_canonical_composite_key() {
    let env = setup_long_ttl_env();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let milestones: Vec<Milestone> = crate::load_milestones(&env, contract_id);
    crate::store_milestones(&env, contract_id, &milestones);

    env.as_contract(&client.address, || {
        let key = crate::milestone_storage_key(&env, contract_id);
        let stored: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&key)
            .expect("milestone vector must be present at the canonical key");
        assert_eq!(stored.len(), milestones.len());
    });
}

/// The helper panics (rather than returning silently) on missing entries —
/// observable guarantee off-chain tooling relies on.
#[test]
#[should_panic]
fn load_milestones_panics_on_missing() {
    let env = setup_long_ttl_env();
    let _client = register_client(&env);
    let _ = crate::load_milestones(&env, 12_345_u32);
    let _: Result<(), Error> = Err(Error::ContractNotFound);
}
