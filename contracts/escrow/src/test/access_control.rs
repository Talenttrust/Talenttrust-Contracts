use super::{default_milestones, generated_participants, register_client, total_milestones};
use crate::{EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn test_only_client_can_deposit_funds() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_deposit_funds(&contract_id, &freelancer_addr, &total_milestones());
    assert_eq!(result, Err(Ok(EscrowError::UnauthorizedRole)));
}

#[test]
fn test_freelancer_cannot_approve_milestone_release() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_approve_milestone_release(&contract_id, &freelancer_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::UnauthorizedRole)));
}

#[test]
fn test_freelancer_cannot_release_milestone() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));

    let result = client.try_release_milestone(&contract_id, &freelancer_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::UnauthorizedRole)));
}

#[test]
fn test_only_client_can_issue_reputation() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert!(client.release_milestone(&contract_id, &client_addr, &2));

    let result = client.try_issue_reputation(&contract_id, &freelancer_addr, &freelancer_addr, &5);
    assert_eq!(result, Err(Ok(EscrowError::UnauthorizedRole)));
}

#[test]
fn test_issue_reputation_rejects_freelancer_mismatch() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);
    let wrong_freelancer = soroban_sdk::Address::generate(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert!(client.release_milestone(&contract_id, &client_addr, &2));

    let result = client.try_issue_reputation(&contract_id, &client_addr, &wrong_freelancer, &5);
    assert_eq!(result, Err(Ok(EscrowError::FreelancerMismatch)));
}

#[test]
fn test_create_rejects_arbiter_modes_without_arbiter() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    assert_eq!(result, Err(Ok(EscrowError::MissingArbiter)));
}

#[test]
fn test_create_rejects_invalid_arbiter_role_overlap() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(client_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert_eq!(result, Err(Ok(EscrowError::InvalidArbiter)));
}

#[test]
#[should_panic]
fn test_create_contract_requires_authentication_of_roles() {
    let env = Env::default();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    // No env.mock_all_auths() in this test: role addresses must authorize.
    let _ = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
fn test_create_rejects_same_client_and_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let result = client.try_create_contract(
        &client_addr,
        &client_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(result, Err(Ok(EscrowError::InvalidParticipants)));
}

#[test]
fn test_create_rejects_empty_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);
    let empty = soroban_sdk::Vec::<i128>::new(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &empty,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(result, Err(Ok(EscrowError::EmptyMilestones)));
}

#[test]
fn test_deposit_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_deposit_funds(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::AmountMustBePositive)));
}

#[test]
fn test_deposit_rejects_when_contract_not_created() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    let result = client.try_deposit_funds(&contract_id, &client_addr, &total_milestones());
    assert_eq!(result, Err(Ok(EscrowError::InvalidState)));
}

#[test]
fn test_approve_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::InvalidState)));
}

#[test]
fn test_approve_rejects_already_released_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::MilestoneAlreadyReleased)));
}

#[test]
fn test_approve_rejects_duplicate_client_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::AlreadyApproved)));
}

#[test]
fn test_approve_rejects_duplicate_arbiter_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &arbiter_addr, &0));
    let result = client.try_approve_milestone_release(&contract_id, &arbiter_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::AlreadyApproved)));
}

#[test]
fn test_release_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::InvalidState)));
}

#[test]
fn test_release_rejects_already_released_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::MilestoneAlreadyReleased)));
}

#[test]
fn test_issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert!(client.release_milestone(&contract_id, &client_addr, &2));

    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &0);
    assert_eq!(result, Err(Ok(EscrowError::InvalidRating)));
}

#[test]
fn test_issue_reputation_requires_completed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5);
    assert_eq!(result, Err(Ok(EscrowError::InvalidState)));
}

#[test]
fn test_issue_reputation_rejects_duplicate_issuance() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert!(client.release_milestone(&contract_id, &client_addr, &2));

    assert!(client.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5));
    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &4);
    assert_eq!(result, Err(Ok(EscrowError::ReputationAlreadyIssued)));
}

#[test]
fn test_client_and_arbiter_mode_rejects_third_party_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants(&env);
    let outsider = soroban_sdk::Address::generate(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_approve_milestone_release(&contract_id, &outsider, &0);
    assert_eq!(result, Err(Ok(EscrowError::UnauthorizedRole)));
}

#[test]
fn test_arbiter_only_flow_enforces_arbiter_approval_and_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    // Client cannot approve in ArbiterOnly.
    let client_approval = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_eq!(client_approval, Err(Ok(EscrowError::UnauthorizedRole)));

    assert!(client.approve_milestone_release(&contract_id, &arbiter_addr, &0));
    assert!(client.release_milestone(&contract_id, &arbiter_addr, &0));
}
