/// Comprehensive tests for `simulate_create_contract` dry-run functionality.
///
/// These tests ensure that:
/// 1. Simulate returns the projected outcome matching what `create_contract` would produce
/// 2. Simulate performs all validation checks identical to `create_contract`
/// 3. Simulate makes no storage mutations
/// 4. Simulate requires no authorization
/// 5. Edge cases and error conditions are handled correctly
use soroban_sdk::vec;

use crate::{ContractStatus, ReleaseAuthorization, SimulateCreateContractOutcome};

use super::{create_client, setup};

/// Test that simulate returns the projected contract ID and parameters.
///
/// # Security
/// - Validates contract ID prediction
/// - Ensures all parameters are correctly returned
/// - Verifies total amount calculation
#[test]
fn simulate_returns_projected_outcome() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];

    let outcome = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Verify outcome contains correct values
    assert_eq!(outcome.contract_id, 1);
    assert_eq!(outcome.client, client_addr);
    assert_eq!(outcome.freelancer, freelancer_addr);
    assert_eq!(outcome.arbiter, None);
    assert_eq!(outcome.release_authorization, ReleaseAuthorization::ClientOnly);
    assert_eq!(outcome.milestones.len(), 2);
    assert_eq!(outcome.milestones.get(0).unwrap(), 200_0000000_i128);
    assert_eq!(outcome.milestones.get(1).unwrap(), 400_0000000_i128);
    assert_eq!(outcome.total_amount, 600_0000000_i128);
}

/// Test that simulate doesn't mutate storage (contract not created).
///
/// # Security
/// - Ensures storage remains unmodified after simulate
/// - Validates no contract record is persisted
/// - Verifies contract ID counter is not incremented
#[test]
fn simulate_does_not_mutate_storage() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    // Call simulate
    let outcome = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Verify contract was NOT actually created
    assert!(!client.contract_exists(&outcome.contract_id));

    // Verify next contract ID is still 1 (not incremented to 2)
    assert_eq!(client.get_next_contract_id(), 1);

    // Simulate another call - should get the same contract ID
    let outcome2 = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert_eq!(outcome2.contract_id, 1);
}

/// Test that simulate matches create_contract outcome.
///
/// # Security
/// - Ensures simulate outcome matches real contract creation
/// - Validates consistency between dry-run and actual operations
#[test]
fn simulate_outcome_matches_create_contract() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 300_0000000_i128];

    // Get simulated outcome
    let outcome = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Create the actual contract
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Verify IDs match
    assert_eq!(outcome.contract_id, contract_id);

    // Verify contract was created
    assert!(client.contract_exists(&contract_id));

    // Verify contract details match outcome
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.client, outcome.client);
    assert_eq!(contract.freelancer, outcome.freelancer);
    assert_eq!(contract.arbiter, outcome.arbiter);
    assert_eq!(contract.release_authorization, outcome.release_authorization);

    // Verify milestones match
    let stored_milestones = client.get_milestones(&contract_id);
    assert_eq!(stored_milestones.len(), outcome.milestones.len());
    for i in 0..stored_milestones.len() {
        assert_eq!(
            stored_milestones.get(i).unwrap().amount,
            outcome.milestones.get(i as u32).unwrap()
        );
    }

    // Verify total amount matches
    let total: i128 = stored_milestones
        .iter()
        .fold(0_i128, |sum, m| sum + m.amount);
    assert_eq!(total, outcome.total_amount);
}

/// Test that simulate validates empty milestones.
///
/// # Security
/// - Prevents invalid contract simulation
/// - Validates input sanitization
#[test]
#[should_panic]
fn simulate_rejects_empty_milestones() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env];

    client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

/// Test that simulate validates zero-amount milestones.
///
/// # Security
/// - Prevents dust attacks during simulation
/// - Validates milestone amount constraints
#[test]
#[should_panic]
fn simulate_rejects_zero_amount_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 0_i128];

    client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

/// Test that simulate rejects negative milestone amounts.
///
/// # Security
/// - Prevents negative amount attacks
/// - Validates amount sign
#[test]
#[should_panic]
fn simulate_rejects_negative_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, -100_0000000_i128];

    client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

/// Test that simulate validates same client and freelancer.
///
/// # Security
/// - Prevents self-dealing during simulation
/// - Validates participant uniqueness
#[test]
#[should_panic]
fn simulate_rejects_same_participants() {
    let (env, client_addr, _) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    client.simulate_create_contract(
        &client_addr,
        &client_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

/// Test that simulate validates too many milestones.
///
/// # Security
/// - Enforces milestone count limits during simulation
/// - Prevents resource exhaustion
#[test]
#[should_panic]
fn simulate_rejects_too_many_milestones() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);

    // Create more milestones than allowed
    let mut milestones = vec![&env];
    for _ in 0..11 {
        milestones.push_back(100_0000000_i128);
    }

    client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

/// Test that simulate validates arbiter requirement for ArbiterOnly mode.
///
/// # Security
/// - Ensures arbiter is present when required
/// - Validates authorization mode constraints
#[test]
#[should_panic]
fn simulate_requires_arbiter_for_arbiter_only() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    );
}

