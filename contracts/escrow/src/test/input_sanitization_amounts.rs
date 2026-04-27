//! Comprehensive tests for amount validation and input sanitization
//! 
//! Tests all money-like values for positivity, max bounds, and stroop precision rules.

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, EscrowError, MAX_TOTAL_ESCROW_STROOPS,
    validate_single_amount, validate_milestone_amounts, validate_deposit_amount,
    safe_add_amounts, safe_subtract_amounts, AmountValidationError};

fn setup() -> (Env, EscrowClient, Address, Address) {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);
    (env, client, hiring_party, service_provider)
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_is_zero() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 0_i128];
    client.create_contract(&hiring_party, &service_provider, &milestones);
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_is_negative() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, -1_i128];
    client.create_contract(&hiring_party, &service_provider, &milestones);
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_any_milestone_is_non_positive() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128, 0_i128, 200_0000000_i128];
    client.create_contract(&hiring_party, &service_provider, &milestones);
}

#[test]
fn test_create_contract_accepts_all_positive_milestones() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128, 1_i128, 999_0000000_i128];
    let id = client.create_contract(&hiring_party, &service_provider, &milestones);
    assert!(id > 0);
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_total_exceeds_maximum() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 600_000_0000000_i128, 500_000_0000000_i128]; // 6M + 5M > 1M max
    client.create_contract(&hiring_party, &service_provider, &milestones);
}

#[test]
#[should_panic]
fn test_deposit_funds_panics_on_zero_amount() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(&hiring_party, &service_provider, &milestones);
    client.deposit_funds(&contract_id, &0_i128);
}

#[test]
#[should_panic]
fn test_deposit_funds_panics_on_negative_amount() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128];
    let contract_id = client.create_contract(&hiring_party, &service_provider, &milestones);
    client.deposit_funds(&contract_id, &-100_0000000_i128);
}

#[test]
#[should_panic]
fn test_deposit_funds_panics_when_exceeding_contract_maximum() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 500_0000000_i128];
    let contract_id = client.create_contract(&hiring_party, &service_provider, &milestones);
    client.deposit_funds(&contract_id, &1_000_000_0000000_i128); // 1M tokens > remaining capacity
}

#[test]
fn test_deposit_funds_accepts_valid_amounts() {
    let (env, client, hiring_party, service_provider) = setup();
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let contract_id = client.create_contract(&hiring_party, &service_provider, &milestones);
    
    // Valid deposit
    assert!(client.deposit_funds(&contract_id, &100_0000000_i128));
    
    // Another valid deposit within remaining capacity
    assert!(client.deposit_funds(&contract_id, &200_0000000_i128));
}

#[test]
fn test_single_amount_validation() {
    // Valid amounts
    assert!(validate_single_amount(1).is_ok()); // Minimum positive
    assert!(validate_single_amount(100_0000000).is_ok()); // 1 token
    assert!(validate_single_amount(1_000_000_0000000).is_ok()); // Max single amount

    // Invalid amounts
    assert_eq!(validate_single_amount(0), Err(AmountValidationError::NonPositiveAmount));
    assert_eq!(validate_single_amount(-1), Err(AmountValidationError::NonPositiveAmount));
    assert_eq!(validate_single_amount(-100_0000000), Err(AmountValidationError::NonPositiveAmount));
    assert_eq!(
        validate_single_amount(1_000_000_0000001), 
        Err(AmountValidationError::AmountExceedsMaximum)
    );
}

#[test]
fn test_milestone_amounts_validation() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Valid milestone arrays
    let milestones1 = vec![100_0000000, 200_0000000, 300_0000000];
    assert!(validate_milestone_amounts(&milestones1, max_total).is_ok());
    assert_eq!(validate_milestone_amounts(&milestones1, max_total).unwrap(), 600_0000000);

    // Single milestone at maximum
    let milestones2 = vec![max_total];
    assert!(validate_milestone_amounts(&milestones2, max_total).is_ok());

    // Multiple milestones within bounds
    let milestones3 = vec![500_000_0000000, 500_000_0000000];
    assert!(validate_milestone_amounts(&milestones3, max_total).is_ok());

    // Invalid arrays
    let milestones4 = vec![100_0000000, 0, 300_0000000]; // Contains zero
    assert_eq!(validate_milestone_amounts(&milestones4, max_total), Err(AmountValidationError::NonPositiveAmount));

    let milestones5 = vec![100_0000000, -50_0000000, 300_0000000]; // Contains negative
    assert_eq!(validate_milestone_amounts(&milestones5, max_total), Err(AmountValidationError::NonPositiveAmount));

    let milestones6 = vec![600_000_0000000, 500_000_0000000]; // Exceeds contract max
    assert_eq!(validate_milestone_amounts(&milestones6, max_total), Err(AmountValidationError::ExceedsContractMaximum));
}

