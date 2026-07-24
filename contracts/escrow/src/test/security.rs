use soroban_sdk::testutils::Ledger as _;
use super::{
    create_contract, default_milestones, generated_participants, register_client,
    total_milestone_amount,
};
use crate::{Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, String, Vec};

fn reputation_comment(env: &Env) -> String {
    String::from_str(env, "Good job")
}

#[test]
fn create_rejects_same_participants() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (addr, _) = generated_participants(&env);

    let result = client.try_create_contract(
        &addr,
        &addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, EscrowError::InvalidParticipant);
}

#[test]
fn create_rejects_empty_milestone_list() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let empty = Vec::<i128>::new(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &empty,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, EscrowError::EmptyMilestones);
}

#[test]
fn create_rejects_non_positive_milestone_amount() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
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
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_deposit_funds(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::InvalidDepositAmount);
}

#[test]
fn release_rejects_when_contract_not_funded() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::InsufficientFunds);
}

#[test]
fn release_rejects_invalid_milestone_id() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount()));
    client.approve_milestone_release(&contract_id, &client_addr, &0);
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::AlreadyReleased);
}

#[test]
fn issue_reputation_rejects_unfinished_contract() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let comment = reputation_comment(&env);
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &comment);
    super::assert_contract_error(result, Error::NotCompleted);
}

#[test]
fn issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    let comment = reputation_comment(&env);
    let result = client.try_issue_reputation(&contract_id, &client_addr, &0, &comment);
    super::assert_contract_error(result, Error::InvalidRating);
}

#[test]
fn issue_reputation_once_per_contract() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    let comment = reputation_comment(&env);
    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &comment));
    let result = client.try_issue_reputation(&contract_id, &client_addr, &4, &comment);
    super::assert_contract_error(result, Error::ReputationAlreadyIssued);
}

#[test]
fn issue_reputation_rejects_empty_comment() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    let empty_comment = String::from_str(&env, "");
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &empty_comment);
    super::assert_contract_error(result, Error::EmptyComment);
}

#[test]
fn issue_reputation_rejects_unauthorized_caller() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let unauthorized = soroban_sdk::Address::generate(&env);

    let comment = reputation_comment(&env);
    let result = client.try_issue_reputation(&contract_id, &unauthorized, &5, &comment);
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

/// Finalize a completed contract and return all identifiers.
fn finalized_contract(
    env: &Env,
) -> (
    crate::EscrowClient<'_>,
    soroban_sdk::Address,
    soroban_sdk::Address,
    u32,
) {
    let client = register_client(env);
    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    (client, client_addr, freelancer_addr, contract_id)
}

#[test]
fn finalized_contract_read_operations_still_work() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();

    let (client, _, _, contract_id) = finalized_contract(&env);

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, crate::ContractStatus::Completed);

    let record = client.get_finalization_record(&contract_id);
    assert!(record.is_some());
}

#[test]
fn finalize_cannot_be_called_twice() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();

    let (client, client_addr, _, contract_id) = finalized_contract(&env);

    let result = client.try_finalize_contract(&contract_id, &client_addr);

    super::assert_contract_error(result, EscrowError::AlreadyFinalized);
}

#[test]
fn finalized_contract_rejects_cancel() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();

    let (client, client_addr, _, contract_id) = finalized_contract(&env);

    let result = client.try_cancel_contract(&contract_id, &client_addr);

    super::assert_contract_error(result, EscrowError::AlreadyFinalized);
}

#[test]
fn finalized_contract_rejects_refund() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();

    let (client, _, _, contract_id) = finalized_contract(&env);

    let indices = vec![&env, 0u32];

    let result = client.try_refund_unreleased_milestones(&contract_id, &indices);

    super::assert_contract_error(result, EscrowError::AlreadyFinalized);
}

#[test]
fn finalized_contract_rejects_release() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();

    let (client, client_addr, _, contract_id) = finalized_contract(&env);

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);

    super::assert_contract_error(result, EscrowError::AlreadyFinalized);
}

#[test]
fn deposit_rejected_after_cancel() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    // Cancel immediately in Created state
    assert!(client.cancel_contract(&contract_id, &client_addr));

    let result = client.try_deposit_funds(&contract_id, &client_addr, &100_i128);
    super::assert_contract_error(result, EscrowError::ContractCancelled);
}

#[test]
fn release_rejected_after_cancel() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    // Fully fund and then cancel
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    assert!(client.cancel_contract(&contract_id, &client_addr));

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, EscrowError::ContractCancelled);
}

#[test]
fn refund_rejected_after_refund() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    // Fund and refund all milestones
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    let all_indices = vec![&env, 0_u32, 1_u32, 2_u32];
    assert!(client.refund_unreleased_milestones(&contract_id, &all_indices) > 0);

    // Second refund attempt should be rejected as contract is terminally refunded
    let res = client.try_refund_unreleased_milestones(&contract_id, &all_indices);
    super::assert_contract_error(res, EscrowError::ContractRefunded);
}
