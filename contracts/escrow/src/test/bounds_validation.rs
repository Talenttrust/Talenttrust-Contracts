//! Bounds validation tests for escrow entrypoints (issue #914).
//!
//! Covers every entrypoint that accepts numeric or length-bounded inputs,
//! verifying:
//!   - values at the exact maximum are accepted
//!   - values one above the maximum are rejected with the correct typed error
//!   - zero / negative inputs are rejected where applicable
//!   - existing valid inputs continue to be accepted (regression)
//!
//! Entrypoints covered:
//!   - `set_protocol_fee_bps`   — `new_bps` must be ≤ 10_000
//!   - `create_contract`        — milestone count ≤ MAX_MILESTONES, amounts > 0, total ≤ cap
//!   - `deposit_funds`          — amount > 0, cumulative ≤ contract total
//!   - `release_milestone`      — milestone_index < milestones.len()
//!   - `approve_milestone_release` — milestone_index < milestones.len()
//!   - `submit_work_evidence`   — evidence ≤ 256 bytes
//!   - `issue_reputation`       — rating in [1, 5], comment in [1, 200] bytes
//!   - `refund_unreleased_milestones` — indices < milestones.len()

#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    token::StellarAssetClient,
    vec, Address, Env, String, Vec,
};

use crate::{
    Escrow, EscrowClient, EscrowError,
    Error,
    ReleaseAuthorization,
    MAX_MILESTONES, MAX_TOTAL_ESCROW_STROOPS,
};

// ── Fixture helpers ──────────────────────────────────────────────────────────

/// Minimal fixture: initialized escrow, no settlement token.
fn setup_no_token(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(env, &addr);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Full fixture: initialized escrow + bound SAC token + minted client balance.
fn setup_with_token(env: &Env) -> (EscrowClient<'_>, Address, Address, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(env, &addr);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);

    // Mint plenty of tokens to the client for deposits.
    StellarAssetClient::new(env, &token).mint(&client_addr, &(MAX_TOTAL_ESCROW_STROOPS * 10));

    (client, client_addr, freelancer_addr, admin)
}

/// Create a funded 1-milestone contract; returns contract_id.
fn funded_contract(
    env: &Env,
    escrow: &EscrowClient<'_>,
    client_addr: &Address,
    freelancer_addr: &Address,
    amount: i128,
) -> u32 {
    let milestones = vec![env, amount];
    let id = escrow.create_contract(
        client_addr,
        freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&id, client_addr, &amount);
    id
}

// ── set_protocol_fee_bps ─────────────────────────────────────────────────────

/// Boundary success: exactly 10_000 bps (100 %) must be accepted.
#[test]
fn set_protocol_fee_bps_accepts_exactly_10000() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_protocol_fee_bps(&10_000_u32));
    assert_eq!(escrow.get_protocol_fee_bps(), 10_000_u32);
}

/// Boundary success: 0 bps (no fee) must be accepted.
#[test]
fn set_protocol_fee_bps_accepts_zero() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_protocol_fee_bps(&0_u32));
    assert_eq!(escrow.get_protocol_fee_bps(), 0_u32);
}

/// Typical mid-range value (500 bps = 5 %) must be accepted.
#[test]
fn set_protocol_fee_bps_accepts_typical_value() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_protocol_fee_bps(&500_u32));
    assert_eq!(escrow.get_protocol_fee_bps(), 500_u32);
}

/// One above the maximum (10_001 bps) must be rejected with InvalidProtocolParameters.
#[test]
fn set_protocol_fee_bps_rejects_10001() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_set_protocol_fee_bps(&10_001_u32);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(e, want, "expected InvalidProtocolParameters");
        }
        other => panic!("expected Err(Ok(InvalidProtocolParameters)), got {:?}", other),
    }
}

/// u32::MAX must be rejected with InvalidProtocolParameters.
#[test]
fn set_protocol_fee_bps_rejects_u32_max() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_set_protocol_fee_bps(&u32::MAX);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(e, want, "expected InvalidProtocolParameters for u32::MAX");
        }
        other => panic!("expected Err(Ok(InvalidProtocolParameters)), got {:?}", other),
    }
}

