//! Covers the versioned migration path for `Contract` storage
//! (`migration::migrate_contract_storage`): legacy (schema v1) records must
//! upgrade transparently on read, an already-current record must be a
//! no-op, and no accounting data may be lost across the upgrade.

use super::{assert_contract_error, create_contract, register_client};
use crate::{Contract, ContractV1, DataKey, Error, CONTRACT_STORAGE_SCHEMA_VERSION};
use soroban_sdk::Env;

/// Overwrite a contract's storage with the pre-`reputation_issued` (schema
/// v1) layout and drop its version marker, simulating a record written by a
/// deployment that predates the migration.
fn downgrade_to_v1(env: &Env, escrow_addr: &soroban_sdk::Address, contract_id: u32) {
    env.as_contract(escrow_addr, || {
        let current: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .expect("contract must exist before it can be downgraded");

        let legacy = ContractV1 {
            client: current.client,
            freelancer: current.freelancer,
            arbiter: current.arbiter,
            status: current.status,
            total_deposited: current.total_deposited,
            funded_amount: current.funded_amount,
            released_amount: current.released_amount,
            refunded_amount: current.refunded_amount,
            release_authorization: current.release_authorization,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &legacy);
        env.storage()
            .persistent()
            .remove(&DataKey::ContractSchemaVersion(contract_id));
    });
}

fn read_schema_version(env: &Env, escrow_addr: &soroban_sdk::Address, contract_id: u32) -> u32 {
    env.as_contract(escrow_addr, || {
        env.storage()
            .persistent()
            .get(&DataKey::ContractSchemaVersion(contract_id))
            .unwrap_or(1)
    })
}

#[test]
fn new_contract_is_created_at_current_schema_version() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, id) = create_contract(&env, &client);

    assert_eq!(
        read_schema_version(&env, &client.address, id),
        CONTRACT_STORAGE_SCHEMA_VERSION
    );
}

#[test]
fn legacy_v1_contract_migrates_on_read_and_preserves_data() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, id) = create_contract(&env, &client);

    let before = client.get_contract(&id);
    downgrade_to_v1(&env, &client.address, id);
    assert_eq!(
        read_schema_version(&env, &client.address, id),
        1,
        "downgrade helper must clear the version marker"
    );

    let migrated = client.get_contract(&id);

    // All fields present on the legacy layout must survive the upgrade untouched.
    assert_eq!(migrated.client, client_addr);
    assert_eq!(migrated.freelancer, freelancer_addr);
    assert_eq!(migrated.arbiter, before.arbiter);
    assert_eq!(migrated.status, before.status);
    assert_eq!(migrated.total_deposited, before.total_deposited);
    assert_eq!(migrated.funded_amount, before.funded_amount);
    assert_eq!(migrated.released_amount, before.released_amount);
    assert_eq!(migrated.refunded_amount, before.refunded_amount);
    assert_eq!(migrated.release_authorization, before.release_authorization);
    // The field that didn't exist on v1 gets a safe, explicit default.
    assert_eq!(migrated.reputation_issued, false);

    // The record is rewritten in place at the current version so subsequent
    // reads take the fast path instead of re-migrating.
    assert_eq!(
        read_schema_version(&env, &client.address, id),
        CONTRACT_STORAGE_SCHEMA_VERSION
    );
}

#[test]
fn migration_preserves_data_after_deposits_and_partial_progress() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);

    let total = super::total_milestone_amount();
    client.deposit_funds(&id, &client_addr, &total);
    client.approve_milestone_release(&id, &client_addr, &0u32);
    client.release_milestone(&id, &client_addr, &0u32);

    let before = client.get_contract(&id);
    assert!(before.released_amount > 0, "fixture must have progressed");

    downgrade_to_v1(&env, &client.address, id);
    let migrated = client.get_contract(&id);

    assert_eq!(migrated.status, before.status);
    assert_eq!(migrated.total_deposited, before.total_deposited);
    assert_eq!(migrated.funded_amount, before.funded_amount);
    assert_eq!(migrated.released_amount, before.released_amount);
    assert_eq!(migrated.refunded_amount, before.refunded_amount);
    assert_eq!(migrated.reputation_issued, before.reputation_issued);
}

#[test]
fn read_at_current_version_is_a_no_op() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, id) = create_contract(&env, &client);

    let first = client.get_contract(&id);
    assert_eq!(
        read_schema_version(&env, &client.address, id),
        CONTRACT_STORAGE_SCHEMA_VERSION
    );

    let second = client.get_contract(&id);
    assert_eq!(first, second);
    assert_eq!(
        read_schema_version(&env, &client.address, id),
        CONTRACT_STORAGE_SCHEMA_VERSION,
        "reading an already-current record must not change its version marker"
    );
}

#[test]
fn get_contract_unknown_id_still_reports_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    assert_contract_error(client.try_get_contract(&999u32), Error::ContractNotFound);
}
