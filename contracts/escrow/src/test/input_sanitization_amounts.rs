//! Comprehensive tests for amount validation and input sanitization
//!
//! Tests all money-like values for positivity, max bounds, and stroop precision rules.

use soroban_sdk::testutils::Ledger as _;
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

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_is_zero() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 600_000_0000000_i128, 500_000_0000000_i128]; // 6M + 5M > 1M max
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
fn test_deposit_funds_panics_on_zero_amount() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    let (client, hiring_party, service_provider) = setup(&env);
    let milestones = vec![&env, 500_0000000_i128];
    let contract_id = client.create_contract(
        &hiring_party,
        &service_provider,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &hiring_party, &1_000_000_0000000_i128); // 1M tokens > remaining capacity
}

#[test]
fn test_deposit_funds_accepts_valid_amounts() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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

    // Test overflow detection in array validation
    let overflow_milestones = [200_000_0000000; 10]; // 200M tokens each
    assert_eq!(
        validate_milestone_amounts(&overflow_milestones, max_total),
        Err(EscrowError::InvalidMilestoneAmount)
    );
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
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

#[test]
fn test_validate_deposit_amount_boundaries_table_driven() {
    struct TestCase {
        name: &'static str,
        deposit_amount: i128,
        current_deposited: i128,
        max_contract_total: i128,
        expected: Result<(), EscrowError>,
    }

    let test_cases = [
        // Zero and negative amounts
        TestCase {
            name: "zero deposit amount should fail with AmountMustBePositive",
            deposit_amount: 0,
            current_deposited: 0,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::AmountMustBePositive),
        },
        TestCase {
            name: "negative deposit amount should fail with AmountMustBePositive",
            deposit_amount: -1,
            current_deposited: 0,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::AmountMustBePositive),
        },
        TestCase {
            name: "large negative deposit should fail with AmountMustBePositive",
            deposit_amount: -100_0000000,
            current_deposited: 500_0000000,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::AmountMustBePositive),
        },
        // Exactly-remaining capacity
        TestCase {
            name: "deposit exactly remaining capacity should succeed",
            deposit_amount: 500_0000000,
            current_deposited: 500_0000000,
            max_contract_total: 1_000_0000000,
            expected: Ok(()),
        },
        TestCase {
            name: "deposit entire contract total when nothing deposited should succeed",
            deposit_amount: 1_000_0000000,
            current_deposited: 0,
            max_contract_total: 1_000_0000000,
            expected: Ok(()),
        },
        TestCase {
            name: "deposit exactly one stroop to fill contract should succeed",
            deposit_amount: 1,
            current_deposited: 999_9999999,
            max_contract_total: 1_000_0000000,
            expected: Ok(()),
        },
        // One stroop under remaining capacity
        TestCase {
            name: "deposit one stroop under remaining capacity should succeed",
            deposit_amount: 499_9999999,
            current_deposited: 500_0000000,
            max_contract_total: 1_000_0000000,
            expected: Ok(()),
        },
        TestCase {
            name: "deposit leaves one stroop remaining should succeed",
            deposit_amount: 999_9999999,
            current_deposited: 0,
            max_contract_total: 1_000_0000000,
            expected: Ok(()),
        },
        // One stroop over remaining capacity
        TestCase {
            name: "deposit one stroop over remaining should fail with InvalidMilestoneAmount",
            deposit_amount: 500_0000001,
            current_deposited: 500_0000000,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::InvalidMilestoneAmount),
        },
        TestCase {
            name: "deposit one stroop over total when nothing deposited should fail",
            deposit_amount: 1_000_0000001,
            current_deposited: 0,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::InvalidMilestoneAmount),
        },
        TestCase {
            name: "deposit two stroops when one remaining should fail",
            deposit_amount: 2,
            current_deposited: 999_9999999,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::InvalidMilestoneAmount),
        },
        // Already fully funded contract
        TestCase {
            name: "any deposit when fully funded should fail with InvalidMilestoneAmount",
            deposit_amount: 1,
            current_deposited: 1_000_0000000,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::InvalidMilestoneAmount),
        },
        TestCase {
            name: "large deposit when fully funded should fail with InvalidMilestoneAmount",
            deposit_amount: 500_0000000,
            current_deposited: 1_000_0000000,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::InvalidMilestoneAmount),
        },
        TestCase {
            name: "deposit when over-funded should fail with InvalidMilestoneAmount",
            deposit_amount: 1,
            current_deposited: 1_000_0000001,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::InvalidMilestoneAmount),
        },
        // Large amounts and overflow detection
        TestCase {
            name: "massive deposit exceeding contract total should fail",
            deposit_amount: 999_999_0000000,
            current_deposited: 1_0000000,
            max_contract_total: 1_000_0000000,
            expected: Err(EscrowError::InvalidMilestoneAmount),
        },
        TestCase {
            name: "deposit exceeding max single amount should fail with InvalidMilestoneAmount",
            deposit_amount: crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS + 1,
            current_deposited: 0,
            max_contract_total: crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS * 2,
            expected: Err(EscrowError::InvalidMilestoneAmount),
        },
        TestCase {
            name: "potential i128 overflow in addition should fail with PotentialOverflow",
            deposit_amount: 1,
            current_deposited: i128::MAX,
            max_contract_total: i128::MAX,
            expected: Err(EscrowError::PotentialOverflow),
        },
        // Minimal valid deposits
        TestCase {
            name: "minimum positive deposit (1 stroop) should succeed",
            deposit_amount: 1,
            current_deposited: 0,
            max_contract_total: 1_000_0000000,
            expected: Ok(()),
        },
    ];

    for tc in test_cases {
        let result = validate_deposit_amount(
            tc.deposit_amount,
            tc.current_deposited,
            tc.max_contract_total,
        );

        assert_eq!(
            result, tc.expected,
            "Test case '{}' failed: expected {:?}, got {:?}",
            tc.name, tc.expected, result
        );
    }
}