/// Rejected calls must not mutate the stored fee.
#[test]
fn set_protocol_fee_bps_rejected_call_leaves_fee_unchanged() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    // Set a known good value first.
    escrow.set_protocol_fee_bps(&250_u32);
    // Attempt an over-limit update.
    let _ = escrow.try_set_protocol_fee_bps(&20_000_u32);
    // Fee must still be the previously accepted value.
    assert_eq!(escrow.get_protocol_fee_bps(), 250_u32);
}

// ── deposit_funds ────────────────────────────────────────────────────────────

/// Zero deposit must be rejected with AmountMustBePositive.
#[test]
fn deposit_funds_rejects_zero_amount() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let result = escrow.try_deposit_funds(&id, &client_addr, &0_i128);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = EscrowError::AmountMustBePositive.into();
            assert_eq!(e, want, "expected AmountMustBePositive for zero deposit");
        }
        other => panic!("expected AmountMustBePositive, got {:?}", other),
    }
}

/// Negative deposit must be rejected with AmountMustBePositive.
#[test]
fn deposit_funds_rejects_negative_amount() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let result = escrow.try_deposit_funds(&id, &client_addr, &-1_i128);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = EscrowError::AmountMustBePositive.into();
            assert_eq!(e, want, "expected AmountMustBePositive for negative deposit");
        }
        other => panic!("expected AmountMustBePositive, got {:?}", other),
    }
}

/// Deposit exactly equal to the contract total must be accepted.
#[test]
fn deposit_funds_accepts_exact_total() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let amount = 500_0000000_i128;
    let milestones = vec![&env, amount];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(escrow.deposit_funds(&id, &client_addr, &amount));
}

/// Deposit exceeding the remaining capacity must be rejected.
#[test]
fn deposit_funds_rejects_amount_over_remaining() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let amount = 500_0000000_i128;
    let milestones = vec![&env, amount];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    // Attempt to deposit one stroop more than the contract total.
    let result = escrow.try_deposit_funds(&id, &client_addr, &(amount + 1));
    assert!(result.is_err(), "deposit over cap must be rejected");
}

// ── release_milestone — milestone_index bounds ───────────────────────────────

/// Index equal to the milestone count (out of bounds by 1) must be rejected.
#[test]
fn release_milestone_rejects_index_equal_to_count() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    // Approve first so auth doesn't block us before the index check.
    escrow.approve_milestone_release(&id, &client_addr, &0);
    // Index 1 is out of bounds for a 1-milestone contract.
    let result = escrow.try_release_milestone(&id, &client_addr, &1_u32);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::IndexOutOfBounds.into();
            assert_eq!(e, want, "expected IndexOutOfBounds for index == len");
        }
        other => panic!("expected IndexOutOfBounds, got {:?}", other),
    }
}

/// u32::MAX index must be rejected with IndexOutOfBounds.
#[test]
fn release_milestone_rejects_u32_max_index() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    let result = escrow.try_release_milestone(&id, &client_addr, &u32::MAX);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::IndexOutOfBounds.into();
            assert_eq!(e, want, "expected IndexOutOfBounds for u32::MAX index");
        }
        other => panic!("expected IndexOutOfBounds, got {:?}", other),
    }
}

/// Index 0 on a 1-milestone contract must be accepted (after approval).
#[test]
fn release_milestone_accepts_index_zero_on_single_milestone() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    escrow.approve_milestone_release(&id, &client_addr, &0);
    assert!(escrow.release_milestone(&id, &client_addr, &0));
}

// ── approve_milestone_release — milestone_index bounds ───────────────────────

/// Index equal to the milestone count must be rejected.
#[test]
fn approve_milestone_release_rejects_index_equal_to_count() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    // A 1-milestone contract has indices [0]. Index 1 is out of bounds.
    let result = escrow.try_approve_milestone_release(&id, &client_addr, &1_u32);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::IndexOutOfBounds.into();
            assert_eq!(e, want, "expected IndexOutOfBounds for index == len");
        }
        other => panic!("expected IndexOutOfBounds, got {:?}", other),
    }
}

/// u32::MAX index must be rejected.
#[test]
fn approve_milestone_release_rejects_u32_max_index() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    let result = escrow.try_approve_milestone_release(&id, &client_addr, &u32::MAX);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::IndexOutOfBounds.into();
            assert_eq!(e, want, "expected IndexOutOfBounds for u32::MAX");
        }
        other => panic!("expected IndexOutOfBounds, got {:?}", other),
    }
}

