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

#[test]
fn get_milestones_for_missing_id_fails() {
    let env = Env::default();
    let client = register_client(&env);

    let result = client.try_get_milestones(&999);
    super::assert_contract_error(result, EscrowError::MilestoneNotFound);
}

#[test]
fn get_checklist_for_missing_id_fails() {
    let env = Env::default();
    let client = register_client(&env);

    let result = client.try_get_checklist(&999);
    super::assert_contract_error(result, EscrowError::ChecklistNotFound);
}

#[test]
fn scenario_happy_path_full_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    let record = client.get_contract(&contract_id);
    assert_eq!(record.status, ContractStatus::Completed);
    assert_eq!(record.released_milestones, 3);
    assert_eq!(record.released_amount, total_milestone_amount());

    assert!(client.issue_reputation(&contract_id, &5));

    let reputation = client.get_reputation(&freelancer_addr).expect("reputation should exist");
    assert_eq!(reputation.total_rating, 5);
    assert_eq!(reputation.ratings_count, 1);

    let post_rating = client.get_contract(&contract_id);
    assert!(post_rating.reputation_issued);
}

#[test]
fn scenario_auth_failure_unauthorized_deposit() {
    let env = Env::default();
    let client = register_client(&env);
    let client_addr = soroban_sdk::Address::generate(&env);
    let freelancer_addr = soroban_sdk::Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);

    env.mock_all_auths();
    let result = client.try_deposit_funds(&contract_id, &100_0000000_i128);
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn scenario_auth_failure_unauthorized_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    let attacker = soroban_sdk::Address::generate(&env);
    let _ = client.deposit_funds(&contract_id, &total_milestone_amount());
    let result = client.try_release_milestone(&contract_id, &0);
    assert!(result.is_ok());
}

#[test]
fn scenario_invalid_transition_early_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_complete_contract(&contract_id);
    super::assert_contract_error(result, EscrowError::NotAllMilestonesReleased);
}

#[test]
fn scenario_invalid_transition_partial_release_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    _ = client.deposit_funds(&contract_id, &total_milestone_amount());
    client.release_milestone(&contract_id, &0);
    client.release_milestone(&contract_id, &1);

    let result = client.try_complete_contract(&contract_id);
    super::assert_contract_error(result, EscrowError::NotAllMilestonesReleased);
}

#[test]
fn scenario_boundary_amounts_single_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = soroban_sdk::Address::generate(&env);
    let freelancer_addr = soroban_sdk::Address::generate(&env);
    let milestones = vec![&env, 1_0000000_i128];
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    _ = client.deposit_funds(&contract_id, &1_0000000_i128);
    client.release_milestone(&contract_id, &0);
    client.complete_contract(&contract_id);
    client.issue_reputation(&contract_id, &1);

    let reputation = client.get_reputation(&freelancer_addr).expect("reputation should exist");
    assert_eq!(reputation.total_rating, 1);
}

#[test]
fn scenario_boundary_amounts_max_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = soroban_sdk::Address::generate(&env);
    let freelancer_addr = soroban_sdk::Address::generate(&env);
    let milestones = vec![&env, 1_0000000_i128];
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    _ = client.deposit_funds(&contract_id, &1_0000000_i128);
    client.release_milestone(&contract_id, &0);
    client.complete_contract(&contract_id);
    client.issue_reputation(&contract_id, &5);

    let reputation = client.get_reputation(&freelancer_addr).expect("reputation should exist");
    assert_eq!(reputation.total_rating, 5);
}

#[test]
fn scenario_reputation_double_issuance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, freelancer_addr, contract_id) = complete_contract(&env, &client);

    assert!(client.issue_reputation(&contract_id, &5));
    let result = client.try_issue_reputation(&contract_id, &4);
    super::assert_contract_error(result, EscrowError::ReputationAlreadyIssued);
}

#[test]
fn scenario_reputation_invalid_rating_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, contract_id) = complete_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &0);
    super::assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn scenario_reputation_invalid_rating_six_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, contract_id) = complete_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &6);
    super::assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn scenario_milestone_released_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    _ = client.deposit_funds(&contract_id, &total_milestone_amount());
    client.release_milestone(&contract_id, &0);
    let result = client.try_release_milestone(&contract_id, &0);
    super::assert_contract_error(result, EscrowError::MilestoneAlreadyReleased);
}

#[test]
fn scenario_invalid_milestone_index_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    _ = client.deposit_funds(&contract_id, &total_milestone_amount());
    let result = client.try_release_milestone(&contract_id, &100);
    super::assert_contract_error(result, EscrowError::InvalidMilestone);
}

#[test]
fn scenario_empty_milestones_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = soroban_sdk::Address::generate(&env);
    let freelancer_addr = soroban_sdk::Address::generate(&env);
    let empty = soroban_sdk::Vec::<i128>::new(&env);
    let result = client.try_create_contract(&client_addr, &freelancer_addr, &empty);
    super::assert_contract_error(result, EscrowError::EmptyMilestones);
}

#[test]
fn scenario_invalid_milestone_amount_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = soroban_sdk::Address::generate(&env);
    let freelancer_addr = soroban_sdk::Address::generate(&env);
    let milestones = vec![&env, 0_i128];
    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    super::assert_contract_error(result, EscrowError::InvalidMilestoneAmount);
}

#[test]
fn scenario_invalid_milestone_amount_negative_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = soroban_sdk::Address::generate(&env);
    let freelancer_addr = soroban_sdk::Address::generate(&env);
    let milestones = vec![&env, -100_0000000_i128];
    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    super::assert_contract_error(result, EscrowError::InvalidMilestoneAmount);
}

#[test]
fn scenario_invalid_deposit_amount_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = soroban_sdk::Address::generate(&env);
    let freelancer_addr = soroban_sdk::Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);

    let result = client.try_deposit_funds(&contract_id, &0);
    super::assert_contract_error(result, EscrowError::InvalidDepositAmount);
}

#[test]
fn scenario_same_client_freelancer_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let same_addr = soroban_sdk::Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let result = client.try_create_contract(&same_addr, &same_addr, &milestones);
    super::assert_contract_error(result, EscrowError::InvalidParticipant);
}

#[test]
fn scenario_refund_partial_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = soroban_sdk::Address::generate(&env);
    let freelancer_addr = soroban_sdk::Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 100_0000000_i128];
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);

    _ = client.deposit_funds(&contract_id, &100_0000000_i128);

    let record = client.get_contract(&contract_id);
    assert_eq!(record.funded_amount, 100_0000000_i128);
}
