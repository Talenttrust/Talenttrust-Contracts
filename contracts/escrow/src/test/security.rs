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
fn test_create_rejects_empty_milestone_list() {
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
fn test_release_rejects_when_contract_not_funded() {
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
fn test_release_rejects_insufficient_escrow_balance() {
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
fn test_issue_reputation_rejects_unfinished_contract() {
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

    // No auth mocking in this test: create_contract must request client auth.
    let _ = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
}

#[test]
fn governance_requires_admin_auth_valid_parameters_and_pending_admin_acceptance() {
    let (env, contract_id) = setup(false);
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let next_admin = Address::generate(&env);

    assert_panics(|| {
        client.initialize_protocol_governance(&admin, &10_i128, &4_u32, &1_i128, &5_i128);
    });

    env.mock_all_auths();

    assert!(client.initialize_protocol_governance(&admin, &10_i128, &4_u32, &1_i128, &5_i128));

    assert_panics(|| {
        client.initialize_protocol_governance(&admin, &10_i128, &4_u32, &1_i128, &5_i128);
    });
    assert_panics(|| {
        client.update_protocol_parameters(&0_i128, &4_u32, &1_i128, &5_i128);
    });
    assert_panics(|| {
        client.update_protocol_parameters(&10_i128, &0_u32, &1_i128, &5_i128);
    });
    assert_panics(|| {
        client.update_protocol_parameters(&10_i128, &4_u32, &5_i128, &4_i128);
    });
    assert_panics(|| {
        client.propose_governance_admin(&admin);
    });

    assert!(client.propose_governance_admin(&next_admin));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(next_admin.clone())
    );
}

#[test]
fn governance_admin_actions_require_current_admin_and_ratings_follow_governed_range() {
    let (env, contract_id) = setup(true);
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let next_admin = Address::generate(&env);
    let escrow_client = Address::generate(&env);
    let freelancer = Address::generate(&env);

    client.initialize_protocol_governance(&admin, &10_i128, &3_u32, &2_i128, &4_i128);
    client.propose_governance_admin(&next_admin);
    client.accept_governance_admin();
    assert!(client.update_protocol_parameters(&10_i128, &3_u32, &3_i128, &4_i128));

    let id = client.create_contract(&escrow_client, &freelancer, &vec![&env, 10_i128]);
    client.deposit_funds(&id, &10_i128);
    client.release_milestone(&id, &0_u32);

    assert_panics(|| {
        client.issue_reputation(&freelancer, &2_i128);
    });
    assert_panics(|| {
        client.issue_reputation(&freelancer, &5_i128);
    });
}
