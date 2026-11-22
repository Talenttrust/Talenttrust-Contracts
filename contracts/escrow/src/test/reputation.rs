//! Reputation, ratings and weighted reputation behaviour.

use super::*;

#[test]
fn rate_resolved_escrow_succeeds_once_and_cancelled_rejected() {
    let s = setup();

    // Create and resolve dispute
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    open_dispute_bonded(&s, &id, &s.client);
    resolve_refunding_bond(&s, &id, &5_000);

    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);

    // Rating a Resolved escrow succeeds once
    s.escrow.issue_reputation(&id, &4);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 4);

    // Second rating attempt is rejected
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &4),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );

    // Cancelled escrow rating is rejected
    let id_cancel = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id_cancel, &(50 * U));
    s.escrow.cancel_contract(&id_cancel);
    assert_eq!(
        s.escrow.get_contract(&id_cancel).status,
        ContractStatus::Cancelled
    );
    assert_eq!(
        s.escrow.try_issue_reputation(&id_cancel, &5),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn rate_then_void_restores_reputation_and_prevents_rerating() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    // Initial reputation is 0
    let rep0 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep0.completed_contracts, 0);
    assert_eq!(rep0.total_rating, 0);

    // Issue rating 5
    s.escrow.issue_reputation(&id, &5);
    let rep1 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep1.completed_contracts, 1);
    assert_eq!(rep1.total_rating, 5);

    // Verify rating record is stored
    let rating_rec = s.escrow.get_rating(&id).expect("rating record must exist");
    assert_eq!(rating_rec.freelancer, s.freelancer);
    assert_eq!(rating_rec.rating, 5);
    assert!(!rating_rec.voided);

    // Admin voids rating
    let reason = symbol_short!("fraud");
    s.escrow.void_rating(&id, &reason);

    // Reputation is restored to exact previous values
    let rep2 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep2.completed_contracts, 0);
    assert_eq!(rep2.total_rating, 0);

    // Rating record reflects voided state
    let rating_rec_voided = s.escrow.get_rating(&id).expect("rating record must exist");
    assert!(rating_rec_voided.voided);

    // Voiding twice is rejected with RatingAlreadyVoided
    assert_eq!(
        s.escrow.try_void_rating(&id, &reason),
        Err(Ok(EscrowError::RatingAlreadyVoided))
    );

    // Voided escrow cannot be re-rated (reputation_already_issued remains)
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
}

#[test]
fn void_unrated_escrow_fails_with_rating_not_found() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    assert_eq!(s.escrow.get_rating(&id), None);

    let reason = symbol_short!("fraud");
    assert_eq!(
        s.escrow.try_void_rating(&id, &reason),
        Err(Ok(EscrowError::RatingNotFound))
    );
}

#[test]
fn void_one_of_three_ratings_preserves_others() {
    let s = setup();

    // Create 3 escrows
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    let id3 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );

    // Deposit and complete each
    s.escrow.deposit_funds(&id1, &(50 * U));
    s.escrow.release_milestone(&id1, &0);
    s.escrow.issue_reputation(&id1, &5);

    s.escrow.deposit_funds(&id2, &(50 * U));
    s.escrow.release_milestone(&id2, &0);
    s.escrow.issue_reputation(&id2, &4);

    s.escrow.deposit_funds(&id3, &(50 * U));
    s.escrow.release_milestone(&id3, &0);
    s.escrow.issue_reputation(&id3, &3);

    // Total should be 3 contracts, 12 rating
    let rep_before = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep_before.completed_contracts, 3);
    assert_eq!(rep_before.total_rating, 12);

    // Void rating for escrow 2 (rating 4)
    s.escrow.void_rating(&id2, &symbol_short!("collusion"));

    // Total should now be 2 contracts, 8 rating (5 + 3)
    let rep_after = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep_after.completed_contracts, 2);
    assert_eq!(rep_after.total_rating, 8);

    // Check individual records
    assert!(!s.escrow.get_rating(&id1).unwrap().voided);
    assert!(s.escrow.get_rating(&id2).unwrap().voided);
    assert!(!s.escrow.get_rating(&id3).unwrap().voided);
}

#[test]
fn ratings_issued_before_upgrade_return_rating_not_found() {
    let s = setup();
    let unrecorded_id = 42;
    assert_eq!(s.escrow.get_rating(&unrecorded_id), None);
    assert_eq!(
        s.escrow.try_void_rating(&unrecorded_id, &symbol_short!("legacy")),
        Err(Ok(EscrowError::RatingNotFound))
    );
}

#[test]
fn non_admin_cannot_void_rating() {
    let s = setup();
    assert_eq!(s.escrow.get_admin(), s.admin);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    s.escrow.issue_reputation(&id, &5);

    let non_admin = Address::generate(&s.env);
    s.env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "void_rating",
            args: soroban_sdk::IntoVal::into_val(&(&id, symbol_short!("fraud")), &s.env),
            sub_invokes: &[],
        },
    }]);

    // Calling void_rating as non-admin must fail
    assert!(s.escrow.try_void_rating(&id, &symbol_short!("fraud")).is_err());
}

