use super::{complete_contract, create_contract, register_client};
use crate::{Contract, ContractStatus, DataKey, EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};
fn valid_comment(env: &Env) -> String {
    String::from_str(env, "Great job!")
}

/// Completes a new escrow for the supplied participants so multiple contracts
/// can accrue reputation credits to the same freelancer.
fn complete_contract_for(
    env: &Env,
    client: &crate::EscrowClient<'_>,
    client_addr: &Address,
    freelancer_addr: &Address,
) -> u32 {
    let contract_id = client.create_contract(
        client_addr,
        freelancer_addr,
        &None,
        &super::default_milestones(env),
        &ReleaseAuthorization::ClientOnly,
    );
    let total = super::total_milestone_amount();
    assert!(client.deposit_funds(&contract_id, client_addr, &total));
    for milestone_index in 0..3 {
        assert!(client.approve_milestone_release(&contract_id, client_addr, &milestone_index));
        assert!(client.release_milestone(&contract_id, client_addr, &milestone_index));
    }
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Completed
    );
    contract_id
}

#[test]
fn pending_reputation_credits_accumulate_and_drain_across_completed_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let freelancer = Address::generate(&env);
    let first_client = Address::generate(&env);
    let second_client = Address::generate(&env);
    let third_client = Address::generate(&env);

    let first_contract = complete_contract_for(&env, &client, &first_client, &freelancer);
    assert_eq!(client.get_pending_reputation_credits(&freelancer), 1);

    let second_contract = complete_contract_for(&env, &client, &second_client, &freelancer);
    assert_eq!(client.get_pending_reputation_credits(&freelancer), 2);

    let third_contract = complete_contract_for(&env, &client, &third_client, &freelancer);
    assert_eq!(client.get_pending_reputation_credits(&freelancer), 3);

    // A fully refunded contract is terminal but never earns a reputation credit.
    let refunded_client = Address::generate(&env);
    let refunded_contract = client.create_contract(
        &refunded_client,
        &freelancer,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(
        &refunded_contract,
        &refunded_client,
        &super::total_milestone_amount(),
    ));
    assert_eq!(
        client.refund_unreleased_milestones(&refunded_contract, &vec![&env, 0_u32, 1, 2]),
        super::total_milestone_amount()
    );
    assert_eq!(
        client.get_contract(&refunded_contract).status,
        ContractStatus::Refunded
    );
    assert_eq!(client.get_pending_reputation_credits(&freelancer), 3);

    assert!(client.issue_reputation(&first_contract, &first_client, &5, &valid_comment(&env)));
    assert_eq!(client.get_pending_reputation_credits(&freelancer), 2);
    assert_eq!(
        client
            .get_reputation(&freelancer)
            .unwrap()
            .completed_contracts,
        1
    );

    assert!(client.issue_reputation(&second_contract, &second_client, &4, &valid_comment(&env)));
    assert_eq!(client.get_pending_reputation_credits(&freelancer), 1);
    assert_eq!(
        client
            .get_reputation(&freelancer)
            .unwrap()
            .completed_contracts,
        2
    );

    assert!(client.issue_reputation(&third_contract, &third_client, &3, &valid_comment(&env)));
    assert_eq!(client.get_pending_reputation_credits(&freelancer), 0);
    assert_eq!(
        client
            .get_reputation(&freelancer)
            .unwrap()
            .completed_contracts,
        3
    );

    let duplicate =
        client.try_issue_reputation(&first_contract, &first_client, &1, &valid_comment(&env));
    super::assert_contract_error(duplicate, EscrowError::ReputationAlreadyIssued);
    assert_eq!(client.get_pending_reputation_credits(&freelancer), 0);
}

#[test]
fn issue_reputation_rejects_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);
    let unauthorized = Address::generate(&env);

    let result = client.try_issue_reputation(&contract_id, &unauthorized, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn issue_reputation_rejects_non_completed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::NotCompleted);
}

#[test]
fn issue_reputation_rejects_invalid_rating_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    let result_low =
        client.try_issue_reputation(&contract_id, &client_addr, &0, &valid_comment(&env));
    super::assert_contract_error(result_low, EscrowError::InvalidRating);

    let result_high =
        client.try_issue_reputation(&contract_id, &client_addr, &6, &valid_comment(&env));
    super::assert_contract_error(result_high, EscrowError::InvalidRating);
}

#[test]
fn issue_reputation_rejects_empty_comment() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    let empty_comment = String::from_str(&env, "");
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &empty_comment);
    super::assert_contract_error(result, EscrowError::EmptyComment);
}

#[test]
fn issue_reputation_rejects_comment_too_long() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    let long_str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let long_comment = String::from_str(&env, long_str);
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &long_comment);
    super::assert_contract_error(result, EscrowError::CommentTooLong);
}

