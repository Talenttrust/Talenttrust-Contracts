use super::{assert_contract_error, complete_contract, register_client};
use crate::{Error, EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};

fn valid_comment(env: &Env) -> String {
    String::from_str(env, "Great work!")
}

fn setup_completed_contract(
    env: &Env,
) -> (crate::EscrowClient<'_>, Address, Address, Address, u32) {
    let client = register_client(env);
    let (client_addr, freelancer_addr, contract_id) = complete_contract(env, &client);
    let arbiter_addr = Address::generate(env);
    (
        client,
        client_addr,
        freelancer_addr,
        arbiter_addr,
        contract_id,
    )
}

fn setup_completed_contract_with_arbiter(
    env: &Env,
) -> (crate::EscrowClient<'_>, Address, Address, Address, u32) {
    let client = register_client(env);
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    let milestones = super::default_milestones(env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = super::total_milestone_amount();
    assert!(client.deposit_funds(&contract_id, &client_addr, &total));
    for i in 0..3u32 {
        assert!(client.approve_milestone_release(&contract_id, &client_addr, &i));
        assert!(client.release_milestone(&contract_id, &client_addr, &i));
    }
    (
        client,
        client_addr,
        freelancer_addr,
        arbiter_addr,
        contract_id,
    )
}

// ===========================================================================
// issue_reputation: role matrix
// ===========================================================================

#[test]
fn reputation_matrix_client_can_issue() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);

    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));
}

#[test]
fn reputation_matrix_admin_cannot_issue() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);
    let admin = Address::generate(&env);

    let result = client.try_issue_reputation(&contract_id, &admin, &5, &valid_comment(&env));
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn reputation_matrix_freelancer_cannot_issue() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _client_addr, freelancer_addr, _arbiter, contract_id) =
        setup_completed_contract(&env);

    let result =
        client.try_issue_reputation(&contract_id, &freelancer_addr, &5, &valid_comment(&env));
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn reputation_matrix_arbiter_cannot_issue() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _client_addr, _freelancer, arbiter_addr, contract_id) =
        setup_completed_contract_with_arbiter(&env);

    let result = client.try_issue_reputation(&contract_id, &arbiter_addr, &5, &valid_comment(&env));
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn reputation_matrix_stranger_cannot_issue() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);
    let stranger = Address::generate(&env);

    let result = client.try_issue_reputation(&contract_id, &stranger, &5, &valid_comment(&env));
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ===========================================================================
// issue_reputation: guard conditions
// ===========================================================================

#[test]
fn reputation_matrix_issue_requires_completed_status() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));
    assert_contract_error(result, EscrowError::NotCompleted);
}

#[test]
fn reputation_matrix_issue_rejects_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);

    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));
    let result = client.try_issue_reputation(&contract_id, &client_addr, &4, &valid_comment(&env));
    assert_contract_error(result, EscrowError::ReputationAlreadyIssued);
}

#[test]
fn reputation_matrix_issue_rejects_self_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);

    // Tamper: set freelancer = client
    crate::test::EscrowFixture::builder()
        .with_admin(Address::generate(&env))
        .with_participants(client_addr.clone(), client_addr.clone(), None)
        .with_milestones(super::default_milestones(&env))
        .funded()
        .build();

    // For the original contract, patch storage directly
    env.as_contract(&client.address, || {
        let key = crate::DataKey::Contract(contract_id);
        let mut contract: crate::Contract = env.storage().persistent().get(&key).unwrap();
        contract.freelancer = client_addr.clone();
        env.storage().persistent().set(&key, &contract);
    });

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));
    assert_contract_error(result, EscrowError::SelfRating);
}

#[test]
fn reputation_matrix_issue_rejects_invalid_rating_low() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &0, &valid_comment(&env));
    assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn reputation_matrix_issue_rejects_invalid_rating_high() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &6, &valid_comment(&env));
    assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn reputation_matrix_issue_rejects_empty_comment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);

    let empty = String::from_str(&env, "");
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &empty);
    assert_contract_error(result, EscrowError::EmptyComment);
}

#[test]
fn reputation_matrix_issue_rejects_long_comment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);

    let long_str = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqr";
    let long_comment = String::from_str(&env, long_str);
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &long_comment);
    assert_contract_error(result, EscrowError::CommentTooLong);
}

#[test]
fn reputation_matrix_issue_rejects_nonexistent_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, _contract_id) = setup_completed_contract(&env);

    let result = client.try_issue_reputation(&999u32, &client_addr, &5, &valid_comment(&env));
    assert_contract_error(result, EscrowError::InvalidContractId);
}

// ===========================================================================
// Read-only actions: any role can read
// ===========================================================================

#[test]
fn reputation_matrix_anyone_can_get_reputation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, _arbiter, contract_id) =
        setup_completed_contract(&env);

    assert!(client.issue_reputation(&contract_id, &client_addr, &4, &valid_comment(&env)));

    let admin = Address::generate(&env);
    let stranger = Address::generate(&env);

    // All roles can read reputation
    assert!(client.get_reputation(&freelancer_addr).is_some());
    assert!(client.get_reputation(&admin).is_some()); // returns None for unknown, no error
    assert!(client.get_reputation(&stranger).is_some());
    assert!(client.get_reputation(&client_addr).is_some());
}

#[test]
fn reputation_matrix_anyone_can_get_reputation_comment() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);

    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));

    // No auth needed, any caller can read the comment
    let _admin = Address::generate(&env);
    let _stranger = Address::generate(&env);
    let comment = client.get_reputation_comment(&contract_id);
    assert!(comment.is_some());
    assert_eq!(comment.unwrap(), valid_comment(&env));
}

#[test]
fn reputation_matrix_anyone_can_get_average_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, _arbiter, contract_id) =
        setup_completed_contract(&env);

    assert!(client.issue_reputation(&contract_id, &client_addr, &3, &valid_comment(&env)));

    // All roles can read average rating
    let rating = client.get_average_rating(&freelancer_addr);
    assert_eq!(rating, Some(30_000));

    let unknown = Address::generate(&env);
    assert!(client.get_average_rating(&unknown).is_none());
}

#[test]
fn reputation_matrix_anyone_can_get_pending_reputation_credits() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _client_addr, freelancer_addr, _arbiter, _contract_id) =
        setup_completed_contract(&env);

    // Pending credits are readable by anyone
    let credits = client.get_pending_reputation_credits(&freelancer_addr);
    assert_eq!(credits, 1);

    let stranger = Address::generate(&env);
    let stranger_credits = client.get_pending_reputation_credits(&stranger);
    assert_eq!(stranger_credits, 0);
}

// ===========================================================================
// Edge: paused contract rejects issue_reputation
// ===========================================================================

#[test]
fn reputation_matrix_issue_rejects_paused_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, _arbiter, contract_id) = setup_completed_contract(&env);

    client.pause();

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));
    assert_contract_error(result, EscrowError::ContractPaused);
}

// ===========================================================================
// Edge: read-only actions still work when paused
// ===========================================================================

#[test]
fn reputation_matrix_read_actions_work_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, _arbiter, contract_id) =
        setup_completed_contract(&env);

    assert!(client.issue_reputation(&contract_id, &client_addr, &4, &valid_comment(&env)));

    client.pause();

    // Read-only actions must still succeed while paused
    assert!(client.get_reputation(&freelancer_addr).is_some());
    assert!(client.get_reputation_comment(&contract_id).is_some());
    assert_eq!(client.get_average_rating(&freelancer_addr), Some(40_000));
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 0);
}
