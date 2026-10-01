//! Tests for the versioned reputation storage migration path (issue #1012).
//!
//! Coverage matrix
//! ───────────────
//! | Scenario | Test |
//! |----------|------|
//! | v1 (legacy) record migrates to v2, data preserved | [`migration_v1_to_v2_preserves_data`] |
//! | v1 record with zero values migrates cleanly | [`migration_v1_zero_values_migrates`] |
//! | v2 (current) record is a no-op, returns false | [`migration_current_version_is_noop`] |
//! | Absent record (never written) is a no-op, storage untouched | [`migration_absent_record_is_noop`] |
//! | migration-on-read helper upgrades v1 when called explicitly | [`get_reputation_is_read_only_does_not_migrate_v1`] |
//! | get_reputation on absent address returns None | [`get_reputation_absent_returns_none`] |
//! | multiple migrate calls are idempotent | [`migrate_is_idempotent`] |
//! | migrate_reputation_storage public entrypoint returns true on migration | [`public_entrypoint_returns_true_on_migration`] |
//! | public entrypoint returns false for already-current record | [`public_entrypoint_returns_false_on_noop`] |
//! | version marker is written with correct value after migration | [`version_marker_written_correctly`] |
//! | reputation issued after migration is still readable | [`issue_reputation_after_migration_readable`] |
//! | public entrypoint on unknown address does not panic | [`public_entrypoint_unknown_address_does_not_panic`] |
//! | v1 readable via all query paths, reads side-effect free | [`compat_v1_readable_via_all_query_paths`] |
//! | mixed v1+v2 pagination | [`compat_mixed_v1_v2_pagination`] |
//! | future marker is a no-op | [`compat_future_marker_is_noop`] |
//! | corrupted zero marker heals to v2 | [`compat_corrupted_zero_marker_heals`] |
//! | zero-contract boundary | [`compat_zero_contracts_boundary`] |
//! | idempotency matrix (v1/v2/absent) | [`compat_idempotency_matrix`] |

use crate::{
    reputation_migration::{migrate_reputation_storage_impl, read_reputation_version},
    DataKey, Reputation, REPUTATION_STORAGE_VERSION,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

use super::register_client;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Write a bare v1 reputation record directly into persistent storage, bypassing
/// the version marker, to simulate legacy on-chain state.
fn write_v1_reputation(env: &Env, escrow_addr: &Address, address: &Address, rep: &Reputation) {
    env.as_contract(escrow_addr, || {
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(address.clone()), rep);
        // Intentionally do NOT write ReputationStorageVersion — this is the v1 layout.
    });
}

/// Read the version marker directly from persistent storage (None = never written).
fn read_version_direct(env: &Env, escrow_addr: &Address, address: &Address) -> Option<u32> {
    env.as_contract(escrow_addr, || {
        env.storage()
            .persistent()
            .get::<_, u32>(&DataKey::ReputationStorageVersion(address.clone()))
    })
}

/// Read the reputation record directly from persistent storage.
fn read_reputation_direct(
    env: &Env,
    escrow_addr: &Address,
    address: &Address,
) -> Option<Reputation> {
    env.as_contract(escrow_addr, || {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(address.clone()))
    })
}

/// Write a version marker directly (for future/corrupted-marker tests).
fn write_version_direct(env: &Env, escrow_addr: &Address, address: &Address, version: u32) {
    env.as_contract(escrow_addr, || {
        env.storage().persistent().set(
            &DataKey::ReputationStorageVersion(address.clone()),
            &version,
        );
    });
}

/// Append an address to the reputation index (simulates legacy index state).
fn push_to_index(env: &Env, escrow_addr: &Address, address: &Address) {
    env.as_contract(escrow_addr, || {
        let mut idx: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::ReputationIndex)
            .unwrap_or_else(|| Vec::new(env));
        idx.push_back(address.clone());
        env.storage()
            .persistent()
            .set(&DataKey::ReputationIndex, &idx);
    });
}

fn valid_comment(env: &Env) -> String {
    String::from_str(env, "Great work!")
}

// ── Migration correctness ────────────────────────────────────────────────────

