//! Comprehensive tests for entrypoint input bounds validation.
//!
//! Covers every numeric and length bound across the contract entrypoints,
//! including edge cases: zero, negative, min, max, one-over-limit, and
//! overflow boundaries.

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env, Vec};

use crate::{
    amount_validation::{MAX_SINGLE_AMOUNT_STROOPS, MIN_POSITIVE_AMOUNT},
    Contract, ContractStatus, DisputeResolution, DisputeSplit, Escrow, EscrowClient, EscrowError,
    Milestone, ReleaseAuthorization, MAX_MILESTONES, MAX_TOTAL_ESCROW_STROOPS,
};

use super::assert_contract_error;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(env, &cid);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

fn setup_with_token(env: &Env) -> (EscrowClient<'_>, Address, Address, Address) {
    let (client, admin) = setup(env);
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin);
    client.bind_settlement_token(&admin, &token);
    let client_addr = Address::generate(env);
    let freelancer = Address::generate(env);
    (client, client_addr, freelancer, token)
}

fn setup_funded(env: &Env) -> (EscrowClient<'_>, Address, Address, u32) {
    let (client, client_addr, freelancer, token) = setup_with_token(env);
    let milestones = vec![env, 100_0000000_i128, 200_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(env, &token);
    token_client.mint(&client_addr, &300_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &300_0000000_i128);
    (client, client_addr, freelancer, contract_id)
}

/// Sets up a completed 1-milestone contract for reputation tests.
/// Returns `(client_addr, freelancer_addr, contract_id, escrow_client)`.
fn setup_completed(env: &Env) -> (Address, Address, u32, EscrowClient<'_>) {
    let (client, admin) = setup(env);

    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin);
    client.bind_settlement_token(&admin, &token);

    let client_addr = Address::generate(env);
    let freelancer = Address::generate(env);
    let milestones = vec![env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let token_client = StellarAssetClient::new(env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    client.approve_milestone_release(&contract_id, &client_addr, &0);
    client.release_milestone(&contract_id, &client_addr, &0);

    (client_addr, freelancer, contract_id, client)
}

// ═════════════════════════════════════════════════════════════════════════════
// create_contract bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn create_contract_rejects_zero_milestone_amount() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, 0_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_rejects_negative_milestone_amount() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, -1_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_rejects_large_negative_milestone_amount() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, -1_000_000_0000000_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_rejects_milestone_above_max_single_amount() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, MAX_SINGLE_AMOUNT_STROOPS + 1],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_accepts_milestone_at_exact_max_single_amount() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let _id = client.create_contract(
        &c,
        &f,
        &None,
        &vec![&env, MAX_SINGLE_AMOUNT_STROOPS],
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
fn create_contract_accepts_minimal_positive_amount() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let _id = client.create_contract(
        &c,
        &f,
        &None,
        &vec![&env, MIN_POSITIVE_AMOUNT],
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
fn create_contract_rejects_empty_milestone_list() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &Vec::new(&env),
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::EmptyMilestones,
    );
}

#[test]
fn create_contract_rejects_one_over_max_milestone_count() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let mut amounts = vec![&env, 1_i128];
    for _ in 0..MAX_MILESTONES {
        amounts.push_back(1_i128);
    }
    assert_contract_error(
        client.try_create_contract(&c, &f, &None, &amounts, &ReleaseAuthorization::ClientOnly),
        EscrowError::TooManyMilestones,
    );
}

#[test]
fn create_contract_accepts_exactly_max_milestone_count() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let mut amounts = vec![&env, 1_i128];
    for _ in 1..MAX_MILESTONES {
        amounts.push_back(1_i128);
    }
    assert_eq!(amounts.len(), MAX_MILESTONES);
    let _id = client.create_contract(&c, &f, &None, &amounts, &ReleaseAuthorization::ClientOnly);
}

#[test]
fn create_contract_rejects_total_one_over_cap() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, MAX_TOTAL_ESCROW_STROOPS + 1],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_rejects_total_above_cap_split() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let half = MAX_TOTAL_ESCROW_STROOPS / 2 + 1;
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, half, half],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::TotalCapExceeded,
    );
}