#[test]
fn test_deposit_amount_validation() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Valid deposits
    assert!(validate_deposit_amount(100_0000000, 0, max_total).is_ok());
    assert!(validate_deposit_amount(100_0000000, 500_0000000, max_total).is_ok());
    assert!(validate_deposit_amount(max_total, 0, max_total).is_ok());

    // Invalid deposits
    assert_eq!(validate_deposit_amount(0, 0, max_total), Err(AmountValidationError::NonPositiveAmount));
    assert_eq!(validate_deposit_amount(-1, 0, max_total), Err(AmountValidationError::NonPositiveAmount));

    // Would exceed maximum
    assert_eq!(
        validate_deposit_amount(600_000_0000000, 500_000_0000000, max_total),
        Err(AmountValidationError::ExceedsContractMaximum)
    );

    // Single amount exceeds maximum
    assert_eq!(
        validate_deposit_amount(1_000_000_0000001, 0, max_total),
        Err(AmountValidationError::AmountExceedsMaximum)
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
    assert_eq!(safe_subtract_amounts(0, 1), None);
    assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
}

#[test]
fn test_edge_cases() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Test minimum positive amounts
    assert!(validate_single_amount(1).is_ok());
    let small_milestones = vec![1, 1, 1];
    assert!(validate_milestone_amounts(&small_milestones, max_total).is_ok());

    // Test boundary values
    assert!(validate_single_amount(1_000_000_0000000).is_ok()); // Max single amount
    assert_eq!(validate_single_amount(1_000_000_0000001), Err(AmountValidationError::AmountExceedsMaximum));

    // Test contract boundary
    let boundary_milestones = vec![MAX_TOTAL_ESCROW_STROOPS];
    assert!(validate_milestone_amounts(&boundary_milestones, max_total).is_ok());

    let over_boundary_milestones = vec![MAX_TOTAL_ESCROW_STROOPS + 1];
    assert_eq!(validate_milestone_amounts(&over_boundary_milestones, max_total), Err(AmountValidationError::AmountExceedsContractMaximum));
}

#[test]
fn test_stroop_precision() {
    // All i128 values are valid stroop amounts since stroop is the smallest unit
    // This test documents the precision requirements
    let valid_stroop_amounts = vec![
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
    let mut many_milestones = Vec::new();
    for _ in 0..10 {
        many_milestones.push(100_0000000); // 1 token each
    }
    assert!(validate_milestone_amounts(&many_milestones, max_total).is_ok());

    // Test overflow detection in array validation
    let mut overflow_milestones = Vec::new();
    for _ in 0..10 {
        overflow_milestones.push(200_000_0000000); // 200M tokens each
    }
    assert_eq!(validate_milestone_amounts(&overflow_milestones, max_total), Err(AmountValidationError::ExceedsContractMaximum));
}

#[test]
fn test_cumulative_deposit_validation() {
    let max_total = MAX_TOTAL_ESCROW_STROOPS;

    // Test cumulative deposit validation
    assert!(validate_deposit_amount(100_0000000, 0, max_total).is_ok());
    assert!(validate_deposit_amount(100_0000000, 100_0000000, max_total).is_ok());
    assert!(validate_deposit_amount(100_0000000, 200_0000000, max_total).is_ok());

    // Should fail when cumulative exceeds maximum
    assert_eq!(
        validate_deposit_amount(800_000_0000000, 300_000_0000000, max_total),
        Err(AmountValidationError::ExceedsContractMaximum)
    );
}