/// A v1 record (no version marker) is upgraded to v2 and all field values are
/// preserved exactly.
#[test]
fn migration_v1_to_v2_preserves_data() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let original = Reputation {
        completed_contracts: 7,
        total_rating: 31,
        last_rating: 4,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &original);

    // Confirm pre-migration state: no version marker, record is present.
    assert_eq!(read_version_direct(&env, &escrow_addr, &freelancer), None);

    let migrated = env.as_contract(&escrow_addr, || {
        migrate_reputation_storage_impl(&env, &freelancer)
    });
    assert!(
        migrated,
        "expected migration to report true for a v1 record"
    );

    // Post-migration: version marker must equal the current version.
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &freelancer),
        Some(REPUTATION_STORAGE_VERSION)
    );

    // Data preserved exactly.
    let after = read_reputation_direct(&env, &escrow_addr, &freelancer)
        .expect("reputation record must be present after migration");
    assert_eq!(after.completed_contracts, 7);
    assert_eq!(after.total_rating, 31);
    assert_eq!(after.last_rating, 4);
}

/// A v1 record with all-zero fields migrates cleanly; the version marker is
/// written and the zero-value record is preserved.
#[test]
fn migration_v1_zero_values_migrates() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let zeroed = Reputation {
        completed_contracts: 0,
        total_rating: 0,
        last_rating: 0,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &zeroed);

    let migrated = env.as_contract(&escrow_addr, || {
        migrate_reputation_storage_impl(&env, &freelancer)
    });
    assert!(migrated);

    env.as_contract(&escrow_addr, || {
        assert_eq!(
            read_reputation_version(&env, &freelancer),
            REPUTATION_STORAGE_VERSION
        );
    });

    // Zero-value record must still be present after migration.
    let after = read_reputation_direct(&env, &escrow_addr, &freelancer);
    assert!(after.is_some());
    let after = after.unwrap();
    assert_eq!(after.completed_contracts, 0);
    assert_eq!(after.total_rating, 0);
    assert_eq!(after.last_rating, 0);
}

/// A legacy record with any negative field is rejected without rewriting the
/// record or sealing it with a current-version marker.
#[test]
fn migration_rejects_negative_reputation_fields_without_writes() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();

    for invalid in [
        Reputation {
            completed_contracts: -1,
            total_rating: 0,
            last_rating: 0,
        },
        Reputation {
            completed_contracts: 0,
            total_rating: -1,
            last_rating: 0,
        },
        Reputation {
            completed_contracts: 0,
            total_rating: 0,
            last_rating: -1,
        },
    ] {
        let freelancer = Address::generate(&env);
        write_v1_reputation(&env, &escrow_addr, &freelancer, &invalid);

        let migrated = env.as_contract(&escrow_addr, || {
            migrate_reputation_storage_impl(&env, &freelancer)
        });

        assert!(!migrated);
        assert_eq!(
            read_reputation_direct(&env, &escrow_addr, &freelancer),
            Some(invalid)
        );
        assert_eq!(read_version_direct(&env, &escrow_addr, &freelancer), None);
    }
}

/// Nonnegative fields remain valid at their representable upper boundary and
/// are preserved exactly by migration.
#[test]
fn migration_accepts_maximum_nonnegative_reputation_fields() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);
    let maximum = Reputation {
        completed_contracts: i128::MAX,
        total_rating: i128::MAX,
        last_rating: i128::MAX,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &maximum);

    let migrated = env.as_contract(&escrow_addr, || {
        migrate_reputation_storage_impl(&env, &freelancer)
    });

    assert!(migrated);
    assert_eq!(
        read_reputation_direct(&env, &escrow_addr, &freelancer),
        Some(maximum)
    );
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &freelancer),
        Some(REPUTATION_STORAGE_VERSION)
    );
}

/// Calling migration on a record that already has the current version marker
/// returns false without touching storage.
#[test]
fn migration_current_version_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let rep = Reputation {
        completed_contracts: 3,
        total_rating: 13,
        last_rating: 5,
    };
    // Write at v1, migrate to v2.
    write_v1_reputation(&env, &escrow_addr, &freelancer, &rep);
    let first = env.as_contract(&escrow_addr, || {
        migrate_reputation_storage_impl(&env, &freelancer)
    });
    assert!(first);

    // Second call must be a no-op.
    let second = env.as_contract(&escrow_addr, || {
        migrate_reputation_storage_impl(&env, &freelancer)
    });
    assert!(!second, "second migration on a v2 record must return false");

    // Data still intact after no-op.
    let after = read_reputation_direct(&env, &escrow_addr, &freelancer).unwrap();
    assert_eq!(after.completed_contracts, 3);
    assert_eq!(after.total_rating, 13);
    assert_eq!(after.last_rating, 5);
}