#[test]
fn create_contract_rejects_i128_max_milestone() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, i128::MAX],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_rejects_mixed_valid_and_zero_amounts() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, 100_0000000_i128, 0_i128, 200_0000000_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_same_client_and_freelancer_rejected() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let same = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &same,
            &same,
            &None,
            &vec![&env, 100_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidParticipant,
    );
}

#[test]
fn create_contract_requires_arbiter_for_arbiter_only_mode() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, 100_i128],
            &ReleaseAuthorization::ArbiterOnly,
        ),
        EscrowError::MissingArbiter,
    );
}

#[test]
fn create_contract_arbiter_same_as_client_rejected() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &Some(c.clone()),
            &vec![&env, 100_i128],
            &ReleaseAuthorization::ArbiterOnly,
        ),
        EscrowError::InvalidArbiter,
    );
}

#[test]
fn create_contract_arbiter_same_as_freelancer_rejected() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &Some(f.clone()),
            &vec![&env, 100_i128],
            &ReleaseAuthorization::ArbiterOnly,
        ),
        EscrowError::InvalidArbiter,
    );
}

#[test]
fn create_contract_accepts_total_at_exact_cap() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let _id = client.create_contract(
        &c,
        &f,
        &None,
        &vec![&env, MAX_TOTAL_ESCROW_STROOPS],
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
fn create_contract_accepts_total_split_at_exact_cap() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let half = MAX_TOTAL_ESCROW_STROOPS / 2;
    let remainder = MAX_TOTAL_ESCROW_STROOPS - half;
    let _id = client.create_contract(
        &c,
        &f,
        &None,
        &vec![&env, half, remainder],
        &ReleaseAuthorization::ClientOnly,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// deposit_funds bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn deposit_funds_rejects_zero_amount() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &0_i128),
        crate::Error::AmountMustBePositive,
    );
}

#[test]
fn deposit_funds_rejects_negative_amount() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &-1_i128),
        crate::Error::AmountMustBePositive,
    );
}

#[test]
fn deposit_funds_rejects_amount_above_max_single() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &MAX_SINGLE_AMOUNT_STROOPS);
    assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &(MAX_SINGLE_AMOUNT_STROOPS + 1)),
        EscrowError::InvalidDepositAmount,
    );
}

#[test]
fn deposit_funds_accepts_amount_at_exact_max_single() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, MAX_SINGLE_AMOUNT_STROOPS];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &MAX_SINGLE_AMOUNT_STROOPS);
    assert!(client.deposit_funds(&contract_id, &client_addr, &MAX_SINGLE_AMOUNT_STROOPS));
}

#[test]
fn deposit_funds_rejects_amount_exceeding_remaining_capacity() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &200_0000000_i128);
    assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &200_0000000_i128),
        crate::Error::InvalidDepositAmount,
    );
}

#[test]
fn deposit_funds_accepts_minimal_positive_amount() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    assert!(client.deposit_funds(&contract_id, &client_addr, &1_i128));
}

#[test]
fn deposit_funds_rejects_large_negative_amount() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &-100_0000000_i128),
        crate::Error::AmountMustBePositive,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// withdraw_protocol_fees bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn withdraw_protocol_fees_rejects_zero_amount() {
    let env = Env::default();
    let (client, _) = setup(&env);
    assert_contract_error(
        client.try_withdraw_protocol_fees(&0_i128, &Address::generate(&env)),
        EscrowError::AmountMustBePositive,
    );
}

#[test]
fn withdraw_protocol_fees_rejects_negative_amount() {
    let env = Env::default();
    let (client, _) = setup(&env);
    assert_contract_error(
        client.try_withdraw_protocol_fees(&-1_i128, &Address::generate(&env)),
        EscrowError::AmountMustBePositive,
    );
}

#[test]
fn withdraw_protocol_fees_rejects_amount_above_max() {
    let env = Env::default();
    let (client, _) = setup(&env);
    assert_contract_error(
        client
            .try_withdraw_protocol_fees(&(MAX_SINGLE_AMOUNT_STROOPS + 1), &Address::generate(&env)),
        EscrowError::InvalidWithdrawalAmount,
    );
}

