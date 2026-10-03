#![cfg(test)]

//! Tests for versioned dispute-storage migration (issue #1017).
//!
//! Covers:
//! - v0 → v1 migrate-on-read with field preservation
//! - current-version no-op
//! - legacy status-only disputed contracts synthesizing v1 metadata
//! - raise/resolve wiring through the versioned path
//! - validation boundaries: invalid, duplicate, and boundary-case inputs

use crate::dispute::{
    get_dispute_storage_version, load_dispute_metadata, migrate_dispute_metadata_v0_to_v1,
    store_dispute_metadata,
};
use crate::{
    types::DataKey, Contract, ContractStatus, DisputeMetadata, DisputeMetadataV0,
    DisputeResolution, EscrowError, DISPUTE_STORAGE_VERSION,
};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

use super::{assert_contract_error, EscrowFixture};

fn funded_fixture_with_arbiter() -> EscrowFixture {
    let mut builder = EscrowFixture::builder();
    let client = Address::generate(builder.env());
    let freelancer = Address::generate(builder.env());
    let arbiter = Address::generate(builder.env());
    builder
        .with_participants(client, freelancer, Some(arbiter))
        .funded()
        .build()
}

/// Pure helper: v0 → v1 copies all fields and stamps the current schema version.
#[test]
fn migrate_v0_to_v1_preserves_fields() {
    let env = Env::default();
    let raiser = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[7u8; 32]);
    let v0 = DisputeMetadataV0 {
        raised_by: raiser.clone(),
        reason_hash: hash.clone(),
        raised_at: 42,
    };

    let v1 = migrate_dispute_metadata_v0_to_v1(v0);
    assert_eq!(v1.schema_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(v1.raised_by, raiser);
    assert_eq!(v1.reason_hash, hash);
    assert_eq!(v1.raised_at, 42);
}

/// Inject a v0 record and confirm load migrates + rewrites as v1 with data preserved.
#[test]
fn old_version_migrates_on_read_and_preserves_data() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    // Mark contract disputed (legacy path) and inject a v0 metadata record.
    let raiser = client_addr.clone();
    let hash = BytesN::from_array(env, &[9u8; 32]);
    let raised_at = 99u64;
    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);

        let v0 = DisputeMetadataV0 {
            raised_by: raiser.clone(),
            reason_hash: hash.clone(),
            raised_at,
        };
        env.storage().persistent().set(&DataKey::Dispute(id), &v0);
        // Explicit legacy marker (missing would also be treated as 0).
        env.storage()
            .persistent()
            .set(&DataKey::DisputeStorageVersion(id), &0u32);
    });

    assert_eq!(client.get_dispute_storage_version(&id), 0);

    let migrated: DisputeMetadata = client.get_dispute(&id);
    assert_eq!(migrated.schema_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(migrated.raised_by, raiser);
    assert_eq!(migrated.reason_hash, hash);
    assert_eq!(migrated.raised_at, raised_at);

    // Rewrite persisted the current version marker and v1 payload.
    assert_eq!(
        client.get_dispute_storage_version(&id),
        DISPUTE_STORAGE_VERSION
    );
    env.as_contract(&client.address, || {
        let stored: DisputeMetadata = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(id))
            .unwrap();
        assert_eq!(stored, migrated);
        assert_eq!(
            get_dispute_storage_version(env, id),
            DISPUTE_STORAGE_VERSION
        );
    });
}

/// Reading an already-current record is a no-op (version and payload unchanged).
#[test]
fn current_version_load_is_noop() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    let hash = BytesN::from_array(env, &[3u8; 32]);
    let original = DisputeMetadata {
        schema_version: DISPUTE_STORAGE_VERSION,
        raised_by: client_addr.clone(),
        reason_hash: hash.clone(),
        raised_at: 123,
    };

    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);
        store_dispute_metadata(env, id, &original);
    });

    let before_version = client.get_dispute_storage_version(&id);
    let loaded = client.get_dispute(&id);
    let after_version = client.get_dispute_storage_version(&id);

    assert_eq!(before_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(after_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(loaded.schema_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(loaded.raised_by, client_addr);
    assert_eq!(loaded.reason_hash, hash);
    assert_eq!(loaded.raised_at, 123);
}

/// Status-only disputed contracts (no metadata key) synthesize a v1 record on read.
#[test]
fn legacy_status_only_dispute_synthesizes_v1_on_read() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);
        // Intentionally no Dispute / DisputeStorageVersion keys.
    });

    assert_eq!(client.get_dispute_storage_version(&id), 0);
    let meta = client.get_dispute(&id);
    assert_eq!(meta.schema_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(meta.raised_by, client_addr);
    assert_eq!(meta.raised_at, 0);
    assert_eq!(
        client.get_dispute_storage_version(&id),
        DISPUTE_STORAGE_VERSION
    );
}

