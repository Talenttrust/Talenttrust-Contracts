//! Comprehensive tests for amount validation and input sanitization
///
/// Tests all money-like values for positivity, max bounds, and stroop precision rules.

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env};

use crate::{
    safe_add_amounts, safe_subtract_amounts, validate_deposit_amount, validate_milestone_amounts,
    validate_single_amount, Escrow, EscrowClient, EscrowError, ReleaseAuthorization,
    MAX_TOTAL_ESCROW_STROOPS,
};

fn setup(env: &Env) -> (EscrowClient<'_>, Address, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(env, &cid);
    let admin = Address::generate(env);
    client.initialize(&admin);
    client.set_governed_params(&admin, &0_u32, &MAX_TOTAL_ESCROW_STROOPS);

    let token_admin = Address::generate(env);
    let token_address = env.register_stellar_asset_contract(token_admin);
    client.bind_settlement_token(&admin, &token_address);

    let hiring_party = Address::generate(env);
    let service_provider = Address::generate(env);

    let token_client = StellarAssetClient::new(env, &token_address);
    token_client.mint(&hiring_party, &10_000_000_0000000_i128);

    (client, hiring_party, service_provider)
}

// -----------------------------------------------------------------------------
// Create contract: milestone amount validation
// -----------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_is_zero() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 0_i128];
    client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_is_negative() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, -1_i128];
    client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_any_milestone_is_non_positive() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 100_0000000_i128, 0_i128, 200_0000000_i128];
    client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
fn test_create_contract_accepts_all_positive_milestones() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 100_0000000_i128, 1_i128, 999_0000000_i128];
    let id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_total_exceeds_maximum() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 600_000_0000000_i128, 500_000_0000000_i128]; // Above the configured cap.
    client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

// -----------------------------------------------------------------------------
// Deposit funds: amount validation
// -----------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_deposit_funds_panics_on_zero_amount() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &hiring_party, &0_i128);
}

#[test]
#[should_panic]
fn test_deposit_funds_panics_on_negative_amount() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &hiring_party, &-100_0000000_i128);
}

#[test]
#[should_panic]
fn test_deposit_funds_panics_when_exceeding_contract_maximum() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 500_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &hiring_party, &1_000_000_0000000_i128); // Above remaining capacity.
}

#[test]
fn test_deposit_funds_accepts_valid_amounts() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Valid deposit
    assert!(client.deposit_funds(&contract_id, &hiring_party, &100_0000000_i128));

    // Another valid deposit within remaining capacity
    assert!(client.deposit_funds(&contract_id, &hiring_party, &200_0000000_i128));
}

#[test]
#[should_panic]
fn test_deposit_funds_rejects_amount_at_max_single_amount_plus_one() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 1_000_000_0000000_i128]; // Max total equals one max milestone.
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    // Amount just above MAX_SINGLE_AMOUNT_STROOPS must be rejected by the
    // centralized single-amount validator rather than slipping through.
    client.deposit_funds(&contract_id, &hiring_party, &(1_000_000_0000000_i128 + 1));
}

#[test]
fn test_deposit_funds_accepts_amount_exactly_at_max_single_amount() {
    let env = Env::default();
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 2_000_000_0000000_i128]; // 2M total
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    // Deposit exactly the max single amount must succeed.
    assert!(client.deposit_funds(&contract_id, &hiring_party, &1_000_000_0000000_i128));
}

// -----------------------------------------------------------------------------
// Pure validator unit tests
// -----------------------------------------------------------------------------

