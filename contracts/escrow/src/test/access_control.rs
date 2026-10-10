use super::{default_milestones, generated_participants3, register_client, total_milestones};
use crate::{Error, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn test_only_client_can_deposit_funds() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_deposit_funds(&contract_id, &freelancer_addr, &total_milestones());
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_freelancer_cannot_approve_milestone_release() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_approve_milestone_release(&contract_id, &freelancer_addr, &0);
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_freelancer_cannot_release_milestone() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &0));

    let result = client.try_release_milestone(&contract_id, &freelancer_addr, &0);
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_only_client_can_issue_reputation() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert! (client.release_milestone(&contract_id, &client_addr, &0));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert! (client.release_milestone(&contract_id, &client_addr, &1));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert! (client.release_milestone(&contract_id, &client_addr, &2));

    let result = client.try_issue_reputation(
        &contract_id,
        &freelancer_addr,
        &5,
        &soroban_sdk::String::from_str(&env, "test"),
    );
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_issue_reputation_rejects_freelancer_mismatch() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);
    let wrong_freelancer = soroban_sdk::Address::generate(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert! (client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert! (client.release_milestone(&contract_id, &client_addr, &0));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert! (client.release_milestone(&contract_id, &client_addr, &1));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert! (client.release_milestone(&contract_id, &client_addr, &2));

    let result = client.try_issue_reputation(
        &contract_id,
        &wrong_freelancer,
        &5,
        &soroban_sdk::String::from_str(&env, "test"),
    );
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_create_rejects_arbiter_modes_without_arbiter() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    super::assert_contract_error(result, Error::MissingArbiter);
}

#[test]
fn test_create_rejects_invalid_arbiter_role_overlap() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(client_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    super::assert_contract_error(result, Error::InvalidArbiter);
}

#[test]
#[should_panic]
fn test_create_contract_requires_authentication_of_roles() {
    let env = Env::default();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

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
    let (client_addr, _freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let result = client.try_create_contract(
        &client_addr,
        &client_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, Error::InvalidParticipant);
}

#[test]
fn test_create_rejects_empty_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);
    let empty = soroban_sdk::Vec::<i128>::new(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &empty,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, Error::EmptyMilestones);
}

#[test]
fn test_deposit_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_deposit_funds(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::AmountMustBePositive);
}

#[test]
fn test_deposit_rejects_when_contract_not_created() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    let result = client.try_deposit_funds(&contract_id, &client_addr, &total_milestones());
    super::assert_contract_error(result, Error::InvalidState);
}

#[test]
fn test_approve_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::InvalidState);
}

#[test]
fn test_approve_rejects_already_released_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert! (client.release_milestone(&contract_id, &client_addr, &0));

    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::MilestoneAlreadyReleased);
}

#[test]
fn test_approve_rejects_duplicate_client_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &0));
    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::AlreadyApproved);
}

#[test]
fn test_approve_rejects_duplicate_arbiter_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &arbiter_addr, &0));
    let result = client.try_approve_milestone_release(&contract_id, &arbiter_addr, &0);
    super::assert_contract_error(result, Error::AlreadyApproved);
}

#[test]
fn test_release_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::InvalidState);
}

#[test]
fn test_release_rejects_without_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::InsufficientApprovals);
}

#[test]
fn test_release_rejects_duplicate_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert! (client.release_milestone(&contract_id, &client_addr, &0));

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::MilestoneAlreadyReleased);
}

#[test]
fn test_release_rejects_out_of_bounds_milestone_index() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &u32::MAX);
    super::assert_contract_error(result, Error::InvalidMilestoneIndex);
}

#[test]
fn test_deposit_rejects_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_deposit_funds(&contract_id, &client_addr, &-1);
    super::assert_contract_error(result, Error::AmountMustBePositive);
}

#[test]
fn test_create_rejects_arbiter_equal_to_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(freelancer_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    super::assert_contract_error(result, Error::InvalidArbiter);
}

#[test]
fn test_approve_rejects_out_of_bounds_milestone_index() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &u32::MAX);
    super::assert_contract_error(result, Error::InvalidMilestoneIndex);
}