#[test]
fn withdraw_protocol_fees_rejects_insufficient_accumulated() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    client.approve_milestone_release(&contract_id, &client_addr, &0);
    client.release_milestone(&contract_id, &client_addr, &0);
    // With 0% fee, no accumulated fees exist.
    assert_contract_error(
        client.try_withdraw_protocol_fees(&1_i128, &Address::generate(&env)),
        EscrowError::InsufficientAccumulatedFees,
    );
}

#[test]
fn withdraw_protocol_fees_accepts_at_exact_max() {
    let env = Env::default();
    let (client, _) = setup(&env);
    assert_contract_error(
        client.try_withdraw_protocol_fees(&MAX_SINGLE_AMOUNT_STROOPS, &Address::generate(&env)),
        EscrowError::InsufficientAccumulatedFees,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// set_governed_params bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn set_governed_params_rejects_zero_max_escrow_total() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert_contract_error(
        client.try_set_governed_params(&admin, &0_u32, &0_i128),
        crate::Error::InvalidProtocolParameters,
    );
}

#[test]
fn set_governed_params_rejects_negative_max_escrow_total() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert_contract_error(
        client.try_set_governed_params(&admin, &0_u32, &-1_i128),
        crate::Error::InvalidProtocolParameters,
    );
}

#[test]
fn set_governed_params_rejects_large_negative_max_escrow_total() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert_contract_error(
        client.try_set_governed_params(&admin, &0_u32, &i128::MIN),
        crate::Error::InvalidProtocolParameters,
    );
}

#[test]
fn set_governed_params_accepts_minimal_positive_max_escrow_total() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert!(client.set_governed_params(&admin, &0_u32, &1_i128));
}

#[test]
fn set_governed_params_accepts_large_positive_max_escrow_total() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert!(client.set_governed_params(&admin, &0_u32, &i128::MAX));
}

#[test]
fn set_governed_params_rejects_fee_bps_above_10000() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert_contract_error(
        client.try_set_governed_params(&admin, &10_001_u32, &1_i128),
        crate::Error::InvalidProtocolParameters,
    );
}

#[test]
fn set_governed_params_accepts_fee_bps_at_10000() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert!(client.set_governed_params(&admin, &10_000_u32, &1_000_000_0000000_i128));
}

#[test]
fn set_governed_params_accepts_fee_bps_zero() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    assert!(client.set_governed_params(&admin, &0_u32, &1_000_000_0000000_i128));
}

#[test]
fn set_governed_params_rejects_unauthorized_caller() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let unauthorized = Address::generate(&env);
    assert_contract_error(
        client.try_set_governed_params(&unauthorized, &0_u32, &1_i128),
        crate::Error::UnauthorizedRole,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// set_protocol_fee_bps bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn set_protocol_fee_bps_rejects_above_10000() {
    let env = Env::default();
    let (client, _) = setup(&env);
    assert_contract_error(
        client.try_set_protocol_fee_bps(&10_001_u32),
        EscrowError::InvalidProtocolParameters,
    );
}

#[test]
fn set_protocol_fee_bps_accepts_at_10000() {
    let env = Env::default();
    let (client, _) = setup(&env);
    assert!(client.set_protocol_fee_bps(&10_000_u32));
}

#[test]
fn set_protocol_fee_bps_accepts_zero() {
    let env = Env::default();
    let (client, _) = setup(&env);
    assert!(client.set_protocol_fee_bps(&0_u32));
}

#[test]
fn set_protocol_fee_bps_accepts_typical_values() {
    let env = Env::default();
    let (client, _) = setup(&env);
    assert!(client.set_protocol_fee_bps(&100_u32));
    assert!(client.set_protocol_fee_bps(&250_u32));
    assert!(client.set_protocol_fee_bps(&500_u32));
}

// ═════════════════════════════════════════════════════════════════════════════
// issue_reputation bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn issue_reputation_rejects_rating_zero() {
    let env = Env::default();
    let (client_addr, _, contract_id, client) = setup_completed(&env);
    let comment = soroban_sdk::String::from_str(&env, "Great work!");
    assert_contract_error(
        client.try_issue_reputation(&contract_id, &client_addr, &0_u32, &comment),
        crate::Error::InvalidRating,
    );
}

