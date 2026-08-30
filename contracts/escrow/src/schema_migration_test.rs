#![cfg(test)]

use crate::types::{DataKey, Error};
use crate::{Escrow, EscrowClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal, Symbol,
};

#[test]
fn test_get_schema_version_default_returns_initial_version() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Initial deployment should report version 1
    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_escrow_storage_from_v1_to_v2_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    assert_eq!(client.get_schema_version(), 1);

    // Perform migration to version 2
    let new_ver = client.migrate_escrow_storage(&admin, &2);
    assert_eq!(new_ver, 2);
    assert_eq!(client.get_schema_version(), 2);

    // Verify migration event emission
    let events = env.events().all();
    let last_event = events.last().expect("Migration event expected");
    assert_eq!(last_event.0, contract_id);
}

#[test]
fn test_migrate_escrow_storage_idempotent_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // First migration to v2
    assert_eq!(client.migrate_escrow_storage(&admin, &2), 2);
    assert_eq!(client.get_schema_version(), 2);

    // Repeated migration to v2 is an idempotent no-op returning 2
    assert_eq!(client.migrate_escrow_storage(&admin, &2), 2);
    assert_eq!(client.get_schema_version(), 2);
}

#[test]
fn test_migrate_escrow_storage_rejects_downgrade() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Migrate to v2
    client.migrate_escrow_storage(&admin, &2);

    // Attempt downgrade to v1
    let result = client.try_migrate_escrow_storage(&admin, &1);
    assert_eq!(result, Err(Ok(Error::InvalidMigrationVersion)));
    assert_eq!(client.get_schema_version(), 2);
}

#[test]
fn test_migrate_escrow_storage_rejects_unsupported_future_version() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Target version 99 exceeds CURRENT_STORAGE_SCHEMA_VERSION (2)
    let result = client.try_migrate_escrow_storage(&admin, &99);
    assert_eq!(result, Err(Ok(Error::InvalidMigrationVersion)));
    assert_eq!(client.get_schema_version(), 1);
}

#[test]
fn test_migrate_escrow_storage_non_admin_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    client.initialize(&admin);

    // Attacker tries to trigger migration
    let result = client.try_migrate_escrow_storage(&attacker, &2);
    assert!(result.is_err());
    assert_eq!(client.get_schema_version(), 1);
}