/// Test that simulate validates arbiter requirement for ClientAndArbiter mode.
///
/// # Security
/// - Ensures arbiter is present when required
/// - Validates authorization mode constraints
#[test]
#[should_panic]
fn simulate_requires_arbiter_for_client_and_arbiter() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );
}

/// Test that simulate validates arbiter is not the client.
///
/// # Security
/// - Prevents role confusion with arbiter=client
/// - Validates participant distinctness
#[test]
#[should_panic]
fn simulate_rejects_arbiter_as_client() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(client_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );
}

/// Test that simulate validates arbiter is not the freelancer.
///
/// # Security
/// - Prevents role confusion with arbiter=freelancer
/// - Validates participant distinctness
#[test]
#[should_panic]
fn simulate_rejects_arbiter_as_freelancer() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(freelancer_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );
}

/// Test that simulate works with arbiter addresses.
///
/// # Security
/// - Validates arbiter handling in outcome
/// - Ensures arbiter is correctly included in projection
#[test]
fn simulate_with_arbiter() {
    let (env, client_addr, freelancer_addr) = setup();
    let arbiter_addr = soroban_sdk::Address::generate(&env);
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    let outcome = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );

    assert_eq!(outcome.arbiter, Some(arbiter_addr));
    assert_eq!(outcome.client, client_addr);
    assert_eq!(outcome.freelancer, freelancer_addr);
}

/// Test that simulate returns correct total with multiple milestones.
///
/// # Security
/// - Validates correct arithmetic in total calculation
/// - Ensures all milestones are included in sum
#[test]
fn simulate_calculates_total_correctly() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![
        &env,
        100_0000000_i128,
        200_0000000_i128,
        150_0000000_i128,
        50_0000000_i128,
    ];

    let outcome = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert_eq!(outcome.total_amount, 500_0000000_i128);
    assert_eq!(outcome.milestones.len(), 4);
}

/// Test that simulate requires no caller authorization.
///
/// # Security
/// - Validates read-only nature of simulate
/// - Ensures no auth required for dry-run
#[test]
fn simulate_requires_no_authorization() {
    let (env, client_addr, freelancer_addr) = setup();
    // Create a client without auto-mocking auth
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    // This should not panic due to missing authorization
    // (simulate doesn't require auth)
    let outcome = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert_eq!(outcome.contract_id, 1);
}

/// Test that simulate with all release authorization modes.
///
/// # Security
/// - Validates all release authorization modes are correctly projected
/// - Ensures mode is correctly included in outcome
#[test]
fn simulate_with_all_authorization_modes() {
    let (env, client_addr, freelancer_addr) = setup();
    let arbiter_addr = soroban_sdk::Address::generate(&env);
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    // Test ClientOnly
    let outcome1 = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(outcome1.release_authorization, ReleaseAuthorization::ClientOnly);

    // Test ArbiterOnly (with arbiter)
    let outcome2 = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    );
    assert_eq!(outcome2.release_authorization, ReleaseAuthorization::ArbiterOnly);

    // Test ClientAndArbiter (with arbiter)
    let outcome3 = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert_eq!(outcome3.release_authorization, ReleaseAuthorization::ClientAndArbiter);

    // Test MultiSig (no arbiter required)
    let outcome4 = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::MultiSig,
    );
    assert_eq!(outcome4.release_authorization, ReleaseAuthorization::MultiSig);
}

/// Test that simulate increments contract ID for each call (reflects counter).
///
/// # Security
/// - Ensures contract IDs would be unique
/// - Validates proper ID allocation sequencing
#[test]
fn simulate_reflects_current_contract_id() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128];

    // First simulate should show ID 1
    let outcome1 = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(outcome1.contract_id, 1);

    // Create a real contract to increment counter
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Second simulate should now show ID 2
    let outcome2 = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(outcome2.contract_id, 2);
}

/// Test edge case with maximum milestone amount.
///
/// # Security
/// - Validates handling of maximum amounts
/// - Ensures total calculation doesn't overflow with max values
#[test]
fn simulate_with_large_amounts() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    // Use large but valid amounts
    let milestones = vec![&env, 1_000_000_000_000_i128, 2_000_000_000_000_i128];

    let outcome = client.simulate_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert_eq!(outcome.total_amount, 3_000_000_000_000_i128);
}
