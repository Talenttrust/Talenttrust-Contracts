//! Comprehensive boundary tests for milestone logic
//!
//! Covers accept/reject boundaries for milestone operations including:
//! - Milestone index validation (in-bounds, out-of-bounds, edge cases)
//! - Release authorization and state validation
//! - Refund validation and boundary conditions
//! - Approval logic boundaries
//!
//! All tests use exact typed error codes and verify events where applicable.

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

use crate::{
    Contract, ContractStatus, Escrow, EscrowClient, EscrowError, Milestone, ReleaseAuthorization,
    MAX_MILESTONES,
};

use super::{assert_contract_error, register_client, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE};

// ---------------------------------------------------------------------------
// Helper: Assert error type from try_ calls
// ---------------------------------------------------------------------------

fn assert_err(
    result: Result<
        Result<bool, soroban_sdk::ConversionError>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
    expected: EscrowError,
) {
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = expected.into();
            assert_eq!(e, want, "wrong error: expected {:?}", expected);
        }
        other => panic!("expected {:?}, got {:?}", expected, other),
    }
}

// ---------------------------------------------------------------------------
// Test 1: Milestone Index Boundaries - Release
// ---------------------------------------------------------------------------

/// Release with milestone index 0 (first milestone) should succeed
#[test]
fn release_milestone_index_zero_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    client.approve_milestone_release(&id, &client_addr, &0);
    assert!(client.release_milestone(&id, &client_addr, &0));
}

/// Release with milestone index equal to milestone count minus 1 (last milestone)
#[test]
fn release_milestone_last_index_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    let total = MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE;
    client.deposit_funds(&id, &client_addr, &total);
    
    // Release last milestone (index 2 for 3 milestones)
    client.approve_milestone_release(&id, &client_addr, &2);
    assert!(client.release_milestone(&id, &client_addr, &2));
}

/// Release with milestone index equal to milestone count (one past end) should fail
#[test]
fn release_milestone_index_equals_count_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    
    // Try to release index 2 (count is 2, so valid indices are 0,1)
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &2),
        EscrowError::IndexOutOfBounds,
    );
}

/// Release with very large milestone index should fail with IndexOutOfBounds
#[test]
fn release_milestone_large_index_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &MILESTONE_ONE);
    
    // Try to release index 999
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &999),
        EscrowError::IndexOutOfBounds,
    );
}

// ---------------------------------------------------------------------------
// Test 2: Milestone Index Boundaries - Approval
// ---------------------------------------------------------------------------

/// Approve milestone index 0 (first milestone) should succeed
#[test]
fn approve_milestone_index_zero_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
}

/// Approve last milestone index should succeed
#[test]
fn approve_milestone_last_index_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    let total = MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE;
    client.deposit_funds(&id, &client_addr, &total);
    
    // Approve last milestone (index 2 for 3 milestones)
    assert!(client.approve_milestone_release(&id, &client_addr, &2));
}

/// Approve milestone with index equal to count should fail
#[test]
fn approve_milestone_index_equals_count_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    
    // Try to approve index 2 (count is 2)
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &2),
        EscrowError::IndexOutOfBounds,
    );
}

/// Approve milestone with very large index should fail
#[test]
fn approve_milestone_large_index_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &MILESTONE_ONE);
    
    // Try to approve index 100
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &100),
        EscrowError::IndexOutOfBounds,
    );
}

// ---------------------------------------------------------------------------
// Test 3: Milestone Index Boundaries - Refund
// ---------------------------------------------------------------------------

/// Refund with milestone index 0 should succeed
#[test]
fn refund_milestone_index_zero_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    
    // Refund first milestone
    let indices = vec![&env, 0u32];
    assert!(client.refund_unreleased_milestones(&id, &indices) > 0);
}

/// Refund last milestone index should succeed
#[test]
fn refund_milestone_last_index_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    let total = MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE;
    client.deposit_funds(&id, &client_addr, &total);
    
    // Refund last milestone (index 2)
    let indices = vec![&env, 2u32];
    assert!(client.refund_unreleased_milestones(&id, &indices) > 0);
}

