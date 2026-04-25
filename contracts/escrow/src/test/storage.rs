use super::{default_milestones, generated_participants, register_client};
use crate::{ContractStatus, DataKey, EscrowError, EscrowRecord, MetaKey, StorageVersion, V1Key};
use soroban_sdk::{symbol_short, Address, Env};

#[test]
fn test_storage_version_defaults_to_v1() {
    let env = Env::default();
    let client = register_client(&env);

    let version = client.get_storage_version();
    assert_eq!(version, 1);
}

#[test]
fn test_migrate_storage_to_current_version_is_noop() {
    let env = Env::default();
    let client = register_client(&env);

    assert!(client.migrate_storage(&1));
    assert_eq!(client.get_storage_version(), 1);
}

#[test]
fn test_migrate_storage_rejects_unknown_target() {
    let env = Env::default();
    let client = register_client(&env);

    let result = client.try_migrate_storage(&2);
    assert_eq!(result, Err(Ok(EscrowError::UnsupportedMigrationTarget)));
}

#[test]
fn test_storage_layout_plan_namespaces() {
    let env = Env::default();
    let client = register_client(&env);

    let plan = client.storage_layout_plan();
    assert_eq!(plan.version, 1);
    assert_eq!(plan.meta_namespace, symbol_short!("meta_v1"));
    assert_eq!(plan.contracts_namespace, symbol_short!("escrow_v1"));
    assert_eq!(plan.reputation_namespace, symbol_short!("rep_v1"));
}

#[test]
fn test_migration_noop_preserves_stored_contract_data() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    assert!(client.migrate_storage(&1));

    let record = client.get_contract(&contract_id);
    assert_eq!(record.status, ContractStatus::Created);
    assert_eq!(record.milestone_count, 3);
    assert_eq!(record.client, client_addr);
    assert_eq!(record.freelancer, freelancer_addr);
}

#[test]
fn test_migration_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    // Create a contract
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    // Run migration multiple times
    assert!(client.migrate_storage(&1));
    assert!(client.migrate_storage(&1));
    assert!(client.migrate_storage(&1));

    // Verify data integrity after multiple migrations
    let record = client.get_contract(&contract_id);
    assert_eq!(record.status, ContractStatus::Created);
    assert_eq!(record.milestone_count, 3);
    assert_eq!(client.get_storage_version(), 1);
}

#[test]
fn test_migration_preserves_multiple_contracts() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    // Create multiple contracts
    let contract_id_1 =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    let contract_id_2 =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    let contract_id_3 =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    // Run migration
    assert!(client.migrate_storage(&1));

    // Verify all contracts are intact
    let record_1 = client.get_contract(&contract_id_1);
    let record_2 = client.get_contract(&contract_id_2);
    let record_3 = client.get_contract(&contract_id_3);

    assert_eq!(record_1.status, ContractStatus::Created);
    assert_eq!(record_2.status, ContractStatus::Created);
    assert_eq!(record_3.status, ContractStatus::Created);
}

#[test]
fn test_migration_preserves_funded_contract_state() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    // Create and fund a contract
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    let total_amount = 1000_i128 + 2000_i128 + 3000_i128;
    client.deposit_funds(&contract_id, &total_amount);

    // Run migration
    assert!(client.migrate_storage(&1));

    // Verify funded state is preserved
    let record = client.get_contract(&contract_id);
    assert_eq!(record.status, ContractStatus::Funded);
    assert_eq!(record.funded_amount, total_amount);
    assert_eq!(record.released_amount, 0);
}

#[test]
fn test_migration_validates_layout_before_execution() {
    let env = Env::default();
    let client = register_client(&env);

    // Migration should succeed because layout is valid
    assert!(client.migrate_storage(&1));

    // Verify version is still correct
    assert_eq!(client.get_storage_version(), 1);
}

#[test]
fn test_storage_version_initialized_on_first_access() {
    let env = Env::default();
    let contract_id = env.register_contract(None, crate::Escrow);
    let client = crate::EscrowClient::new(&env, &contract_id);

    // First access should initialize version
    let version = client.get_storage_version();
    assert_eq!(version, 1);

    // Verify it's persisted
    env.as_contract(&contract_id, || {
        let storage = env.storage().persistent();
        let version_key = DataKey::Meta(MetaKey::LayoutVersion);
        let stored_version: u32 = storage.get(&version_key).unwrap();
        assert_eq!(stored_version, StorageVersion::V1 as u32);
    });
}

#[test]
fn test_migration_rejects_unsupported_versions() {
    let env = Env::default();
    let client = register_client(&env);

    // Test various unsupported version numbers
    assert_eq!(
        client.try_migrate_storage(&0),
        Err(Ok(EscrowError::UnsupportedMigrationTarget))
    );
    assert_eq!(
        client.try_migrate_storage(&2),
        Err(Ok(EscrowError::UnsupportedMigrationTarget))
    );
    assert_eq!(
        client.try_migrate_storage(&999),
        Err(Ok(EscrowError::UnsupportedMigrationTarget))
    );
}

#[test]
fn test_contract_operations_work_after_migration() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    // Create contract before migration
    let contract_id_1 =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    // Run migration
    assert!(client.migrate_storage(&1));

    // Create contract after migration
    let contract_id_2 =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    // Both contracts should work
    let record_1 = client.get_contract(&contract_id_1);
    let record_2 = client.get_contract(&contract_id_2);

    assert_eq!(record_1.status, ContractStatus::Created);
    assert_eq!(record_2.status, ContractStatus::Created);
    assert_ne!(contract_id_1, contract_id_2);
}
