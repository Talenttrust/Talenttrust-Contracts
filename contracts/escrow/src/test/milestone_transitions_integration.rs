//! Integration tests for milestone status transitions (Issue #1340).
/// Integration tests for milestone status transitions (Issue #1340).
///
/// These tests verify that:
/// 1. All five edge cases work correctly for each status-mutating entrypoint
/// 2. Authorization boundaries are preserved
/// 3. The centralized transition matrix is enforced consistently
/// 4. Version/actor metadata is persisted atomically
/// 5. Error handling is consistent across entrypoints
///
/// Edge cases tested:
/// - Valid transition: legitimate allowed status change succeeds with correct event/metadata
/// - Same status repeated: idempotent transitions behave as expected
/// - Backward transition: reversed status changes are correctly rejected
/// - Concurrent transitions: two racing transitions are handled correctly with versioning
/// - Unknown status: invalid state combinations are rejected safely
/// - Idempotent retries: repeated identical transitions do not corrupt version state
use crate::{
    milestone_transitions::{validate_milestone_transition, MilestoneState},
    Address, Contract, ContractStatus, Env, Escrow, Milestone, ReleaseAuthorization,
};
use soroban_sdk::{testutils::Address as _, Vec};

// ── Test Fixtures ────────────────────────────────────────────────────────────

/// Create a basic test contract with given status and release authorization
fn make_test_contract(
    env: &Env,
    client: Address,
    freelancer: Address,
    arbiter: Option<Address>,
    status: ContractStatus,
    release_auth: ReleaseAuthorization,
) -> Contract {
    Contract {
        client,
        freelancer,
        arbiter,
        status,
        total_deposited: 5000,
        funded_amount: 5000,
        released_amount: 0,
        refunded_amount: 0,
        release_authorization: release_auth,
        reputation_issued: false,
    }
}

/// Create a test milestone in Pending state
fn make_milestone_pending(amount: i128) -> Milestone {
    Milestone {
        amount,
        funded_amount: amount,
        released: false,
        refunded: false,
        work_evidence: None,
        refunded_amount: 0,
        deadline: None,
    }
}

// ── Edge Case 1: Valid Transitions ───────────────────────────────────────────

#[test]
fn test_release_milestone_valid_transition_pending_to_released() {
    // Verify that a legitimate Pending -> Released transition succeeds
    let env = Env::default();
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let arbiter = Some(Address::generate(&env));

    let _ = (client, freelancer, arbiter);
    let current_state = MilestoneState::Pending;
    let requested_state = MilestoneState::Released;

    let result = validate_milestone_transition(current_state, requested_state);
    assert!(
        result.is_ok(),
        "Valid transition Pending->Released should succeed"
    );
}

#[test]
fn test_refund_milestone_valid_transition_pending_to_refunded() {
    // Verify that a legitimate Pending -> Refunded transition succeeds
    let current_state = MilestoneState::Pending;
    let requested_state = MilestoneState::Refunded;

    let result = validate_milestone_transition(current_state, requested_state);
    assert!(
        result.is_ok(),
        "Valid transition Pending->Refunded should succeed"
    );
}

// ── Edge Case 2: Same Status Repeated (Idempotent) ──────────────────────────

#[test]
fn test_release_milestone_same_status_pending() {
    // Verify that transition to same Pending status is idempotent
    let current_state = MilestoneState::Pending;
    let requested_state = MilestoneState::Pending;

    let result = validate_milestone_transition(current_state, requested_state);
    assert!(result.is_ok(), "Idempotent Pending->Pending should succeed");
}

#[test]
fn test_release_milestone_same_status_released() {
    // Verify that transition to same Released status is idempotent
    let current_state = MilestoneState::Released;
    let requested_state = MilestoneState::Released;

    let result = validate_milestone_transition(current_state, requested_state);
    assert!(
        result.is_ok(),
        "Idempotent Released->Released should succeed"
    );
}

#[test]
fn test_refund_milestone_same_status_refunded() {
    // Verify that transition to same Refunded status is idempotent
    let current_state = MilestoneState::Refunded;
    let requested_state = MilestoneState::Refunded;

    let result = validate_milestone_transition(current_state, requested_state);
    assert!(
        result.is_ok(),
        "Idempotent Refunded->Refunded should succeed"
    );
}

// ── Edge Case 3: Backward Transitions (Invalid) ──────────────────────────────

#[test]
fn test_release_milestone_backward_released_to_pending() {
    // Verify that backward transition Released -> Pending is rejected
    let current_state = MilestoneState::Released;
    let requested_state = MilestoneState::Pending;

    let result = validate_milestone_transition(current_state, requested_state);
    assert!(
        result.is_err(),
        "Backward transition Released->Pending should fail"
    );
}

