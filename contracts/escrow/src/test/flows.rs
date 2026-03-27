use super::{
    default_milestones, generated_participants, register_client, total_milestone_amount,
    world_symbol,
};
use crate::ContractStatus;
use soroban_sdk::Env;

#[test]
fn test_hello() {
    let env = Env::default();
    let client = register_client(&env);

    let result = client.hello(&world_symbol());
    assert_eq!(result, world_symbol());
}

#[test]
fn test_create_contract_initializes_storage_and_state() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    assert_eq!(contract_id, 1);
    assert_eq!(client.get_storage_version(), 1);

    let record = client.get_contract(&contract_id);
    assert_eq!(record.client, client_addr);
    assert_eq!(record.freelancer, freelancer_addr);
    assert_eq!(record.milestone_count, 3);
    assert_eq!(record.total_amount, total_milestone_amount());
    assert_eq!(record.funded_amount, 0);
    assert_eq!(record.released_amount, 0);
    assert_eq!(record.released_milestones, 0);
    assert_eq!(record.status, ContractStatus::Created);
    assert!(!record.reputation_issued);
}

#[test]
fn test_full_flow_completes_and_issues_reputation() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    assert!(client.deposit_funds(&contract_id, &total_milestone_amount()));

    assert!(client.release_milestone(&contract_id, &0));
    assert!(client.release_milestone(&contract_id, &1));
    assert!(client.release_milestone(&contract_id, &2));

    let post_release = client.get_contract(&contract_id);
    assert_eq!(post_release.status, ContractStatus::Completed);
    assert_eq!(post_release.released_amount, total_milestone_amount());
    assert_eq!(post_release.released_milestones, 3);

    assert!(client.issue_reputation(&contract_id, &5));
    let reputation = client.get_reputation(&freelancer_addr);
    assert_eq!(reputation.total_rating, 5);
    assert_eq!(reputation.ratings_count, 1);
}

#[test]
fn test_contract_ids_increment() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let first_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    let second_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    assert_eq!(first_id, 1);
    assert_eq!(second_id, 2);
}

#[test]
fn test_layout_plan_is_stable() {
    let env = Env::default();
    let client = register_client(&env);
    let plan = client.storage_layout_plan();

    assert_eq!(plan.version, 1);
    assert_eq!(plan.meta_namespace, soroban_sdk::symbol_short!("meta_v1"));
    assert_eq!(
        plan.contracts_namespace,
        soroban_sdk::symbol_short!("escrow_v1")
    );
    assert_eq!(
        plan.reputation_namespace,
        soroban_sdk::symbol_short!("rep_v1")
    );
}