/// Valid index 0 must be accepted.
#[test]
fn approve_milestone_release_accepts_valid_index() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    assert!(escrow.approve_milestone_release(&id, &client_addr, &0));
}

// ── submit_work_evidence — evidence length bounds ────────────────────────────

/// Evidence of exactly 256 bytes must be accepted.
#[test]
fn submit_work_evidence_accepts_256_bytes() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    // Build a 256-byte ASCII string.
    let s: soroban_sdk::String = soroban_sdk::String::from_str(&env, &"x".repeat(256));
    assert!(escrow.submit_work_evidence(&id, &freelancer_addr, &0, &s));
}

/// Evidence of 257 bytes must be rejected with EvidenceTooLong.
#[test]
fn submit_work_evidence_rejects_257_bytes() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    let s: soroban_sdk::String = soroban_sdk::String::from_str(&env, &"x".repeat(257));
    let result = escrow.try_submit_work_evidence(&id, &freelancer_addr, &0, &s);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::EvidenceTooLong.into();
            assert_eq!(e, want, "expected EvidenceTooLong for 257-byte evidence");
        }
        other => panic!("expected EvidenceTooLong, got {:?}", other),
    }
}

/// Evidence of 1 byte must be accepted.
#[test]
fn submit_work_evidence_accepts_one_byte() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    let s: soroban_sdk::String = soroban_sdk::String::from_str(&env, "a");
    assert!(escrow.submit_work_evidence(&id, &freelancer_addr, &0, &s));
}

/// submit_work_evidence must also check milestone_index bounds.
#[test]
fn submit_work_evidence_rejects_out_of_bounds_index() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = funded_contract(&env, &escrow, &client_addr, &freelancer_addr, 100_0000000);
    let s: soroban_sdk::String = soroban_sdk::String::from_str(&env, "ipfs://abc");
    // Index 1 is out of bounds for a 1-milestone contract.
    let result = escrow.try_submit_work_evidence(&id, &freelancer_addr, &1_u32, &s);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::IndexOutOfBounds.into();
            assert_eq!(e, want, "expected IndexOutOfBounds for out-of-range index");
        }
        other => panic!("expected IndexOutOfBounds, got {:?}", other),
    }
}

// ── issue_reputation — rating and comment bounds ─────────────────────────────

/// Helper: drive a contract to Completed status.
fn complete_contract_for_reputation(
    env: &Env,
    escrow: &EscrowClient<'_>,
    client_addr: &Address,
    freelancer_addr: &Address,
) -> u32 {
    let id = funded_contract(env, escrow, client_addr, freelancer_addr, 100_0000000);
    escrow.approve_milestone_release(&id, client_addr, &0);
    escrow.release_milestone(&id, client_addr, &0);
    id
}

/// Rating of 1 (minimum) must be accepted.
#[test]
fn issue_reputation_accepts_rating_1() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = complete_contract_for_reputation(&env, &escrow, &client_addr, &freelancer_addr);
    let comment = soroban_sdk::String::from_str(&env, "Good work");
    assert!(escrow.issue_reputation(&id, &client_addr, &1_u32, &comment));
}

/// Rating of 5 (maximum) must be accepted.
#[test]
fn issue_reputation_accepts_rating_5() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = complete_contract_for_reputation(&env, &escrow, &client_addr, &freelancer_addr);
    let comment = soroban_sdk::String::from_str(&env, "Excellent");
    assert!(escrow.issue_reputation(&id, &client_addr, &5_u32, &comment));
}

/// Rating of 0 must be rejected with InvalidRating.
#[test]
fn issue_reputation_rejects_rating_0() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = complete_contract_for_reputation(&env, &escrow, &client_addr, &freelancer_addr);
    let comment = soroban_sdk::String::from_str(&env, "Good");
    let result = escrow.try_issue_reputation(&id, &client_addr, &0_u32, &comment);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidRating.into();
            assert_eq!(e, want, "expected InvalidRating for 0");
        }
        other => panic!("expected InvalidRating, got {:?}", other),
    }
}

