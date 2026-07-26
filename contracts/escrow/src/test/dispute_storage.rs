#![cfg(test)]

//! Tests for versioned dispute-storage migration (issue #1017).
//!
//! Covers:
//! - v0 → v1 migrate-on-read with field preservation
//! - current-version no-op
//! - legacy status-only disputed contracts synthesizing v1 metadata
//! - raise/resolve wiring through the versioned path

use crate::dispute::{
    get_dispute_storage_version, load_dispute_metadata, migrate_dispute_metadata_v0_to_v1,
    store_dispute_metadata,
};
use crate::{
    types::DataKey, Contract, ContractStatus, DisputeMetadata, DisputeMetadataV0,
    DisputeResolution, EscrowClient, EscrowError, ReleaseAuthorization, DISPUTE_STORAGE_VERSION,
};
use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env};

use super::{assert_contract_error, register_client, total_milestone_amount};

fn create_funded_with_arbiter(
    env: &Env,
    client: &EscrowClient,
) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter = Address::generate(env);
    let milestones = vec![env, 100_i128, 200_i128, 300_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&id, &client_addr, &600_i128));
    (client_addr, freelancer_addr, arbiter, id)
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
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer, _arbiter, id) = create_funded_with_arbiter(&env, &client);

    // Mark contract disputed (legacy path) and inject a v0 metadata record.
    let raiser = client_addr.clone();
    let hash = BytesN::from_array(&env, &[9u8; 32]);
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
            get_dispute_storage_version(&env, id),
            DISPUTE_STORAGE_VERSION
        );
    });
}

/// Reading an already-current record is a no-op (version and payload unchanged).
#[test]
fn current_version_load_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer, _arbiter, id) = create_funded_with_arbiter(&env, &client);

    let hash = BytesN::from_array(&env, &[3u8; 32]);
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
        store_dispute_metadata(&env, id, &original);
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
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer, _arbiter, id) = create_funded_with_arbiter(&env, &client);

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
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer, arbiter, id) = create_funded_with_arbiter(&env, &client);

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
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer, _arbiter, id) = create_funded_with_arbiter(&env, &client);

    env.as_contract(&client.address, || {
        let key = DataKey::Contract(id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = ContractStatus::Disputed;
        env.storage().persistent().set(&key, &contract);

        let meta = DisputeMetadata {
            schema_version: DISPUTE_STORAGE_VERSION,
            raised_by: client_addr.clone(),
            reason_hash: BytesN::from_array(&env, &[0u8; 32]),
            raised_at: 1,
        };
        env.storage().persistent().set(&DataKey::Dispute(id), &meta);
        env.storage().persistent().set(
            &DataKey::DisputeStorageVersion(id),
            &(DISPUTE_STORAGE_VERSION + 1),
        );
    });

    assert_contract_error(
        client.try_get_dispute(&id),
        EscrowError::UnsupportedDisputeStorageVersion,
    );
}

/// Direct helper coverage: load after store_dispute_metadata is a no-op path.
#[test]
fn load_dispute_metadata_helper_noop_for_current() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, _, id) = create_funded_with_arbiter(&env, &client);

    env.as_contract(&client.address, || {
        let meta = DisputeMetadata {
            schema_version: DISPUTE_STORAGE_VERSION,
            raised_by: client_addr.clone(),
            reason_hash: BytesN::from_array(&env, &[1u8; 32]),
            raised_at: 7,
        };
        store_dispute_metadata(&env, id, &meta);
        let loaded = load_dispute_metadata(&env, id);
        assert_eq!(loaded.raised_at, 7);
        assert_eq!(loaded.schema_version, DISPUTE_STORAGE_VERSION);
    });
}

// Silence unused import warning for total_milestone_amount when not referenced.
#[allow(dead_code)]
fn _keep_helper() -> i128 {
    total_milestone_amount()
}
