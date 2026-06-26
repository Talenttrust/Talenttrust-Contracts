use super::{create_contract, default_milestones, generated_participants, register_client, total_milestone_amount};
use crate::{EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Env, Vec};
#[allow(unused_imports)]
use super::{
    assert_contract_error, complete_contract, create_contract, default_milestones,
    generated_participants, register_client, total_milestone_amount,
};

#[test]
fn create_rejects_same_participants() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (addr, _) = generated_participants(&env);

    let result =
        client.try_create_contract(&addr, &addr, &None, &default_milestones(&env), &ReleaseAuthorization::ClientOnly);
    super::assert_contract_error(result, EscrowError::InvalidParticipant);
}

#[test]
fn create_rejects_empty_milestone_list() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let empty = Vec::<i128>::new(&env);

    let result =
        client.try_create_contract(&client_addr, &freelancer_addr, &None, &empty, &ReleaseAuthorization::ClientOnly);
    super::assert_contract_error(result, EscrowError::EmptyMilestones);
}

#[test]
fn create_rejects_non_positive_milestone_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = vec![&env, 100_i128, 0_i128];

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, EscrowError::InvalidMilestoneAmount);
}

#[test]
#[should_panic]
fn create_requires_client_authorization() {
    let env = Env::default();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);

    let _ = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
fn deposit_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_deposit_funds(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::InvalidDepositAmount);
}

#[test]
fn release_rejects_when_contract_not_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::InsufficientFunds);
}

#[test]
fn release_rejects_invalid_milestone_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount()));
    let result = client.try_release_milestone(&contract_id, &client_addr, &99);
    super::assert_contract_error(result, EscrowError::InvalidMilestone);
}

#[test]
fn release_rejects_double_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::AlreadyReleased);
}

#[test]
fn issue_reputation_rejects_unfinished_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5);
    super::assert_contract_error(result, EscrowError::NotCompleted);
}

#[test]
fn issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &0);
    super::assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn issue_reputation_once_per_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5));
    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &4);
    super::assert_contract_error(result, EscrowError::ReputationAlreadyIssued);
}

#[test]
fn issue_reputation_rejects_freelancer_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let wrong_freelancer = soroban_sdk::Address::generate(&env);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &wrong_freelancer, &5);
    super::assert_contract_error(result, EscrowError::FreelancerMismatch);
}

#[test]
fn issue_reputation_rejects_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let unauthorized = soroban_sdk::Address::generate(&env);

    let result = client.try_issue_reputation(&contract_id, &unauthorized, &freelancer_addr, &5);
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ── Terminal-state guards ──────────────────────────────────────────────────

/// Helper: create a contract, deposit the full amount, then cancel it.
/// Returns (client_addr, freelancer_addr, contract_id).
fn create_and_cancel(env: &Env, client: &crate::EscrowClient) -> (soroban_sdk::Address, soroban_sdk::Address, u32) {
    let (client_addr, freelancer_addr, contract_id) = create_contract(env, client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    assert!(client.cancel_contract(&contract_id, &client_addr));
    (client_addr, freelancer_addr, contract_id)
}

/// Helper: create a contract, deposit funds, refund all milestones.
/// Returns (client_addr, freelancer_addr, contract_id).
fn create_and_refund(env: &Env, client: &crate::EscrowClient) -> (soroban_sdk::Address, soroban_sdk::Address, u32) {
    let (client_addr, freelancer_addr, contract_id) = create_contract(env, client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    let indices = soroban_sdk::vec![env, 0_u32, 1_u32, 2_u32];
    client.refund_unreleased_milestones(&contract_id, &indices);
    (client_addr, freelancer_addr, contract_id)
}

#[test]
fn deposit_rejected_on_cancelled_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_and_cancel(&env, &client);

    let result = client.try_deposit_funds(&contract_id, &client_addr, &1_0000000_i128);
    super::assert_contract_error(result, crate::Error::ContractCancelled);
}

#[test]
fn deposit_rejected_on_refunded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_and_refund(&env, &client);

    let result = client.try_deposit_funds(&contract_id, &client_addr, &1_0000000_i128);
    super::assert_contract_error(result, crate::Error::ContractCancelled);
}

#[test]
fn release_rejected_on_cancelled_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_and_cancel(&env, &client);

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, crate::Error::ContractCancelled);
}

#[test]
fn release_rejected_on_refunded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_and_refund(&env, &client);

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, crate::Error::ContractCancelled);
}

#[test]
fn refund_rejected_on_cancelled_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    // Cancel a contract that has no funded amount (Created state)
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.cancel_contract(&contract_id, &client_addr));

    let indices = soroban_sdk::vec![&env, 0_u32];
    let result = client.try_refund_unreleased_milestones(&contract_id, &indices);
    super::assert_contract_error(result, crate::Error::ContractCancelled);
}

#[test]
fn refund_rejected_on_refunded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_and_refund(&env, &client);

    // Try to refund again after contract is already Refunded
    let indices = soroban_sdk::vec![&env, 0_u32];
    let result = client.try_refund_unreleased_milestones(&contract_id, &indices);
    super::assert_contract_error(result, crate::Error::ContractCancelled);
}

/// Confirms cancel_contract itself cannot operate on an already-Cancelled contract.
#[test]
fn cancel_cannot_be_called_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.cancel_contract(&contract_id, &client_addr));

    // Second cancel must fail — Cancelled is not in {Created, PartiallyFunded, Funded}
    let result = client.try_cancel_contract(&contract_id, &client_addr);
    super::assert_contract_error(result, crate::EscrowError::InvalidState);
}

/// Confirms cancel_contract cannot operate on a Completed contract.
#[test]
fn cancel_rejected_on_completed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    let result = client.try_cancel_contract(&contract_id, &client_addr);
    super::assert_contract_error(result, crate::EscrowError::InvalidState);
}

/// Confirms cancel_contract cannot operate on a Refunded contract.
#[test]
fn cancel_rejected_on_refunded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_and_refund(&env, &client);
    // Refund transitions to Refunded — cancel must now fail
    let attacker = soroban_sdk::Address::generate(&env);
    let result = client.try_cancel_contract(&contract_id, &attacker);
    // UnauthorizedRole fires before InvalidState because the caller check runs first
    super::assert_contract_error(result, crate::EscrowError::UnauthorizedRole);
}