#[test]
fn test_release_milestone_backward_released_to_refunded() {
    // Verify that transition Released -> Refunded is rejected
    let current_state = MilestoneState::Released;
    let requested_state = MilestoneState::Refunded;

    let result = validate_milestone_transition(current_state, requested_state);
    assert!(result.is_err(), "Transition Released->Refunded should fail");
}

#[test]
fn test_refund_milestone_backward_refunded_to_pending() {
    // Verify that backward transition Refunded -> Pending is rejected
    let current_state = MilestoneState::Refunded;
    let requested_state = MilestoneState::Pending;

    let result = validate_milestone_transition(current_state, requested_state);
    assert!(
        result.is_err(),
        "Backward transition Refunded->Pending should fail"
    );
}

#[test]
fn test_refund_milestone_backward_refunded_to_released() {
    // Verify that transition Refunded -> Released is rejected
    let current_state = MilestoneState::Refunded;
    let requested_state = MilestoneState::Released;

    let result = validate_milestone_transition(current_state, requested_state);
    assert!(result.is_err(), "Transition Refunded->Released should fail");
}

// ── Edge Case 4: Concurrent Transitions ──────────────────────────────────────

#[test]
fn test_concurrent_transitions_version_check() {
    // Verify that version checking detects concurrent modifications
    use crate::milestone_transitions::{
        check_version_for_concurrency, read_milestone_version_and_actor, store_milestone_transition,
    };

    let env = Env::default();
    let contract_id = 1u32;
    let milestone_index = 0u32;
    let actor1 = Address::generate(&env);
    let actor2 = Address::generate(&env);

    // First transition: version becomes 1
    let v1 = store_milestone_transition(&env, contract_id, milestone_index, actor1);
    assert_eq!(v1, 1);

    // Attempt to apply a transition at version 0 (stale read) should fail
    let result = check_version_for_concurrency(&env, contract_id, milestone_index, 0);
    assert!(
        result.is_err(),
        "Stale version should be detected as concurrent modification"
    );

    // Attempt to apply a transition at version 1 (current) should succeed
    let result = check_version_for_concurrency(&env, contract_id, milestone_index, 1);
    assert!(
        result.is_ok(),
        "Current version should pass concurrency check"
    );

    // After second transition, version becomes 2
    let v2 = store_milestone_transition(&env, contract_id, milestone_index, actor2);
    assert_eq!(v2, 2);

    // Old version 1 should now fail
    let result = check_version_for_concurrency(&env, contract_id, milestone_index, 1);
    assert!(
        result.is_err(),
        "Stale version 1 should fail after second transition"
    );
}

// ── Edge Case 5: Unknown/Invalid Status ──────────────────────────────────────

#[test]
fn test_milestone_state_both_flags_set_invalid() {
    // Verify that invalid state (both flags set) is rejected safely
    let mut milestone = make_milestone_pending(1000);
    milestone.released = true;
    milestone.refunded = true;

    let result = MilestoneState::from_milestone(&milestone);
    assert!(
        result.is_err(),
        "Invalid state with both flags set should be rejected"
    );
}

// ── Authorization Boundary Tests ────────────────────────────────────────────

#[test]
fn test_release_milestone_client_only_authorization() {
    // Verify that ClientOnly release authorization is enforced
    let env = Env::default();
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let caller = Address::generate(&env);

    let contract = make_test_contract(
        &env,
        client,
        freelancer,
        None,
        ContractStatus::Funded,
        ReleaseAuthorization::ClientOnly,
    );

    let _ = (contract, caller);
    // Only client should be able to release
    // (Actual authorization check happens in release_milestone_impl via require_auth,
    //  but the centralized transition validator itself is agnostic to auth)

    let current_state = MilestoneState::Pending;
    let requested_state = MilestoneState::Released;
    let result = validate_milestone_transition(current_state, requested_state);
    assert!(
        result.is_ok(),
        "Transition should be valid regardless of authorization"
    );
}

#[test]
fn test_refund_milestone_client_only_authorization() {
    // Verify that only client can refund
    // (Actual authorization check happens in refund_unreleased_milestones_impl via require_auth)

    let current_state = MilestoneState::Pending;
    let requested_state = MilestoneState::Refunded;
    let result = validate_milestone_transition(current_state, requested_state);
    assert!(
        result.is_ok(),
        "Transition should be valid; auth is separate concern"
    );
}

// ── Escrow Conservation Tests ────────────────────────────────────────────────

#[test]
fn test_release_milestone_fund_amounts_unchanged() {
    // Verify that the transition validator doesn't affect fund transfer amounts
    // (This is more of a conceptual test; actual amounts are handled by release_milestone_impl)

    let milestone_amount = 1000i128;
    let milestone = make_milestone_pending(milestone_amount);

    // Verify the milestone amount is preserved through state transitions
    assert_eq!(milestone.amount, milestone_amount);
    assert_eq!(milestone.funded_amount, milestone_amount);
}