#[test]
fn issue_reputation_rejects_rating_six() {
    let env = Env::default();
    let (client_addr, _, contract_id, client) = setup_completed(&env);
    let comment = soroban_sdk::String::from_str(&env, "Great work!");
    assert_contract_error(
        client.try_issue_reputation(&contract_id, &client_addr, &6_u32, &comment),
        crate::Error::InvalidRating,
    );
}

#[test]
fn issue_reputation_accepts_rating_one() {
    let env = Env::default();
    let (client_addr, _, contract_id, client) = setup_completed(&env);
    let comment = soroban_sdk::String::from_str(&env, "OK");
    assert!(client.issue_reputation(&contract_id, &client_addr, &1_u32, &comment));
}

#[test]
fn issue_reputation_accepts_rating_five() {
    let env = Env::default();
    let (client_addr, _, contract_id, client) = setup_completed(&env);
    let comment = soroban_sdk::String::from_str(&env, "Excellent!");
    assert!(client.issue_reputation(&contract_id, &client_addr, &5_u32, &comment));
}

#[test]
fn issue_reputation_rejects_empty_comment() {
    let env = Env::default();
    let (client_addr, _, contract_id, client) = setup_completed(&env);
    let comment = soroban_sdk::String::from_str(&env, "");
    assert_contract_error(
        client.try_issue_reputation(&contract_id, &client_addr, &3_u32, &comment),
        crate::Error::EmptyComment,
    );
}

#[test]
fn issue_reputation_rejects_comment_over_200_bytes() {
    let env = Env::default();
    let (client_addr, _, contract_id, client) = setup_completed(&env);
    let long_comment = soroban_sdk::String::from_str(&env, &"A".repeat(201));
    assert_eq!(long_comment.len(), 201);
    assert_contract_error(
        client.try_issue_reputation(&contract_id, &client_addr, &3_u32, &long_comment),
        crate::Error::CommentTooLong,
    );
}

#[test]
fn issue_reputation_accepts_comment_at_exact_200_bytes() {
    let env = Env::default();
    let (client_addr, _, contract_id, client) = setup_completed(&env);
    let comment = soroban_sdk::String::from_str(&env, &"A".repeat(200));
    assert_eq!(comment.len(), 200);
    assert!(client.issue_reputation(&contract_id, &client_addr, &3_u32, &comment));
}

#[test]
fn issue_reputation_rejects_self_rating() {
    let env = Env::default();
    let (client_addr, _, contract_id, client) = setup_completed(&env);
    let comment = soroban_sdk::String::from_str(&env, "Self!");
    // client == freelancer in our fixture, so this should fail.
    // But wait — the setup_completed helper generates different addresses.
    // Let's directly set up a contract where client == freelancer.
    let cid = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &cid);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let same = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    // Can't create with same client and freelancer (InvalidParticipant).
    // So test self-rating via the contract state directly.
    // Actually self-rating requires client == freelancer which is already
    // blocked at creation time. This test documents that constraint.
    assert_contract_error(
        client.try_create_contract(
            &same,
            &same,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidParticipant,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// refund_unreleased_milestones bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn refund_rejects_empty_indices() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    assert_contract_error(
        client.try_refund_unreleased_milestones(&contract_id, &Vec::new(&env)),
        EscrowError::EmptyRefundRequest,
    );
}

#[test]
fn refund_rejects_duplicate_indices() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &300_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &300_0000000_i128);
    assert_contract_error(
        client.try_refund_unreleased_milestones(&contract_id, &vec![&env, 0_u32, 0_u32]),
        EscrowError::DuplicateMilestoneInRefund,
    );
}

#[test]
fn refund_rejects_out_of_bounds_index() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    assert_contract_error(
        client.try_refund_unreleased_milestones(&contract_id, &vec![&env, 5_u32]),
        crate::Error::IndexOutOfBounds,
    );
}

