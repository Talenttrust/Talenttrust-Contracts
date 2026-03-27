use super::{default_milestones, generated_participants, register_client};
use crate::{ContractStatus, EscrowError};
use soroban_sdk::{symbol_short, Env};

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