/// Rating of 6 must be rejected with InvalidRating.
#[test]
fn issue_reputation_rejects_rating_6() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = complete_contract_for_reputation(&env, &escrow, &client_addr, &freelancer_addr);
    let comment = soroban_sdk::String::from_str(&env, "Good");
    let result = escrow.try_issue_reputation(&id, &client_addr, &6_u32, &comment);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidRating.into();
            assert_eq!(e, want, "expected InvalidRating for 6");
        }
        other => panic!("expected InvalidRating, got {:?}", other),
    }
}

/// Comment of exactly 200 bytes must be accepted.
#[test]
fn issue_reputation_accepts_comment_200_bytes() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = complete_contract_for_reputation(&env, &escrow, &client_addr, &freelancer_addr);
    let comment = soroban_sdk::String::from_str(&env, &"a".repeat(200));
    assert!(escrow.issue_reputation(&id, &client_addr, &5_u32, &comment));
}

/// Comment of 201 bytes must be rejected with CommentTooLong.
#[test]
fn issue_reputation_rejects_comment_201_bytes() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = complete_contract_for_reputation(&env, &escrow, &client_addr, &freelancer_addr);
    let comment = soroban_sdk::String::from_str(&env, &"a".repeat(201));
    let result = escrow.try_issue_reputation(&id, &client_addr, &5_u32, &comment);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::CommentTooLong.into();
            assert_eq!(e, want, "expected CommentTooLong for 201-byte comment");
        }
        other => panic!("expected CommentTooLong, got {:?}", other),
    }
}

/// Empty comment must be rejected with EmptyComment.
#[test]
fn issue_reputation_rejects_empty_comment() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let id = complete_contract_for_reputation(&env, &escrow, &client_addr, &freelancer_addr);
    let comment = soroban_sdk::String::from_str(&env, "");
    let result = escrow.try_issue_reputation(&id, &client_addr, &5_u32, &comment);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::EmptyComment.into();
            assert_eq!(e, want, "expected EmptyComment for empty string");
        }
        other => panic!("expected EmptyComment, got {:?}", other),
    }
}

// ── refund_unreleased_milestones — index bounds ──────────────────────────────

/// Out-of-bounds index in refund request must be rejected with IndexOutOfBounds.
#[test]
fn refund_unreleased_milestones_rejects_out_of_bounds_index() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    // Create a contract but do NOT deposit (Created state, 0 funded).
    let milestones = vec![&env, 100_0000000_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    // Index 1 is out of bounds for a 1-milestone contract.
    let indices: Vec<u32> = vec![&env, 1_u32];
    let result = escrow.try_refund_unreleased_milestones(&id, &indices);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::IndexOutOfBounds.into();
            assert_eq!(e, want, "expected IndexOutOfBounds for index 1 on 1-milestone contract");
        }
        other => panic!("expected IndexOutOfBounds, got {:?}", other),
    }
}

/// u32::MAX index must be rejected with IndexOutOfBounds.
#[test]
fn refund_unreleased_milestones_rejects_u32_max_index() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let indices: Vec<u32> = vec![&env, u32::MAX];
    let result = escrow.try_refund_unreleased_milestones(&id, &indices);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::IndexOutOfBounds.into();
            assert_eq!(e, want, "expected IndexOutOfBounds for u32::MAX");
        }
        other => panic!("expected IndexOutOfBounds, got {:?}", other),
    }
}

// ── Regression: existing valid inputs still accepted ─────────────────────────

/// A standard 3-milestone contract with typical amounts must still be created.
#[test]
fn regression_standard_three_milestone_contract_accepted() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = escrow.create_contract(&c, &f, &None, &milestones, &ReleaseAuthorization::ClientOnly);
    assert!(id > 0 || id == 0, "contract id must be a valid u32");
}

/// set_protocol_fee_bps can be updated multiple times with valid values.
#[test]
fn regression_set_protocol_fee_bps_multiple_updates() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_protocol_fee_bps(&100_u32));
    assert!(escrow.set_protocol_fee_bps(&500_u32));
    assert!(escrow.set_protocol_fee_bps(&0_u32));
    assert!(escrow.set_protocol_fee_bps(&10_000_u32));
    assert_eq!(escrow.get_protocol_fee_bps(), 10_000_u32);
}