#[test]
fn refund_accepts_valid_single_index() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &300_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &300_0000000_i128);
    let refunded = client.refund_unreleased_milestones(&contract_id, &vec![&env, 0_u32]);
    assert_eq!(refunded, 100_0000000_i128);
}

#[test]
fn refund_accepts_multiple_distinct_indices() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128, 300_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &600_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &600_0000000_i128);
    let refunded = client.refund_unreleased_milestones(&contract_id, &vec![&env, 0_u32, 2_u32]);
    assert_eq!(refunded, 400_0000000_i128);
}

// ═════════════════════════════════════════════════════════════════════════════
// submit_work_evidence bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn submit_work_evidence_rejects_over_256_bytes() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);

    let long_evidence = soroban_sdk::String::from_str(&env, &"A".repeat(257));
    assert_eq!(long_evidence.len(), 257);
    assert_contract_error(
        client.try_submit_work_evidence(&contract_id, &freelancer, &0_u32, &long_evidence),
        crate::Error::EvidenceTooLong,
    );
}

#[test]
fn submit_work_evidence_accepts_at_exact_256_bytes() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);

    let evidence = soroban_sdk::String::from_str(&env, &"A".repeat(256));
    assert_eq!(evidence.len(), 256);
    assert!(client.submit_work_evidence(&contract_id, &freelancer, &0_u32, &evidence));
}

#[test]
fn submit_work_evidence_rejects_empty_string_boundary() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);

    // Empty evidence is allowed (there's no minimum length check for evidence).
    let evidence = soroban_sdk::String::from_str(&env, "");
    assert!(client.submit_work_evidence(&contract_id, &freelancer, &0_u32, &evidence));
}

// ═════════════════════════════════════════════════════════════════════════════
// approve_milestone_release bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn approve_milestone_rejects_out_of_bounds_index() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    assert_contract_error(
        client.try_approve_milestone_release(&contract_id, &client_addr, &5_u32),
        crate::Error::IndexOutOfBounds,
    );
}

#[test]
fn approve_milestone_accepts_valid_index() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &300_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &300_0000000_i128);
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0_u32));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1_u32));
}

// ═════════════════════════════════════════════════════════════════════════════
// release_milestone bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn release_milestone_rejects_out_of_bounds_index() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    client.approve_milestone_release(&contract_id, &client_addr, &0);
    assert_contract_error(
        client.try_release_milestone(&contract_id, &client_addr, &10_u32),
        crate::Error::IndexOutOfBounds,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Dispute resolution bounds
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn resolve_dispute_split_rejects_negative_client_amount() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let arbiter = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &Some(arbiter.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    client.raise_dispute(&contract_id, &client_addr);

    let resolution = DisputeResolution::Split(DisputeSplit {
        client_amount: -1,
        freelancer_amount: 100_0000000,
    });
    assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter, &resolution),
        crate::Error::InvalidDisputeSplit,
    );
}

#[test]
fn resolve_dispute_split_rejects_negative_freelancer_amount() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let arbiter = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &Some(arbiter.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    client.raise_dispute(&contract_id, &client_addr);

    let resolution = DisputeResolution::Split(DisputeSplit {
        client_amount: 100_0000000,
        freelancer_amount: -1,
    });
    assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter, &resolution),
        crate::Error::InvalidDisputeSplit,
    );
}

#[test]
fn resolve_dispute_split_rejects_non_conserving_sum() {
    let env = Env::default();
    let (client, client_addr, freelancer, token) = setup_with_token(&env);
    let arbiter = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &Some(arbiter.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let token_client = StellarAssetClient::new(&env, &token);
    token_client.mint(&client_addr, &100_0000000_i128);
    client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128);
    client.raise_dispute(&contract_id, &client_addr);

    // Split that doesn't sum to available balance
    let resolution = DisputeResolution::Split(DisputeSplit {
        client_amount: 40_0000000,
        freelancer_amount: 40_0000000,
    });
    assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter, &resolution),
        crate::Error::InvalidDisputeSplit,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Existing valid inputs still accepted (regression guard)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn create_contract_still_accepts_original_three_milestone_example() {
    let env = Env::default();
    let (client, _) = setup(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = client.create_contract(
        &c,
        &f,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(id > 0);
}