#[test]
fn test_rate_client_completed_success_and_event() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 150 * U],
    );
    s.escrow.deposit_funds(&id, &(150 * U));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    // Initial reputation of client is default (0 completed, 0 rating)
    assert_eq!(
        s.escrow.get_client_reputation(&s.client),
        Reputation {
            completed_contracts: 0,
            total_rating: 0,
        }
    );

    // Freelancer rates client with rating 5
    s.escrow.rate_client(&id, &5);

    // Verify event topics: ("rep_cli", id) and data: (client, rating)
    let events = s.env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.0, s.escrow.address);
    assert_eq!(last.1.len(), 2);
    let topic_sym = Symbol::try_from_val(&s.env, &last.1.get(0).unwrap()).unwrap();
    let topic_id = u64::try_from_val(&s.env, &last.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_sym, symbol_short!("rep_cli"));
    assert_eq!(topic_id, id);

    let (event_client, event_rating) = <(Address, u32)>::try_from_val(&s.env, &last.2).unwrap();
    assert_eq!(event_client, s.client);
    assert_eq!(event_rating, 5u32);

    // Reputation updated under client address
    let rep = s.escrow.get_client_reputation(&s.client);
    assert_eq!(
        rep,
        Reputation {
            completed_contracts: 1,
            total_rating: 5,
        }
    );

    // Unrelated addresses have default empty reputation
    let other = Address::generate(&s.env);
    assert_eq!(
        s.escrow.get_client_reputation(&other),
        Reputation::default()
    );

    // Freelancer's reputation is not touched by client rating
    assert_eq!(
        s.escrow.get_reputation(&s.freelancer),
        Reputation::default()
    );
}

#[test]
fn test_rate_client_resolved_success() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(200 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);
    resolve_refunding_bond(&s, &id, &5_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);

    // Freelancer rates client after dispute resolution
    s.escrow.rate_client(&id, &4);

    let rep = s.escrow.get_client_reputation(&s.client);
    assert_eq!(
        rep,
        Reputation {
            completed_contracts: 1,
            total_rating: 4,
        }
    );
}

#[test]
fn test_rate_client_authorization() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    // Client attempting to call rate_client is rejected
    let res = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &s.client,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "rate_client",
                args: (id, 5u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_rate_client(&id, &5);
    assert!(res.is_err());

    // Arbitrary third party calling rate_client is rejected
    let attacker = Address::generate(&s.env);
    let res_attacker = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "rate_client",
                args: (id, 5u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_rate_client(&id, &5);
    assert!(res_attacker.is_err());

    // Freelancer calling rate_client succeeds
    let res_ok = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &s.freelancer,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "rate_client",
                args: (id, 5u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_rate_client(&id, &5);
    assert_eq!(res_ok, Ok(Ok(())));
}

#[test]
fn test_rate_client_duplicate_rejection() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);

    // First rating succeeds
    s.escrow.rate_client(&id, &5);

    // Second rating for the same escrow receives ReputationAlreadyIssued
    assert_eq!(
        s.escrow.try_rate_client(&id, &4),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
}

#[test]
fn test_rate_client_validation() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    s.escrow.release_milestone(&id, &0);

    // Rating 0 is rejected
    assert_eq!(
        s.escrow.try_rate_client(&id, &0),
        Err(Ok(EscrowError::InvalidRating))
    );

    // Rating 6 is rejected
    assert_eq!(
        s.escrow.try_rate_client(&id, &6),
        Err(Ok(EscrowError::InvalidRating))
    );

    // Valid rating 1..=5 accepted
    assert_eq!(s.escrow.try_rate_client(&id, &3), Ok(Ok(())));
    let rep = s.escrow.get_client_reputation(&s.client);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 3);
}

#[test]
fn test_rate_client_status_rejections() {
    let s = setup();

    // 1. Escrow in Funded status rejected
    let id_funded = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id_funded, &(100 * U));
    assert_eq!(
        s.escrow.get_contract(&id_funded).status,
        ContractStatus::Funded
    );
    assert_eq!(
        s.escrow.try_rate_client(&id_funded, &5),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // 2. Escrow in Cancelled status rejected
    let id_cancelled = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50 * U],
    );
    s.escrow.deposit_funds(&id_cancelled, &(50 * U));
    s.escrow.cancel_contract(&id_cancelled);
    assert_eq!(
        s.escrow.get_contract(&id_cancelled).status,
        ContractStatus::Cancelled
    );
    assert_eq!(
        s.escrow.try_rate_client(&id_cancelled, &5),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn test_reputation_independence() {
    let s = setup();

    // Contract 1: Client issues reputation first, then freelancer rates client
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id1, &(100 * U));
    s.escrow.release_milestone(&id1, &0);

    // Client rates freelancer
    s.escrow.issue_reputation(&id1, &5);
    let fl_rep1 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(fl_rep1.completed_contracts, 1);
    assert_eq!(fl_rep1.total_rating, 5);

    // Freelancer rates client afterwards; client-rating is not blocked
    s.escrow.rate_client(&id1, &4);
    let cl_rep1 = s.escrow.get_client_reputation(&s.client);
    assert_eq!(cl_rep1.completed_contracts, 1);
    assert_eq!(cl_rep1.total_rating, 4);

    // Contract 2: Freelancer rates client first, then client rates freelancer
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id2, &(100 * U));
    s.escrow.release_milestone(&id2, &0);

    // Freelancer rates client first
    s.escrow.rate_client(&id2, &5);
    let cl_rep2 = s.escrow.get_client_reputation(&s.client);
    assert_eq!(cl_rep2.completed_contracts, 2);
    assert_eq!(cl_rep2.total_rating, 9);

    // Client rates freelancer afterwards; freelancer-rating is not blocked
    s.escrow.issue_reputation(&id2, &4);
    let fl_rep2 = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(fl_rep2.completed_contracts, 2);
    assert_eq!(fl_rep2.total_rating, 9);

    // Duplicate client rating on id2 rejected
    assert_eq!(
        s.escrow.try_rate_client(&id2, &5),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
    // Duplicate freelancer rating on id2 rejected
    assert_eq!(
        s.escrow.try_issue_reputation(&id2, &4),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );
}