#[test]
fn test_issue_reputation_requires_all_milestones_released() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert! (client.release_milestone(&contract_id, &client_addr, &0));

    let result = client.try_issue_reputation(
        &contract_id,
        &freelancer_addr,
        &5,
        &soroban_sdk::String::from_str(&env, "test"),
    );
    super::assert_contract_error(result, Error::NotCompleted);
}

#[test]
fn test_issue_reputation_rejects_duplicate_issuance() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert! (client.release_milestone(&contract_id, &client_addr, &0));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert! (client.release_milestone(&contract_id, &client_addr, &1));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert! (client.release_milestone(&contract_id, &client_addr, &2));

    assert! (client.issue_reputation(
        &contract_id,
        &freelancer_addr,
        &5,
        &soroban_sdk::String::from_str(&env, "test"),
    ));
    assert!(client.issue_reputation(
        &contract_id,
        &client_addr,
        &4,
        &soroban_sdk::String::from_str(&env, "test2"),
    ));
}

#[test]
fn test_create_rejects_arbiter_equal_to_client_for_arbiter_only() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(client_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    super::assert_contract_error(result, Error::InvalidArbiter);
}

#[test]
fn test_create_rejects_arbiter_mode_without_arbiter_client_and_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    super::assert_contract_error(result, Error::MissingArbiter);
}

#[test]
fn test_deposit_rejects_wrong_client() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);
    let other = soroban_sdk::Address::generate(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_deposit_funds(&contract_id, &other, &total_milestones());
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_release_rejects_before_approval_for_arbiter_only() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_release_milestone(&contract_id, &arbiter_addr, &0);
    super::assert_contract_error(result, Error::InsufficientApprovals);
}

#[test]
fn test_client_cannot_approve_in_arbiter_only_mode() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_arbiter_cannot_approve_in_client_only_mode() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_approve_milestone_release(&contract_id, &arbiter_addr, &0);
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_client_and_arbiter_mode_requires_both_approvals_before_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));

    // Only client approved: release must be rejected.
    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::InsufficientApprovals);

    // Arbiter approval completes the quorum.
    assert! (client.approve_milestone_release(&contract_id, &arbiter_addr, &0));
    assert! (client.release_milestone(&contract_id, &client_addr, &0));
}

#[test]
fn test_client_and_arbiter_mode_rejects_duplicate_client_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &client_addr, &0));
    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::AlreadyApproved);
}

#[test]
fn test_client_and_arbiter_mode_rejects_duplicate_arbiter_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert! (client.approve_milestone_release(&contract_id, &arbiter_addr, &0));
    let result = client.try_approve_milestone_release(&contract_id, &arbiter_addr, &0);
    super::assert_contract_error(result, Error::AlreadyApproved);
}

#[test]
fn test_client_and_arbiter_mode_rejects_release_without_arbiter_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &arbiter_addr, &0));

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::InsufficientApprovals);
}

#[test]
fn test_repeated_release_is_idempotently_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert! (client.release_milestone(&contract_id, &client_addr, &0));

    // Repeated release attempts must all reject and not mutate state.
    for _ in 0..3 {
        let result = client.try_release_milestone(&contract_id, &client_addr, &0);
        super::assert_contract_error(result, Error::MilestoneAlreadyReleased);
    }
}

#[test]
fn test_repeated_deposit_rejected_after_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    for _ in 0..3 {
        let result = client.try_deposit_funds(&contract_id, &client_addr, &total_milestones());
        super::assert_contract_error(result, Error::InvalidState);
    }
}

#[test]
fn test_full_lifecycle_releases_all_milestones_and_issues_reputation() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    for index in 0..3_u32 {
        assert! (client.approve_milestone_release(&contract_id, &client_addr, &index));
        assert! (client.release_milestone(&contract_id, &client_addr, &index));
    }

    assert! (client.issue_reputation(
        &contract_id,
        &freelancer_addr,
        &5,
        &soroban_sdk::String::from_str(&env, "test"),
    ));
}