/// raise_dispute writes current-version metadata; resolve clears it.
#[test]
fn raise_persists_current_version_and_resolve_clears_metadata() {
    let fixture = funded_fixture_with_arbiter();
    let arbiter = fixture.arbiter.clone().expect("arbiter configured");
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    assert!(client.raise_dispute(&id, &client_addr));
    assert_eq!(
        client.get_dispute_storage_version(&id),
        DISPUTE_STORAGE_VERSION
    );

    let meta = client.get_dispute(&id);
    assert_eq!(meta.schema_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(meta.raised_by, client_addr);

    assert!(client.resolve_dispute(&id, &arbiter, &DisputeResolution::FullRefund));
    assert_eq!(client.get_dispute_storage_version(&id), 0);
    assert_contract_error(client.try_get_dispute(&id), EscrowError::DisputeNotFound);
}

/// Unsupported future versions fail closed.
#[test]
fn unsupported_future_version_is_rejected() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);

        let meta = DisputeMetadata {
            schema_version: DISPUTE_STORAGE_VERSION,
            raised_by: client_addr.clone(),
            reason_hash: BytesN::from_array(env, &[0u8; 32]),
            raised_at: 1,
        };
        env.storage().persistent().set(&DataKey::Dispute(id), &meta);
        env.storage().persistent().set(
            &DataKey::DisputeStorageVersion(id),
            &(DISPUTE_STORAGE_VERSION + 1),
        );
    });

    assert_contract_error(client.try_get_dispute(&id), EscrowError::InvalidState);
}

/// Direct helper coverage: load after store_dispute_metadata is a no-op path.
#[test]
fn load_dispute_metadata_helper_noop_for_current() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    env.as_contract(&client.address, || {
        let meta = DisputeMetadata {
            schema_version: DISPUTE_STORAGE_VERSION,
            raised_by: client_addr.clone(),
            reason_hash: BytesN::from_array(env, &[1u8; 32]),
            raised_at: 7,
        };
        store_dispute_metadata(env, id, &meta);
        let loaded = load_dispute_metadata(env, id);
        assert_eq!(loaded.raised_at, 7);
        assert_eq!(loaded.schema_version, DISPUTE_STORAGE_VERSION);
    });
}

/// Boundary: raised_at = 0 is a valid timestamp and must round-trip unchanged.
#[test]
fn boundary_raised_at_zero_round_trips() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);

        let meta = DisputeMetadata {
            schema_version: DISPUTE_STORAGE_VERSION,
            raised_by: client_addr.clone(),
            reason_hash: BytesN::from_array(env, &[0u8; 32]),
            raised_at: 0,
        };
        store_dispute_metadata(env, id, &meta);
    });

    let loaded = client.get_dispute(&id);
    assert_eq!(loaded.raised_at, 0);
    assert_eq!(loaded.raised_by, client_addr);
    assert_eq!(loaded.schema_version, DISPUTE_STORAGE_VERSION);
}

/// Boundary: raised_at = u64::MAX must round-trip without truncation or overflow.
#[test]
fn boundary_raised_at_max_round_trips() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);

        let meta = DisputeMetadata {
            schema_version: DISPUTE_STORAGE_VERSION,
            raised_by: client_addr.clone(),
            reason_hash: BytesN::from_array(env, &[0xffu8; 32]),
            raised_at: u64::MAX,
        };
        store_dispute_metadata(env, id, &meta);
    });

    let loaded = client.get_dispute(&id);
    assert_eq!(loaded.raised_at, u64::MAX);
    assert_eq!(loaded.raised_by, client_addr);
}

/// Boundary: all-zero reason hash is a valid (non-empty) 32-byte value.
#[test]
fn boundary_zero_reason_hash_is_valid() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    let zero_hash = BytesN::from_array(env, &[0u8; 32]);
    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);

        let meta = DisputeMetadata {
            schema_version: DISPUTE_STORAGE_VERSION,
            raised_by: client_addr.clone(),
            reason_hash: zero_hash.clone(),
            raised_at: 1,
        };
        store_dispute_metadata(env, id, &meta);
    });

    let loaded = client.get_dispute(&id);
    assert_eq!(loaded.reason_hash, zero_hash);
}

/// Rejection: reading dispute metadata for a non-existent contract id fails closed.
#[test]
fn unknown_contract_id_is_rejected() {
    let fixture = funded_fixture_with_arbiter();
    let client = fixture.escrow();
    let unknown_id = fixture.escrow_id + 1;

    assert_contract_error(
        client.try_get_dispute(&unknown_id),
        EscrowError::DisputeNotFound,
    );
}

/// Rejection: reading dispute metadata when the contract is not disputed fails closed.
#[test]
fn non_disputed_contract_get_dispute_is_rejected() {
    let fixture = funded_fixture_with_arbiter();
    let client = fixture.escrow();
    let id = fixture.escrow_id;

    // Fixture is funded but not disputed.
    assert_contract_error(client.try_get_dispute(&id), EscrowError::DisputeNotFound);
}