/// An address that has never had a reputation record written: migration returns
/// false and leaves storage completely untouched (no record, no version marker).
#[test]
fn migration_absent_record_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let unknown = Address::generate(&env);

    // Confirm no record exists before migration attempt.
    assert_eq!(
        read_reputation_direct(&env, &escrow_addr, &unknown),
        None,
        "no record should exist before migration"
    );

    let result = env.as_contract(&escrow_addr, || {
        migrate_reputation_storage_impl(&env, &unknown)
    });

    // Migration of an absent record must return false.
    assert!(!result, "migration of an absent record must return false");

    // Storage must remain completely untouched.
    assert_eq!(
        read_reputation_direct(&env, &escrow_addr, &unknown),
        None,
        "absent record must remain None after migration"
    );

    // Version marker must also remain absent.
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &unknown),
        None,
        "no version marker must be written for an absent record"
    );

    // Repeated calls stay no-op and still write nothing (retry safety).
    let retry = env.as_contract(&escrow_addr, || {
        migrate_reputation_storage_impl(&env, &unknown)
    });
    assert!(!retry, "retry on absent record must also return false");
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &unknown),
        None,
        "retry must not create a version marker"
    );
    env.as_contract(&escrow_addr, || {
        assert_eq!(
            read_reputation_version(&env, &unknown),
            1,
            "absent record must read as v1 legacy"
        );
    });
}

/// Multiple successive migration calls are idempotent: only the first
/// returns true; subsequent calls all return false.
#[test]
fn migrate_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let rep = Reputation {
        completed_contracts: 2,
        total_rating: 9,
        last_rating: 5,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &rep);

    env.as_contract(&escrow_addr, || {
        assert!(migrate_reputation_storage_impl(&env, &freelancer));
        assert!(!migrate_reputation_storage_impl(&env, &freelancer));
        assert!(!migrate_reputation_storage_impl(&env, &freelancer));
    });

    // Data still intact.
    let after = read_reputation_direct(&env, &escrow_addr, &freelancer).unwrap();
    assert_eq!(after.completed_contracts, 2);
    assert_eq!(after.total_rating, 9);
    assert_eq!(after.last_rating, 5);
    // Marker pinned to current version after idempotent retries.
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &freelancer),
        Some(REPUTATION_STORAGE_VERSION),
        "version marker must stay at current version after retries"
    );
}

// ── Getter read-only invariant ───────────────────────────────────────────────

/// `get_reputation` stays read-only for RPC simulation safety: it returns v1
/// data as-is without writing a version marker. Explicit
/// `migrate_reputation_storage` owns all state writes.
#[test]
fn get_reputation_is_read_only_does_not_migrate_v1() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let original = Reputation {
        completed_contracts: 5,
        total_rating: 22,
        last_rating: 4,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &original);

    // Confirm no version marker before the read.
    assert_eq!(read_version_direct(&env, &escrow_addr, &freelancer), None);

    // get_reputation must NOT mutate storage.
    let result = escrow_client.get_reputation(&freelancer);
    assert!(result.is_some(), "expected a reputation record");
    let rep = result.unwrap();
    assert_eq!(rep.completed_contracts, 5);
    assert_eq!(rep.total_rating, 22);
    assert_eq!(rep.last_rating, 4);

    // Version marker must still be absent after a read.
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &freelancer),
        None,
        "get_reputation must not write a version marker"
    );

    // Explicit migration still upgrades afterwards (retry-safe).
    assert!(escrow_client.migrate_reputation_storage(&freelancer));
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &freelancer),
        Some(REPUTATION_STORAGE_VERSION),
        "explicit migration must write the version marker"
    );
}

/// `get_reputation` returns `None` for an address that has never had reputation written.
#[test]
fn get_reputation_absent_returns_none() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let unknown = Address::generate(&env);

    assert!(escrow_client.get_reputation(&unknown).is_none());
}

// ── Public entrypoint ─────────────────────────────────────────────────────────

/// The public `migrate_reputation_storage` entrypoint returns `true` when it
/// upgrades a legacy v1 record.
#[test]
fn public_entrypoint_returns_true_on_migration() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let rep = Reputation {
        completed_contracts: 1,
        total_rating: 5,
        last_rating: 5,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &rep);

    let result = escrow_client.migrate_reputation_storage(&freelancer);
    assert!(
        result,
        "entrypoint must return true when migrating a v1 record"
    );

    // Record preserved.
    let after = escrow_client
        .get_reputation(&freelancer)
        .expect("record must exist after migration");
    assert_eq!(after.completed_contracts, 1);
    assert_eq!(after.total_rating, 5);
    assert_eq!(after.last_rating, 5);
}

