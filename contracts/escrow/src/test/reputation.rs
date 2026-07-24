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

// ---------------------------------------------------------------------------
// get_reputation_view tests
// ---------------------------------------------------------------------------

#[test]
fn get_reputation_view_returns_all_zero_defaults_for_unknown_address() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let unknown = Address::generate(&env);

    let view = client.get_reputation_view(&unknown);
    assert_eq!(view.completed_contracts, 0);
    assert_eq!(view.total_rating, 0);
    assert_eq!(view.last_rating, 0);
    assert_eq!(view.average_rating_bps, 0);
    assert_eq!(view.pending_credits, 0);
}

#[test]
fn get_reputation_view_pending_credits_before_rating() {
    // After completing a contract but before issuing reputation,
    // pending_credits should be 1 and rated fields should be 0.
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let freelancer = Address::generate(&env);
    let client_addr = Address::generate(&env);

    complete_contract_for(&env, &client, &client_addr, &freelancer);

    let view = client.get_reputation_view(&freelancer);
    assert_eq!(view.completed_contracts, 0);
    assert_eq!(view.total_rating, 0);
    assert_eq!(view.last_rating, 0);
    assert_eq!(view.average_rating_bps, 0);
    assert_eq!(view.pending_credits, 1);
}

#[test]
fn get_reputation_view_after_single_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));

    let view = client.get_reputation_view(&freelancer_addr);
    assert_eq!(view.completed_contracts, 1);
    assert_eq!(view.total_rating, 5);
    assert_eq!(view.last_rating, 5);
    // 5 * 10_000 / 1 = 50_000
    assert_eq!(view.average_rating_bps, 50_000);
    assert_eq!(view.pending_credits, 0);
}

#[test]
fn get_reputation_view_average_bps_matches_get_average_rating() {
    // Ensure the embedded average in the view is consistent with the
    // standalone get_average_rating function.
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    client.issue_reputation(&contract_id, &client_addr, &4, &valid_comment(&env));

    let view = client.get_reputation_view(&freelancer_addr);
    let standalone = client.get_average_rating(&freelancer_addr);
    assert_eq!(Some(view.average_rating_bps), standalone);
}

#[test]
fn get_reputation_view_after_multiple_ratings() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let freelancer = Address::generate(&env);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);
    let c3 = Address::generate(&env);

    let id1 = complete_contract_for(&env, &client, &c1, &freelancer);
    let id2 = complete_contract_for(&env, &client, &c2, &freelancer);
    let id3 = complete_contract_for(&env, &client, &c3, &freelancer);

    client.issue_reputation(&id1, &c1, &3, &valid_comment(&env));
    client.issue_reputation(&id2, &c2, &4, &valid_comment(&env));
    client.issue_reputation(&id3, &c3, &5, &valid_comment(&env));

    let view = client.get_reputation_view(&freelancer);
    assert_eq!(view.completed_contracts, 3);
    assert_eq!(view.total_rating, 12);
    assert_eq!(view.last_rating, 5);
    // 12 * 10_000 / 3 = 40_000
    assert_eq!(view.average_rating_bps, 40_000);
    assert_eq!(view.pending_credits, 0);
}

#[test]
fn get_reputation_view_pending_credits_count_multiple_unrated_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let freelancer = Address::generate(&env);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);
    let c3 = Address::generate(&env);

    complete_contract_for(&env, &client, &c1, &freelancer);
    complete_contract_for(&env, &client, &c2, &freelancer);
    complete_contract_for(&env, &client, &c3, &freelancer);

    let view = client.get_reputation_view(&freelancer);
    assert_eq!(view.completed_contracts, 0);
    assert_eq!(view.average_rating_bps, 0);
    assert_eq!(view.pending_credits, 3);
}

#[test]
fn get_reputation_view_fractional_average_preserved_in_bps() {
    // Ratings 1 and 2 → average 1.5 → 15_000 bps
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let freelancer = Address::generate(&env);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);

    let id1 = complete_contract_for(&env, &client, &c1, &freelancer);
    let id2 = complete_contract_for(&env, &client, &c2, &freelancer);

    client.issue_reputation(&id1, &c1, &1, &valid_comment(&env));
    client.issue_reputation(&id2, &c2, &2, &valid_comment(&env));

    let view = client.get_reputation_view(&freelancer);
    assert_eq!(view.average_rating_bps, 15_000);
}

#[test]
fn get_reputation_view_minimum_rating_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    client.issue_reputation(&contract_id, &client_addr, &1, &valid_comment(&env));

    let view = client.get_reputation_view(&freelancer_addr);
    assert_eq!(view.last_rating, 1);
    assert_eq!(view.average_rating_bps, 10_000);
}

#[test]
fn get_reputation_view_maximum_rating_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));

    let view = client.get_reputation_view(&freelancer_addr);
    assert_eq!(view.last_rating, 5);
    assert_eq!(view.average_rating_bps, 50_000);
}

#[test]
fn get_reputation_view_is_read_only_does_not_mutate_state() {
    // Calling get_reputation_view twice should return identical results.
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    client.issue_reputation(&contract_id, &client_addr, &4, &valid_comment(&env));

    let view1 = client.get_reputation_view(&freelancer_addr);
    let view2 = client.get_reputation_view(&freelancer_addr);
    assert_eq!(view1, view2);
}

#[test]
fn get_reputation_view_mixed_rated_and_pending() {
    // 2 rated + 1 pending → pending_credits == 1, completed_contracts == 2
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let freelancer = Address::generate(&env);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);
    let c3 = Address::generate(&env);

    let id1 = complete_contract_for(&env, &client, &c1, &freelancer);
    let id2 = complete_contract_for(&env, &client, &c2, &freelancer);
    complete_contract_for(&env, &client, &c3, &freelancer); // unrated

    client.issue_reputation(&id1, &c1, &5, &valid_comment(&env));
    client.issue_reputation(&id2, &c2, &3, &valid_comment(&env));

    let view = client.get_reputation_view(&freelancer);
    assert_eq!(view.completed_contracts, 2);
    assert_eq!(view.total_rating, 8);
    assert_eq!(view.last_rating, 3);
    // 8 * 10_000 / 2 = 40_000
    assert_eq!(view.average_rating_bps, 40_000);
    assert_eq!(view.pending_credits, 1);
}

#[test]
fn get_reputation_view_distinct_addresses_are_independent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let freelancer_a = Address::generate(&env);
    let freelancer_b = Address::generate(&env);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);

    let id1 = complete_contract_for(&env, &client, &c1, &freelancer_a);
    let id2 = complete_contract_for(&env, &client, &c2, &freelancer_b);

    client.issue_reputation(&id1, &c1, &5, &valid_comment(&env));
    // freelancer_b has a pending credit but no rating

    let view_a = client.get_reputation_view(&freelancer_a);
    let view_b = client.get_reputation_view(&freelancer_b);

    assert_eq!(view_a.completed_contracts, 1);
    assert_eq!(view_a.average_rating_bps, 50_000);
    assert_eq!(view_a.pending_credits, 0);

    assert_eq!(view_b.completed_contracts, 0);
    assert_eq!(view_b.average_rating_bps, 0);
    assert_eq!(view_b.pending_credits, 1);
    let _ = id2;
}