/// Rejection: duplicate raise_dispute on an already-disputed contract fails closed
/// and preserves the original metadata (no silent overwrite / data loss).
#[test]
fn duplicate_raise_dispute_is_rejected_and_preserves_metadata() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    assert!(client.raise_dispute(&id, &client_addr));
    let first = client.get_dispute(&id);

    // Second raise must be rejected; state must remain unchanged.
    let second = client.try_raise_dispute(&id, &client_addr);
    assert!(second.is_err(), "duplicate raise_dispute must be rejected");

    let after = client.get_dispute(&id);
    assert_eq!(after.raised_by, first.raised_by);
    assert_eq!(after.reason_hash, first.reason_hash);
    assert_eq!(after.raised_at, first.raised_at);
    assert_eq!(after.schema_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(
        client.get_dispute_storage_version(&id),
        DISPUTE_STORAGE_VERSION
    );

    // Sanity: env still usable and storage intact.
    env.as_contract(&client.address, || {
        let stored: DisputeMetadata = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(id))
            .unwrap();
        assert_eq!(stored, after);
    });
}

/// Rejection: resolve_dispute on a non-disputed contract fails closed.
#[test]
fn resolve_non_disputed_contract_is_rejected() {
    let fixture = funded_fixture_with_arbiter();
    let arbiter = fixture.arbiter.clone().expect("arbiter configured");
    let client = fixture.escrow();
    let id = fixture.escrow_id;

    assert_contract_error(
        client.try_resolve_dispute(&id, &arbiter, &DisputeResolution::FullRefund),
        EscrowError::DisputeNotFound,
    );
}

/// Rejection: resolve_dispute by a non-arbiter caller fails closed and leaves
/// dispute metadata intact (authorization invariant preserved).
#[test]
fn resolve_by_non_arbiter_is_rejected_and_preserves_metadata() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    assert!(client.raise_dispute(&id, &client_addr));
    let before = client.get_dispute(&id);

    let attacker = Address::generate(env);
    let result = client.try_resolve_dispute(&id, &attacker, &DisputeResolution::FullRefund);
    assert!(result.is_err(), "non-arbiter resolve must be rejected");

    // Metadata and version must be untouched.
    let after = client.get_dispute(&id);
    assert_eq!(after, before);
    assert_eq!(
        client.get_dispute_storage_version(&id),
        DISPUTE_STORAGE_VERSION
    );
}

/// Regression: v0 migration with raised_at = 0 preserves the zero timestamp
/// (guards against accidental `unwrap_or(default_nonzero)` style bugs).
#[test]
fn migrate_v0_with_zero_raised_at_preserves_zero() {
    let env = Env::default();
    let raiser = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    let v0 = DisputeMetadataV0 {
        raised_by: raiser.clone(),
        reason_hash: hash.clone(),
        raised_at: 0,
    };

    let v1 = migrate_dispute_metadata_v0_to_v1(v0);
    assert_eq!(v1.schema_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(v1.raised_at, 0);
    assert_eq!(v1.raised_by, raiser);
    assert_eq!(v1.reason_hash, hash);
}

/// Regression: v0 migration with raised_at = u64::MAX preserves the max value.
#[test]
fn migrate_v0_with_max_raised_at_preserves_max() {
    let env = Env::default();
    let raiser = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0xaau8; 32]);
    let v0 = DisputeMetadataV0 {
        raised_by: raiser.clone(),
        reason_hash: hash.clone(),
        raised_at: u64::MAX,
    };

    let v1 = migrate_dispute_metadata_v0_to_v1(v0);
    assert_eq!(v1.schema_version, DISPUTE_STORAGE_VERSION);
    assert_eq!(v1.raised_at, u64::MAX);
    assert_eq!(v1.raised_by, raiser);
    assert_eq!(v1.reason_hash, hash);
}

/// Rejection: a v0 payload paired with a future version marker must fail closed
/// rather than silently migrating or overwriting unknown data.
#[test]
fn future_version_with_v0_payload_is_rejected() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let client_addr = fixture.client.clone();
    let id = fixture.escrow_id;

    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);

        let v0 = DisputeMetadataV0 {
            raised_by: client_addr.clone(),
            reason_hash: BytesN::from_array(env, &[1u8; 32]),
            raised_at: 5,
        };
        env.storage().persistent().set(&DataKey::Dispute(id), &v0);
        env.storage().persistent().set(
            &DataKey::DisputeStorageVersion(id),
            &(DISPUTE_STORAGE_VERSION + 1),
        );
    });

    assert_contract_error(client.try_get_dispute(&id), EscrowError::InvalidState);
}

/// A current payload paired with an older marker is corrupted state, not a
/// valid v0 record; loading must fail closed instead of silently trusting one.
#[test]
fn current_payload_with_legacy_version_marker_is_rejected() {
    let fixture = funded_fixture_with_arbiter();
    let env = &fixture.env;
    let client = fixture.escrow();
    let id = fixture.escrow_id;
    let metadata = DisputeMetadata {
        schema_version: DISPUTE_STORAGE_VERSION,
        raised_by: fixture.client.clone(),
        reason_hash: BytesN::from_array(env, &[4u8; 32]),
        raised_at: 7,
    };

    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(id), &metadata);
        env.storage()
            .persistent()
            .set(&DataKey::DisputeStorageVersion(id), &0u32);
    });

    assert_contract_error(client.try_get_dispute(&id), EscrowError::InvalidState);
}