/// Refund milestone with index equal to count should fail
#[test]
fn refund_milestone_index_equals_count_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    
    // Try to refund index 2 (count is 2)
    let indices = vec![&env, 2u32];
    assert_contract_error(
        client.try_refund_unreleased_milestones(&id, &indices),
        EscrowError::IndexOutOfBounds,
    );
}

/// Refund milestone with very large index should fail
#[test]
fn refund_milestone_large_index_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &MILESTONE_ONE);
    
    // Try to refund index 999
    let indices = vec![&env, 999u32];
    assert_contract_error(
        client.try_refund_unreleased_milestones(&id, &indices),
        EscrowError::IndexOutOfBounds,
    );
}

// ---------------------------------------------------------------------------
// Test 4: Multiple Milestone Operations - Boundaries
// ---------------------------------------------------------------------------

/// Refund all milestones by index (0 to count-1) should succeed
#[test]
fn refund_all_milestones_by_index_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    let total = MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE;
    client.deposit_funds(&id, &client_addr, &total);
    
    // Refund all milestones (indices 0, 1, 2)
    let indices = vec![&env, 0u32, 1u32, 2u32];
    let refunded = client.refund_unreleased_milestones(&id, &indices);
    assert_eq!(refunded, total);
}

/// Refund with duplicate milestone indices should fail
#[test]
fn refund_duplicate_indices_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    
    // Try to refund index 0 twice
    let indices = vec![&env, 0u32, 0u32];
    assert_contract_error(
        client.try_refund_unreleased_milestones(&id, &indices),
        EscrowError::DuplicateMilestoneInRefund,
    );
}

/// Refund empty indices list should fail
#[test]
fn refund_empty_indices_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &MILESTONE_ONE);
    
    // Try to refund empty list
    let indices = Vec::new(&env);
    assert_contract_error(
        client.try_refund_unreleased_milestones(&id, &indices),
        EscrowError::EmptyRefundRequest,
    );
}

// ---------------------------------------------------------------------------
// Test 5: State-Based Milestone Operation Boundaries
// ---------------------------------------------------------------------------

/// Release milestone on Created contract (not funded) should fail
#[test]
fn release_milestone_on_created_contract_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    // Don't deposit - try to release immediately
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        EscrowError::InvalidState,
    );
}

/// Approve milestone on Created contract (not funded) should fail
#[test]
fn approve_milestone_on_created_contract_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    // Don't deposit - try to approve immediately
    assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &0),
        EscrowError::InvalidState,
    );
}

/// Release already-released milestone should fail
#[test]
fn release_already_released_milestone_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    
    // Release milestone 0
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    
    // Try to release milestone 0 again
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        EscrowError::AlreadyReleased,
    );
}

/// Refund already-released milestone should fail
#[test]
fn refund_already_released_milestone_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    
    // Release milestone 0
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    
    // Try to refund released milestone 0
    let indices = vec![&env, 0u32];
    assert_contract_error(
        client.try_refund_unreleased_milestones(&id, &indices),
        EscrowError::AlreadyReleased,
    );
}

/// Refund already-refunded milestone should fail
#[test]
fn refund_already_refunded_milestone_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &(MILESTONE_ONE + MILESTONE_TWO));
    
    // Refund milestone 0
    let indices = vec![&env, 0u32];
    client.refund_unreleased_milestones(&id, &indices);
    
    // Try to refund milestone 0 again
    assert_contract_error(
        client.try_refund_unreleased_milestones(&id, &indices),
        EscrowError::AlreadyRefunded,
    );
}

// ---------------------------------------------------------------------------
// Test 6: MAX_MILESTONES Boundary Tests
// ---------------------------------------------------------------------------

/// Contract with exactly MAX_MILESTONES (10) milestones should succeed
#[test]
fn create_contract_with_max_milestones_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    
    // Create exactly MAX_MILESTONES milestones
    let mut milestones = Vec::new(&env);
    for _ in 0..MAX_MILESTONES {
        milestones.push_back(MILESTONE_ONE);
    }
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    assert!(id > 0);
}