#[test]
fn dust_release_rating_returns_below_threshold() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    open_dispute_bonded(&s, &id, &s.client);
    // 1 bps of the held balance reaches the freelancer: far below the threshold
    resolve_refunding_bond(&s, &id, &1);
    assert!(s.escrow.get_contract(&id).released_amount < MIN_REPUTATION_VALUE);
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::BelowReputationThreshold))
    );
    assert_eq!(s.escrow.get_reputation(&s.freelancer), Reputation::default());
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), None);
}

#[test]
fn weighted_score_large_and_small_job() {
    let s = setup();
    // 1 token unit = 10 * U; a 5-star 1,000-unit job and a 1-star 10-unit job.
    let unit = MIN_REPUTATION_VALUE;
    let big = completed_escrow(&s, 1_000 * unit);
    let small = completed_escrow(&s, 10 * unit);
    s.escrow.issue_reputation(&big, &5);
    s.escrow.issue_reputation(&small, &1);
    // (5 * 1000 + 1 * 10) / 1010 = 4.9603 -> 496
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), Some(496));
    let w = s.escrow.get_weighted_reputation(&s.freelancer);
    assert_eq!(w.rated_contracts, 2);
    assert_eq!(w.weight_total, 1_010 * unit);
    assert_eq!(w.weighted_rating_sum, 5_010 * unit);
    // Legacy reputation keeps incrementing as before
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 2);
    assert_eq!(rep.total_rating, 6);
}

#[test]
fn weighted_score_rounds_half_up() {
    let s = setup();
    let unit = MIN_REPUTATION_VALUE;
    let a = completed_escrow(&s, 2 * unit);
    let b = completed_escrow(&s, unit);
    s.escrow.issue_reputation(&a, &5);
    s.escrow.issue_reputation(&b, &4);
    // (5 * 2 + 4 * 1) / 3 = 4.6667 -> 467 (truncation would give 466)
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), Some(467));
}

#[test]
fn reputation_score_is_overflow_safe() {
    assert_eq!(reputation_score(i128::MAX, i128::MAX), Some(100));
    assert_eq!(reputation_score(5 * (i128::MAX / 5), i128::MAX / 5), Some(500));
    let near = reputation_score(i128::MAX, i128::MAX / 2 + 1).unwrap();
    assert!((199..=200).contains(&near));
    assert_eq!(reputation_score(0, 0), None);
    // A rating at the maximum escrow value does not overflow the contract path
    let s = setup();
    StellarAssetClient::new(&s.env, &s.token.address).mint(&s.client, &MAX_ESCROW_TOTAL);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, MAX_ESCROW_TOTAL],
    );
    s.escrow.deposit_funds(&id, &MAX_ESCROW_TOTAL);
    s.escrow.release_milestone(&id, &0);
    s.escrow.issue_reputation(&id, &5);
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), Some(500));
}

#[test]
fn voiding_rating_reverses_weighted_reputation() {
    let s = setup();
    let unit = MIN_REPUTATION_VALUE;
    let a = completed_escrow(&s, 1_000 * unit);
    let b = completed_escrow(&s, 10 * unit);
    s.escrow.issue_reputation(&a, &5);
    s.escrow.issue_reputation(&b, &1);
    s.escrow.void_rating(&a, &symbol_short!("fraud"));
    assert_eq!(s.escrow.get_reputation_score(&s.freelancer), Some(100));
    let w = s.escrow.get_weighted_reputation(&s.freelancer);
    assert_eq!(w.rated_contracts, 1);
    assert_eq!(w.weight_total, 10 * unit);
}

#[test]
fn rep_event_includes_weight() {
    let s = setup();
    let id = completed_escrow(&s, 20 * U);
    s.escrow.issue_reputation(&id, &4);
    // Structured since #1712: topics (1, "rep", id), payload ReputationEvent.
    let ev: ReputationEvent = last_escrow_event(&s, "rep", id).unwrap();
    assert_eq!(ev.freelancer, s.freelancer);
    assert_eq!(ev.rating, 4);
    assert_eq!(ev.weight, 20 * U);
}
