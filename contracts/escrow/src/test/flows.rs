use super::{
    complete_contract, create_contract, register_client, total_milestone_amount, world_symbol,
};
use crate::{ContractStatus, EscrowError};
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn hello_round_trips_symbol() {
    let env = Env::default();
    let client = register_client(&env);

    assert_eq!(client.hello(&world_symbol()), world_symbol());
}

#[test]
fn create_contract_stores_expected_state() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    let record = client.get_contract(&contract_id);
    assert_eq!(contract_id, 1);
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
fn full_flow_completes_and_issues_reputation() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (_client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    let post_release = client.get_contract(&contract_id);
    assert_eq!(post_release.status, ContractStatus::Completed);
    assert_eq!(post_release.released_amount, total_milestone_amount());
    assert_eq!(post_release.released_milestones, 3);

    assert!(client.issue_reputation(&contract_id, &5));

    let reputation = client
        .get_reputation(&freelancer_addr)
        .expect("reputation should exist after issuance");
    assert_eq!(reputation.total_rating, 5);
    assert_eq!(reputation.ratings_count, 1);
    assert_eq!(reputation.completed_contracts, 1);

    let post_rating = client.get_contract(&contract_id);
    assert!(post_rating.reputation_issued);
}

#[test]
fn contract_ids_increment_from_persistent_counter() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);

    let (_, _, first_id) = create_contract(&env, &client);
    let (_, _, second_id) = create_contract(&env, &client);

    assert_eq!(first_id, 1);
    assert_eq!(second_id, 2);
}

#[test]
fn reputation_aggregates_across_completed_contracts() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);

    let (_, freelancer_addr, first_id) = complete_contract(&env, &client);
    assert!(client.issue_reputation(&first_id, &5));

    let client_addr = soroban_sdk::Address::generate(&env);
    let second_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &super::default_milestones(&env),
    );
    assert!(client.deposit_funds(&second_id, &total_milestone_amount()));
    assert!(client.release_milestone(&second_id, &0));
    assert!(client.release_milestone(&second_id, &1));
    assert!(client.release_milestone(&second_id, &2));
    assert!(client.issue_reputation(&second_id, &4));

    let reputation = client
        .get_reputation(&freelancer_addr)
        .expect("reputation should exist after two completed contracts");
    assert_eq!(reputation.total_rating, 9);
    assert_eq!(reputation.ratings_count, 2);
    assert_eq!(reputation.completed_contracts, 2);
}

#[test]
fn get_contract_for_missing_id_fails() {
    let env = Env::default();
    let client = register_client(&env);

    let result = client.try_get_contract(&999);
    super::assert_contract_error(result, EscrowError::ContractNotFound);
}