/// Release all MAX_MILESTONES should succeed
#[test]
fn release_all_max_milestones_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    
    // Create exactly MAX_MILESTONES milestones
    let mut milestones = Vec::new(&env);
    for _ in 0..MAX_MILESTONES {
        milestones.push_back(MILESTONE_ONE);
    }
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    let total = MILESTONE_ONE * (MAX_MILESTONES as i128);
    client.deposit_funds(&id, &client_addr, &total);
    
    // Release all milestones
    for i in 0..MAX_MILESTONES {
        client.approve_milestone_release(&id, &client_addr, &i);
        client.release_milestone(&id, &client_addr, &i);
    }
    
    // Verify contract is completed
    let contract = client.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Completed);
}

/// Access last milestone (index MAX_MILESTONES-1) should succeed
#[test]
fn access_last_milestone_at_max_count_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    
    // Create exactly MAX_MILESTONES milestones
    let mut milestones = Vec::new(&env);
    for _ in 0..MAX_MILESTONES {
        milestones.push_back(MILESTONE_ONE);
    }
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    let total = MILESTONE_ONE * (MAX_MILESTONES as i128);
    client.deposit_funds(&id, &client_addr, &total);
    
    // Access last milestone (index 9 when MAX_MILESTONES is 10)
    let last_index = MAX_MILESTONES - 1;
    client.approve_milestone_release(&id, &client_addr, &last_index);
    assert!(client.release_milestone(&id, &client_addr, &last_index));
}

// ---------------------------------------------------------------------------
// Test 7: Authorization Boundary Tests
// ---------------------------------------------------------------------------

/// Release without approval when ClientOnly should fail
#[test]
fn release_without_approval_client_only_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &MILESTONE_ONE);
    
    // Try to release without approval
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        EscrowError::InsufficientApprovals,
    );
}

/// Unauthorized caller trying to release should fail
#[test]
fn unauthorized_caller_release_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let attacker = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &MILESTONE_ONE);
    client.approve_milestone_release(&id, &client_addr, &0);
    
    // Try to release as attacker
    assert_contract_error(
        client.try_release_milestone(&id, &attacker, &0),
        EscrowError::UnauthorizedRole,
    );
}

/// Unauthorized caller trying to approve should fail
#[test]
fn unauthorized_caller_approve_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let attacker = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &MILESTONE_ONE);
    
    // Try to approve as attacker (not client, not arbiter)
    assert_contract_error(
        client.try_approve_milestone_release(&id, &attacker, &0),
        EscrowError::UnauthorizedRole,
    );
}

// ---------------------------------------------------------------------------
// Test 8: Edge Cases and Combined Operations
// ---------------------------------------------------------------------------

/// Release milestone with index u32::MAX should fail
#[test]
fn release_milestone_max_u32_index_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &MILESTONE_ONE);
    
    // Try to release with maximum u32 index
    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &u32::MAX),
        EscrowError::IndexOutOfBounds,
    );
}

/// Sequential release of all milestones in order should succeed
#[test]
fn sequential_release_all_milestones_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    let total = MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE;
    client.deposit_funds(&id, &client_addr, &total);
    
    // Release in order: 0, 1, 2
    for i in 0..3 {
        client.approve_milestone_release(&id, &client_addr, &i);
        client.release_milestone(&id, &client_addr, &i);
    }
    
    let contract = client.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Completed);
}

/// Non-sequential release (2, 0, 1) should succeed
#[test]
fn non_sequential_release_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    let total = MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE;
    client.deposit_funds(&id, &client_addr, &total);
    
    // Release out of order: 2, then 0, then 1
    client.approve_milestone_release(&id, &client_addr, &2);
    client.release_milestone(&id, &client_addr, &2);
    
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    
    client.approve_milestone_release(&id, &client_addr, &1);
    client.release_milestone(&id, &client_addr, &1);
    
    let contract = client.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Completed);
}

/// Mix of release and refund operations should succeed
#[test]
fn mixed_release_and_refund_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    
    let total = MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE;
    client.deposit_funds(&id, &client_addr, &total);
    
    // Release milestone 0
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    
    // Refund milestone 1
    let indices = vec![&env, 1u32];
    client.refund_unreleased_milestones(&id, &indices);
    
    // Release milestone 2
    client.approve_milestone_release(&id, &client_addr, &2);
    client.release_milestone(&id, &client_addr, &2);
    
    // Contract should be completed after all milestones are processed
    let contract = client.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Completed);
}