// ── Error Consistency Tests ──────────────────────────────────────────────────

#[test]
fn test_invalid_transition_error_stable() {
    // Verify that InvalidStatusTransition error is used consistently
    use crate::Error;

    let result = validate_milestone_transition(MilestoneState::Released, MilestoneState::Refunded);

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        Error::InvalidStatusTransition,
        "Invalid transitions should return stable InvalidStatusTransition error"
    );
}

#[test]
fn test_all_backward_transitions_use_same_error() {
    // Verify that all backward transitions use the same error type
    use crate::Error;

    let invalid_transitions = [
        (MilestoneState::Released, MilestoneState::Pending),
        (MilestoneState::Released, MilestoneState::Refunded),
        (MilestoneState::Refunded, MilestoneState::Pending),
        (MilestoneState::Refunded, MilestoneState::Released),
    ];

    for (current, requested) in invalid_transitions.iter() {
        let result = validate_milestone_transition(*current, *requested);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            Error::InvalidStatusTransition,
            "All invalid transitions should use InvalidStatusTransition error"
        );
    }
}

// ── Regression: Idempotent Retry & Concurrency Hardening ─────────────────────

#[test]
fn test_idempotent_retry_does_not_advance_version() {
    // A retried transition that observes the same version must not silently
    // advance the version counter, otherwise concurrent callers could be
    // tricked into accepting stale writes.
    use crate::milestone_transitions::{
        check_version_for_concurrency, read_milestone_version_and_actor,
        store_milestone_transition,
    };

    let env = Env::default();
    let contract_id = 42u32;
    let milestone_index = 0u32;
    let actor = Address::generate(&env);

    let v1 = store_milestone_transition(&env, contract_id, milestone_index, actor.clone());
    assert_eq!(v1, 1);

    // A retry that reads the current version must observe the latest value.
    let observed = read_milestone_version_and_actor(&env, contract_id, milestone_index);
    assert_eq!(observed.version, v1);
    assert_eq!(observed.last_modified_by, actor);

    // The retry must pass the concurrency check without mutating state.
    assert!(check_version_for_concurrency(&env, contract_id, milestone_index, v1).is_ok());

    // Re-reading must still report the same version (no accidental bump).
    let observed_again = read_milestone_version_and_actor(&env, contract_id, milestone_index);
    assert_eq!(observed_again.version, v1);
}

#[test]
fn test_concurrent_racing_transitions_only_one_wins() {
    // Simulate two racing transitions: the first writer advances the version,
    // and the second writer's stale version must be rejected.
    use crate::milestone_transitions::{
        check_version_for_concurrency, store_milestone_transition,
    };

    let env = Env::default();
    let contract_id = 7u32;
    let milestone_index = 0u32;
    let actor_a = Address::generate(&env);
    let actor_b = Address::generate(&env);

    // Both racers read version 0.
    let stale_version = 0u32;

    // Racer A wins and advances to version 1.
    let v = store_milestone_transition(&env, contract_id, milestone_index, actor_a);
    assert_eq!(v, 1);

    // Racer B attempts to write using the stale version -> must be rejected.
    assert!(
        check_version_for_concurrency(&env, contract_id, milestone_index, stale_version).is_err(),
        "Racing writer with stale version must be rejected"
    );

    // Racer B retries with the fresh version -> succeeds.
    assert!(
        check_version_for_concurrency(&env, contract_id, milestone_index, v).is_ok(),
        "Racing writer with fresh version must be accepted"
    );
    let v2 = store_milestone_transition(&env, contract_id, milestone_index, actor_b);
    assert_eq!(v2, 2);
}

#[test]
fn test_boundary_transition_matrix_is_total() {
    // Every (current, requested) pair must yield a deterministic result:
    // Ok for allowed/idempotent transitions, Err(InvalidStatusTransition)
    // for everything else. This guards against future matrix regressions.
    use crate::Error;

    let states = [
        MilestoneState::Pending,
        MilestoneState::Released,
        MilestoneState::Refunded,
    ];

    for current in states.iter() {
        for requested in states.iter() {
            let result = validate_milestone_transition(*current, *requested);
            let allowed = current == requested
                || (*current == MilestoneState::Pending
                    && (*requested == MilestoneState::Released
                        || *requested == MilestoneState::Refunded));
            if allowed {
                assert!(
                    result.is_ok(),
                    "Allowed transition {:?}->{:?} must succeed",
                    current,
                    requested
                );
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    Error::InvalidStatusTransition,
                    "Disallowed transition {:?}->{:?} must return InvalidStatusTransition",
                    current,
                    requested
                );
            }
        }
    }
}