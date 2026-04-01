use super::{
    create_contract, default_milestones, generated_participants, register_client, MILESTONE_ONE,
};
use crate::EscrowError;
use soroban_sdk::{vec, Env, Vec};

#[test]
fn create_rejects_same_participants() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (addr, _) = generated_participants(&env);

    let result = client.try_create_contract(&addr, &addr, &default_milestones(&env));
    super::assert_contract_error(result, EscrowError::InvalidParticipants);
}

#[test]
fn create_rejects_empty_milestone_list() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let empty = Vec::<i128>::new(&env);

    let result = client.try_create_contract(&client_addr, &freelancer_addr, &empty);
    super::assert_contract_error(result, EscrowError::EmptyMilestones);
}

#[test]
fn create_rejects_non_positive_milestone_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = vec![&env, 100_i128, 0_i128];

    let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
    super::assert_contract_error(result, EscrowError::InvalidMilestoneAmount);
}

#[test]
#[should_panic]
fn create_requires_client_authorization() {
    let env = Env::default();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let _ = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
}

#[test]
fn deposit_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_deposit_funds(&contract_id, &0);
    super::assert_contract_error(result, EscrowError::AmountMustBePositive);
}

#[test]
fn deposit_rejects_overfunding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &(super::total_milestone_amount())));
    let result = client.try_deposit_funds(&contract_id, &1);
    super::assert_contract_error(result, EscrowError::FundingExceedsRequired);
}

#[test]
fn release_rejects_when_contract_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_release_milestone(&contract_id, &0);
    super::assert_contract_error(result, EscrowError::InvalidState);
}

#[test]
fn release_rejects_insufficient_escrow_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &(MILESTONE_ONE - 1)));
    let result = client.try_release_milestone(&contract_id, &0);
    super::assert_contract_error(result, EscrowError::InsufficientEscrowBalance);
}

#[test]
fn release_rejects_invalid_milestone_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &super::total_milestone_amount()));
    let result = client.try_release_milestone(&contract_id, &99);
    super::assert_contract_error(result, EscrowError::MilestoneNotFound);
}

#[test]
fn release_rejects_double_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &super::total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &0));

    let result = client.try_release_milestone(&contract_id, &0);
    super::assert_contract_error(result, EscrowError::MilestoneAlreadyReleased);
}

#[test]
fn issue_reputation_rejects_unfinished_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &5);
    super::assert_contract_error(result, EscrowError::InvalidState);
}

#[test]
fn issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &0);
    super::assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn issue_reputation_once_per_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.issue_reputation(&contract_id, &5));
    let result = client.try_issue_reputation(&contract_id, &4);
    super::assert_contract_error(result, EscrowError::ReputationAlreadyIssued);
}
