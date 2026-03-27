use super::{
    default_milestones, generated_participants, register_client, total_milestone_amount,
    MILESTONE_ONE,
};
use crate::EscrowError;
use soroban_sdk::Env;

#[test]
fn test_create_rejects_same_participants() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (addr, _) = generated_participants(&env);

    let result = client.try_create_contract(&addr, &addr, &default_milestones(&env));
    assert_eq!(result, Err(Ok(EscrowError::InvalidParticipants)));
}

#[test]
fn test_create_rejects_empty_milestones() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let empty = soroban_sdk::Vec::<i128>::new(&env);

    let result = client.try_create_contract(&client_addr, &freelancer_addr, &empty);
    assert_eq!(result, Err(Ok(EscrowError::EmptyMilestones)));
}

#[test]
fn test_create_rejects_non_positive_milestone_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = soroban_sdk::vec![&env, 100_i128, 0_i128];

    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    assert_eq!(result, Err(Ok(EscrowError::InvalidMilestoneAmount)));
}

#[test]
fn test_deposit_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    let result = client.try_deposit_funds(&contract_id, &0);
    assert_eq!(result, Err(Ok(EscrowError::AmountMustBePositive)));
}

#[test]
fn test_deposit_rejects_overfunding() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    assert!(client.deposit_funds(&contract_id, &total_milestone_amount()));
    let result = client.try_deposit_funds(&contract_id, &1);
    assert_eq!(result, Err(Ok(EscrowError::FundingExceedsRequired)));
}

#[test]
fn test_release_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    let result = client.try_release_milestone(&contract_id, &0);
    assert_eq!(result, Err(Ok(EscrowError::InvalidState)));
}

#[test]
fn test_release_rejects_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    assert!(client.deposit_funds(&contract_id, &(MILESTONE_ONE - 1)));

    let result = client.try_release_milestone(&contract_id, &0);
    assert_eq!(result, Err(Ok(EscrowError::InsufficientEscrowBalance)));
}

#[test]
fn test_release_rejects_invalid_milestone_id() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    assert!(client.deposit_funds(&contract_id, &total_milestone_amount()));

    let result = client.try_release_milestone(&contract_id, &99);
    assert_eq!(result, Err(Ok(EscrowError::MilestoneNotFound)));
}

#[test]
fn test_release_rejects_double_release() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    assert!(client.deposit_funds(&contract_id, &total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &0));

    let result = client.try_release_milestone(&contract_id, &0);
    assert_eq!(result, Err(Ok(EscrowError::MilestoneAlreadyReleased)));
}

#[test]
fn test_issue_reputation_requires_completed_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    let result = client.try_issue_reputation(&contract_id, &5);
    assert_eq!(result, Err(Ok(EscrowError::InvalidState)));
}

#[test]
fn test_issue_reputation_rejects_invalid_rating() {
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

    let result = client.try_issue_reputation(&contract_id, &0);
    assert_eq!(result, Err(Ok(EscrowError::InvalidRating)));
}

#[test]
fn test_issue_reputation_once_per_contract() {
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

    assert!(client.issue_reputation(&contract_id, &5));
    let result = client.try_issue_reputation(&contract_id, &4);
    assert_eq!(result, Err(Ok(EscrowError::ReputationAlreadyIssued)));
}

#[test]
#[should_panic]
fn test_create_requires_client_authorization() {
    let env = Env::default();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let _ = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
}