#[test]
fn issue_reputation_rejects_duplicate_issuance() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));
    let result = client.try_issue_reputation(&contract_id, &client_addr, &4, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::ReputationAlreadyIssued);
}

#[test]
fn issue_reputation_rejects_self_rating_when_client_equals_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    env.as_contract(&client.address, || {
        let key = DataKey::Contract(contract_id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.freelancer = client_addr.clone();
        env.storage().persistent().set(&key, &contract);
    });

    let result = client.try_issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::SelfRating);
}

#[test]
fn issue_reputation_succeeds_for_distinct_client_and_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));
}

#[test]
fn issue_reputation_updates_reputation_record_and_pending_credits() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 1);
    assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));

    let reputation = client
        .get_reputation(&freelancer_addr)
        .expect("expected reputation record");
    assert_eq!(reputation.completed_contracts, 1);
    assert_eq!(reputation.total_rating, 5);
    assert_eq!(reputation.last_rating, 5);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 0);
}

// ---------------------------------------------------------------------------
// get_average_rating tests
// ---------------------------------------------------------------------------

#[test]
fn get_average_rating_returns_none_for_unknown_address() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let unknown = Address::generate(&env);
    assert!(client.get_average_rating(&unknown).is_none());
}

#[test]
fn get_average_rating_single_rating_returns_scaled_value() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    client.issue_reputation(&contract_id, &client_addr, &4, &valid_comment(&env));

    // 4 * 10_000 / 1 = 40_000
    assert_eq!(client.get_average_rating(&freelancer_addr), Some(40_000));
}

#[test]
fn get_average_rating_multiple_ratings_returns_correct_scaled_average() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    // First contract: rating 3
    let (client_addr1, freelancer_addr, contract_id1) = complete_contract(&env, &client);
    client.issue_reputation(&contract_id1, &client_addr1, &3, &valid_comment(&env));

    // Second contract: same freelancer, rating 5
    let client_addr2 = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let contract_id2 = client.create_contract(
        &client_addr2,
        &freelancer_addr,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    let total = super::total_milestone_amount();
    client.deposit_funds(&contract_id2, &client_addr2, &total);
    client.approve_milestone_release(&contract_id2, &client_addr2, &0);
    client.release_milestone(&contract_id2, &client_addr2, &0);
    client.approve_milestone_release(&contract_id2, &client_addr2, &1);
    client.release_milestone(&contract_id2, &client_addr2, &1);
    client.approve_milestone_release(&contract_id2, &client_addr2, &2);
    client.release_milestone(&contract_id2, &client_addr2, &2);
    client.issue_reputation(&contract_id2, &client_addr2, &5, &valid_comment(&env));

    // total_rating=8, completed_contracts=2 → 8 * 10_000 / 2 = 40_000
    assert_eq!(client.get_average_rating(&freelancer_addr), Some(40_000));
}

#[test]
fn get_average_rating_fractional_average_is_preserved() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    // First contract: rating 1
    let (client_addr1, freelancer_addr, contract_id1) = complete_contract(&env, &client);
    client.issue_reputation(&contract_id1, &client_addr1, &1, &valid_comment(&env));

    // Second contract: rating 2
    let client_addr2 = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let contract_id2 = client.create_contract(
        &client_addr2,
        &freelancer_addr,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    let total = super::total_milestone_amount();
    client.deposit_funds(&contract_id2, &client_addr2, &total);
    client.approve_milestone_release(&contract_id2, &client_addr2, &0);
    client.release_milestone(&contract_id2, &client_addr2, &0);
    client.approve_milestone_release(&contract_id2, &client_addr2, &1);
    client.release_milestone(&contract_id2, &client_addr2, &1);
    client.approve_milestone_release(&contract_id2, &client_addr2, &2);
    client.release_milestone(&contract_id2, &client_addr2, &2);
    client.issue_reputation(&contract_id2, &client_addr2, &2, &valid_comment(&env));

    // total_rating=3, completed_contracts=2 → 3 * 10_000 / 2 = 15_000
    assert_eq!(client.get_average_rating(&freelancer_addr), Some(15_000));
}

#[test]
fn issue_reputation_rejects_invalid_contract_id_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let result = client.try_issue_reputation(&0, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn issue_reputation_rejects_invalid_contract_id_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create one contract so next_contract_id = 2
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    // Try to use contract_id = 2 (which is next_contract_id)
    let result = client.try_issue_reputation(&2, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::InvalidContractId);

    // Try to use contract_id = 100 (way out of bounds)
    let result = client.try_issue_reputation(&100, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn get_reputation_comment_rejects_invalid_contract_id_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let result = client.try_get_reputation_comment(&0);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn get_reputation_comment_rejects_invalid_contract_id_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create one contract so next_contract_id = 2
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    // Try to use contract_id = 2 (which is next_contract_id)
    let result = client.try_get_reputation_comment(&2);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}