/// The public `migrate_reputation_storage` entrypoint returns `false` when the
/// record is already at the current version.
#[test]
fn public_entrypoint_returns_false_on_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let rep = Reputation {
        completed_contracts: 2,
        total_rating: 8,
        last_rating: 4,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &rep);

    // First call migrates.
    assert!(escrow_client.migrate_reputation_storage(&freelancer));
    // Second call is a no-op.
    assert!(!escrow_client.migrate_reputation_storage(&freelancer));
}

/// After migration the version marker equals `REPUTATION_STORAGE_VERSION`.
#[test]
fn version_marker_written_correctly() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let rep = Reputation {
        completed_contracts: 10,
        total_rating: 45,
        last_rating: 5,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &rep);

    escrow_client.migrate_reputation_storage(&freelancer);

    assert_eq!(
        read_version_direct(&env, &escrow_addr, &freelancer),
        Some(REPUTATION_STORAGE_VERSION),
        "version marker must equal REPUTATION_STORAGE_VERSION after migration"
    );
}

/// Reputation written via `issue_reputation` after an explicit migration is
/// readable and the version marker remains current.
///
/// We set up contract state directly (bypassing `deposit_funds` which requires a
/// SAC token) because this test targets the migration + reputation storage
/// interaction, not the full escrow payment flow.
#[test]
fn issue_reputation_after_migration_readable() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);
    let client_addr = Address::generate(&env);

    // Seed a v1 reputation record simulating prior on-chain state.
    let old_rep = Reputation {
        completed_contracts: 1,
        total_rating: 3,
        last_rating: 3,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &old_rep);

    // Migrate explicitly via the public entrypoint.
    assert!(escrow_client.migrate_reputation_storage(&freelancer));

    // Verify version marker is now present.
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &freelancer),
        Some(REPUTATION_STORAGE_VERSION)
    );

    // Inject a completed contract and pending credit directly into storage so
    // issue_reputation can execute without needing a full SAC funding flow.
    let contract_id: u32 = env.as_contract(&escrow_addr, || {
        let cid: u32 = 9999;
        let contract = crate::Contract {
            client: client_addr.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: crate::ContractStatus::Completed,
            total_deposited: 1_000,
            funded_amount: 1_000,
            released_amount: 1_000,
            refunded_amount: 0,
            release_authorization: crate::ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(cid), &contract);
        env.storage().persistent().set(
            &DataKey::PendingReputationCredits(freelancer.clone()),
            &1_i128,
        );
        cid
    });

    // issue_reputation must succeed on a previously migrated record.
    assert!(escrow_client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));

    // The resulting record must combine the migrated history with the new issuance.
    let rep = escrow_client
        .get_reputation(&freelancer)
        .expect("reputation record must exist after issue_reputation");
    // completed_contracts: 1 (v1 seed) + 1 (new issuance) = 2
    assert_eq!(rep.completed_contracts, 2);
    assert_eq!(rep.last_rating, 5);
    // total_rating: 3 (v1 seed) + 5 (new issuance) = 8
    assert_eq!(rep.total_rating, 8);

    // Version marker must still equal the current version.
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &freelancer),
        Some(REPUTATION_STORAGE_VERSION)
    );
}

/// The public entrypoint is callable on an unknown address and returns `false`
/// without panicking (absent record is a no-op).
#[test]
fn public_entrypoint_unknown_address_does_not_panic() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let unknown = Address::generate(&env);

    // Must not panic and must return false (no record to migrate).
    let result = escrow_client.migrate_reputation_storage(&unknown);
    assert!(
        !result,
        "absent record must return false from public entrypoint"
    );
}

// ── Compatibility contracts ──────────────────────────────────────────────────

/// Legacy v1 records stay readable through every query path with identical
/// results, and reads never write markers (strict read-only).
#[test]
fn compat_v1_readable_via_all_query_paths() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let original = Reputation {
        completed_contracts: 5,
        total_rating: 22,
        last_rating: 4,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &original);
    push_to_index(&env, &escrow_addr, &freelancer);

    // get_reputation returns v1 as-is.
    let rep = escrow_client.get_reputation(&freelancer).expect("v1 must read");
    assert_eq!(rep.completed_contracts, 5);
    assert_eq!(rep.total_rating, 22);
    assert_eq!(rep.last_rating, 4);

    // get_average_rating works on v1: 22 * 10_000 / 5 = 44_000.
    assert_eq!(escrow_client.get_average_rating(&freelancer), Some(44_000));

    // get_reputations_page includes the v1 entry.
    let page = escrow_client.get_reputations_page(&0, &10);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().completed_contracts, 5);

    // Reads are side-effect free: still no marker.
    assert_eq!(read_version_direct(&env, &escrow_addr, &freelancer), None);
}