#[test]
fn test_single_amount_validation() {
    // Valid amounts
    assert!(validate_single_amount(1).is_ok()); // Minimum positive
    assert!(validate_single_amount(100_0000000).is_ok()); // 1 token
    assert!(validate_single_amount(1_000_000_0000000).is_ok()); // Max single amount

    // Invalid amounts
    assert_eq!(
        validate_single_amount(0),
        Err(EscrowError::AmountMustBePositive)
    );
    assert_eq!(
        validate_single_amount(-1),
        Err(EscrowError::AmountMustBePositive)
    );
    assert_eq!(
        validate_single_amount(-100_0000000),
        Err(EscrowError::AmountMustBePositive)
    );
    assert_eq!(
        validate_single_amount(1_000_000_0000001),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_milestone_amounts_validation() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Valid milestone arrays
    let milestones1 = [100_0000000, 200_0000000, 300_0000000];
    assert!(validate_milestone_amounts(&milestones1, max_total).is_ok());
    assert_eq!(
        validate_milestone_amounts(&milestones1, max_total).unwrap(),
        600_0000000
    );

    // Single milestone at maximum
    let milestones2 = [max_total];
    assert!(validate_milestone_amounts(&milestones2, max_total).is_ok());

    // Multiple milestones within bounds
    let milestones3 = [500_000_0000000, 500_000_0000000];
    assert!(validate_milestone_amounts(&milestones3, max_total).is_ok());

    // Invalid arrays
    let milestones4 = [100_0000000, 0, 300_0000000]; // Contains zero
    assert_eq!(
        validate_milestone_amounts(&milestones4, max_total),
        Err(EscrowError::AmountMustBePositive)
    );

    let milestones5 = [100_0000000, -50_0000000, 300_0000000]; // Contains negative
    assert_eq!(
        validate_milestone_amounts(&milestones5, max_total),
        Err(EscrowError::AmountMustBePositive)
    );

    let milestones6 = [600_000_0000000, 500_000_0000000]; // Exceeds contract max
    assert_eq!(
        validate_milestone_amounts(&milestones6, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_deposit_amount_validation() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Valid deposits
    assert!(validate_deposit_amount(100_0000000, 0, max_total).is_ok());
    assert!(validate_deposit_amount(100_0000000, 500_0000000, max_total).is_ok());
    assert!(validate_deposit_amount(max_total, 0, max_total).is_ok());

    // Invalid deposits
    assert_eq!(
        validate_deposit_amount(0, 0, max_total),
        Err(EscrowError::AmountMustBePositive)
    );
    assert_eq!(
        validate_deposit_amount(-1, 0, max_total),
        Err(EscrowError::AmountMustBePositive)
    );

    // Would exceed maximum
    assert_eq!(
        validate_deposit_amount(600_000_0000000, 500_000_0000000, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );

    // Single amount exceeds maximum
    assert_eq!(
        validate_deposit_amount(1_000_000_0000001, 0, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_safe_arithmetic_operations() {
    // Safe addition
    assert_eq!(safe_add_amounts(100, 200), Some(300));
    assert_eq!(safe_add_amounts(0, 0), Some(0));
    assert_eq!(safe_add_amounts(i128::MAX, 1), None);
    assert_eq!(safe_add_amounts(i128::MIN, -1), None);

    // Safe subtraction
    assert_eq!(safe_subtract_amounts(300, 100), Some(200));
    assert_eq!(safe_subtract_amounts(100, 100), Some(0));
    assert_eq!(safe_subtract_amounts(0, 1), Some(-1));
    assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
}

#[test]
fn test_edge_cases() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Test minimum positive amounts
    assert!(validate_single_amount(1).is_ok());
    let small_milestones = [1, 1, 1];
    assert!(validate_milestone_amounts(&small_milestones, max_total).is_ok());

    // Test boundary values
    assert!(validate_single_amount(1_000_000_0000000).is_ok()); // Max single amount
    assert_eq!(
        validate_single_amount(1_000_000_0000001),
        Err(EscrowError::InvalidMilestoneAmount)
    );

    // Test contract boundary
    let boundary_milestones = [MAX_TOTAL_ESCROW_STROOPS];
    assert!(validate_milestone_amounts(&boundary_milestones, max_total).is_ok());

    let over_boundary_milestones = [MAX_TOTAL_ESCROW_STROOPS + 1];
    assert_eq!(
        validate_milestone_amounts(&over_boundary_milestones, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_stroop_precision() {
    // All i128 values are valid stroop amounts since stroop is the smallest unit
    // This test documents the precision requirements
    let valid_stroop_amounts = [
        1,           // 1 stroop
        100,         // 100 stroops
        1_0000000,   // 1 token
        123_4567890, // 123.4567890 tokens
    ];

    for amount in valid_stroop_amounts {
        assert!(validate_single_amount(amount).is_ok());
    }
}

#[test]
fn test_large_amount_arrays() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Test with maximum number of milestones (10)
    let many_milestones = [100_0000000; 10]; // 1 token each
    assert!(validate_milestone_amounts(&many_milestones, max_total).is_ok());
    assert_eq!(
        validate_milestone_amounts(&many_milestones, max_total).unwrap(),
        1_000_000_000
    );

    // Array with one too many milestones must be rejected by the milestone count bound.
    let too_many_milestones = [100_0000000; 11];
    assert_eq!(
        validate_milestone_amounts(&too_many_milestones, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

// -----------------------------------------------------------------------------
// Duplicate submission and concurrency-safety tests
// -----------------------------------------------------------------------------

#[test]
fn test_duplicate_milestone_amounts_are_accepted_and_summed_once() {
    // Duplicate values in the milestone array are not duplicate submissions;
    // each entry is a distinct milestone and must be summed exactly once.
    let max_total = MAX_TOTAL_ESCROW_STROOPS;
    let milestones = [100_0000000, 100_0000000, 100_0000000];
    assert_eq!(
        validate_milestone_amounts(&milestones, max_total).unwrap(),
        300_0000000
    );
}

#[test]
fn test_duplicate_deposits_accumulate_deterministically() {
    // Repeated deposits are not rejected as duplicates; they accumulate and
    // must never exceed the contract maximum. This documents the invariant
    // that deposit validation is based on the current balance, not on a
    // per-caller duplicate tracker.
    let max_total = MAX_TOTAL_ESCROW_STROOPS;
    assert!(validate_deposit_amount(100_0000000, 0, max_total).is_ok());
    assert!(validate_deposit_amount(100_0000000, 100_0000000, max_total).is_ok());
    assert!(validate_deposit_amount(100_0000000, 200_0000000, max_total).is_ok());
    // Once the accumulated balance reaches the maximum, any further deposit
    // must be rejected deterministically.
    assert_eq!(
        validate_deposit_amount(1, 1000_000_0000000, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_concurrent_deposit_validation_is_deterministic() {
    // Validation is a pure function of (amount, current_balance, max_total).
    // Concurrent callers observe the same result for the same inputs, and the
    // contract enforces the invariant on each state transition.
    let max_total = MAX_TOTAL_ESCROW_STROOPS;
    for _ in 0..10 {
        assert!(validate_deposit_amount(100_0000000, 0, max_total).is_ok());
        assert_eq!(
            validate_deposit_amount(0, 0, max_total),
            Err(EscrowError::AmountMustBePositive)
        );
    }
}