/// Mixed v1 + v2 records paginate identically regardless of version.
#[test]
fn compat_mixed_v1_v2_pagination() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let v1_addr = Address::generate(&env);
    let v2_addr = Address::generate(&env);

    write_v1_reputation(
        &env,
        &escrow_addr,
        &v1_addr,
        &Reputation {
            completed_contracts: 2,
            total_rating: 8,
            last_rating: 4,
        },
    );
    push_to_index(&env, &escrow_addr, &v1_addr);

    write_v1_reputation(
        &env,
        &escrow_addr,
        &v2_addr,
        &Reputation {
            completed_contracts: 3,
            total_rating: 15,
            last_rating: 5,
        },
    );
    push_to_index(&env, &escrow_addr, &v2_addr);
    assert!(escrow_client.migrate_reputation_storage(&v2_addr));

    let page = escrow_client.get_reputations_page(&0, &10);
    assert_eq!(page.len(), 2);
    // Both versions read identically; marker state differs only.
    assert_eq!(read_version_direct(&env, &escrow_addr, &v1_addr), None);
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &v2_addr),
        Some(REPUTATION_STORAGE_VERSION)
    );
}

/// Future markers (> CURRENT) are forward-compatible no-ops: no panic, no
/// downgrade, data intact, reads still work.
#[test]
fn compat_future_marker_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    let original = Reputation {
        completed_contracts: 4,
        total_rating: 18,
        last_rating: 5,
    };
    write_v1_reputation(&env, &escrow_addr, &freelancer, &original);
    write_version_direct(&env, &escrow_addr, &freelancer, 99);

    let migrated = env.as_contract(&escrow_addr, || {
        migrate_reputation_storage_impl(&env, &freelancer)
    });
    assert!(!migrated, "future version must be a no-op");
    assert!(!escrow_client.migrate_reputation_storage(&freelancer));

    // Data intact, marker untouched (no downgrade to v2).
    let after = read_reputation_direct(&env, &escrow_addr, &freelancer).unwrap();
    assert_eq!(after.completed_contracts, 4);
    assert_eq!(after.total_rating, 18);
    assert_eq!(read_version_direct(&env, &escrow_addr, &freelancer), Some(99));

    // Reads still serve the record.
    assert!(escrow_client.get_reputation(&freelancer).is_some());
}

/// Corrupted `0` marker heals forward to v2 when a record exists.
#[test]
fn compat_corrupted_zero_marker_heals() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    write_v1_reputation(
        &env,
        &escrow_addr,
        &freelancer,
        &Reputation {
            completed_contracts: 1,
            total_rating: 5,
            last_rating: 5,
        },
    );
    write_version_direct(&env, &escrow_addr, &freelancer, 0);

    assert!(escrow_client.migrate_reputation_storage(&freelancer));
    let after = read_reputation_direct(&env, &escrow_addr, &freelancer).unwrap();
    assert_eq!(after.completed_contracts, 1);
    assert_eq!(
        read_version_direct(&env, &escrow_addr, &freelancer),
        Some(REPUTATION_STORAGE_VERSION)
    );
}

/// Zero-contract boundary never traps: average is `None`, migration still works.
#[test]
fn compat_zero_contracts_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let freelancer = Address::generate(&env);

    write_v1_reputation(
        &env,
        &escrow_addr,
        &freelancer,
        &Reputation {
            completed_contracts: 0,
            total_rating: 0,
            last_rating: 0,
        },
    );

    assert_eq!(escrow_client.get_average_rating(&freelancer), None);
    assert!(escrow_client.migrate_reputation_storage(&freelancer));
    assert_eq!(escrow_client.get_average_rating(&freelancer), None);
}

/// Idempotency matrix via the public entrypoint: v1→true-then-false,
/// v2→false, absent→false with zero writes.
#[test]
fn compat_idempotency_matrix() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_client = register_client(&env);
    let escrow_addr = escrow_client.address.clone();
    let v1_addr = Address::generate(&env);
    let absent = Address::generate(&env);

    write_v1_reputation(
        &env,
        &escrow_addr,
        &v1_addr,
        &Reputation {
            completed_contracts: 1,
            total_rating: 4,
            last_rating: 4,
        },
    );

    assert!(escrow_client.migrate_reputation_storage(&v1_addr));
    assert!(!escrow_client.migrate_reputation_storage(&v1_addr));

    assert!(!escrow_client.migrate_reputation_storage(&absent));
    assert_eq!(read_version_direct(&env, &escrow_addr, &absent), None);
}
