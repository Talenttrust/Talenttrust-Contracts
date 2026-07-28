//! Disputes authorization-matrix tests (issue #21).
//!
//! This module provides an exhaustive role-by-action matrix for the two
//! dispute entrypoints:
//!
//! | Role        | `raise_dispute` | `resolve_dispute` |
//! |-------------|----------------|-------------------|
//! | client      | ✅ ALLOW       | ❌ UnauthorizedRole|
//! | freelancer  | ✅ ALLOW       | ❌ UnauthorizedRole|
//! | arbiter     | ❌ UnauthorizedRole | ✅ ALLOW      |
//! | admin       | ❌ UnauthorizedRole | ❌ UnauthorizedRole|
//! | stranger    | ❌ UnauthorizedRole | ❌ UnauthorizedRole|
//!
//! Additional state-gate tests verify the error codes returned when callers
//! that would otherwise be allowed act from a wrong contract lifecycle state.
//!
//! ## Structure
//!
//! - **Section 1** – `raise_dispute` matrix: who may and may not raise.
//! - **Section 2** – `resolve_dispute` matrix: who may and may not resolve.
//! - **Section 3** – State-gate matrix: valid callers, wrong lifecycle state.
//! - **Section 4** – Edge cases: arbiter == None, double raise, paused contract.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{
    ContractStatus, DisputeResolution, DisputeSplit, Error, Escrow, EscrowClient,
    ReleaseAuthorization,
};

use super::assert_contract_error;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Build an initialized escrow client; returns (client_handle, admin_addr).
fn make_escrow(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths();
    let contract_address = env.register(Escrow, ());
    let escrow = EscrowClient::new(env, &contract_address);
    let admin = Address::generate(env);
    escrow.initialize(&admin);
    (escrow, admin)
}

/// Create a contract with one milestone (100 stroops) with an arbiter assigned,
/// then deposit the full milestone amount.
///
/// Returns `(client_addr, freelancer_addr, arbiter_addr, contract_id)`.
fn setup_funded(env: &Env, escrow: &EscrowClient<'_>) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    let milestones = vec![env, 100_i128];

    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(escrow.deposit_funds(&contract_id, &client_addr, &100_i128));
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

/// Like `setup_funded` but WITHOUT an arbiter.
fn setup_funded_no_arbiter(env: &Env, escrow: &EscrowClient<'_>) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 100_i128];

    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(escrow.deposit_funds(&contract_id, &client_addr, &100_i128));
    (client_addr, freelancer_addr, contract_id)
}

/// Advance a funded contract into `Disputed` state.
///
/// Returns `(client_addr, freelancer_addr, arbiter_addr, contract_id)`.
fn setup_disputed(env: &Env, escrow: &EscrowClient<'_>) -> (Address, Address, Address, u32) {
    let (client_addr, freelancer_addr, arbiter_addr, contract_id) = setup_funded(env, escrow);
    assert!(escrow.raise_dispute(&contract_id, &client_addr));
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

// ---------------------------------------------------------------------------
// Section 1 – raise_dispute authorization matrix
// ---------------------------------------------------------------------------

/// Matrix row: CLIENT — allowed to raise a dispute on a funded contract.
#[test]
fn raise_dispute_matrix_client_is_allowed() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (client_addr, _freelancer_addr, _arbiter_addr, contract_id) = setup_funded(&env, &escrow);

    assert!(
        escrow.raise_dispute(&contract_id, &client_addr),
        "client must be allowed to raise a dispute"
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Disputed,
        "contract must enter Disputed state after raise by client"
    );
}

/// Matrix row: FREELANCER — allowed to raise a dispute on a funded contract.
#[test]
fn raise_dispute_matrix_freelancer_is_allowed() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, freelancer_addr, _arbiter_addr, contract_id) = setup_funded(&env, &escrow);

    assert!(
        escrow.raise_dispute(&contract_id, &freelancer_addr),
        "freelancer must be allowed to raise a dispute"
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// Matrix row: ARBITER — denied from raising a dispute (UnauthorizedRole).
#[test]
fn raise_dispute_matrix_arbiter_is_denied() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, _freelancer_addr, arbiter_addr, contract_id) = setup_funded(&env, &escrow);

    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &arbiter_addr),
        Error::UnauthorizedRole,
    );
    // State must remain unchanged.
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Funded
    );
}

/// Matrix row: ADMIN — denied from raising a dispute (UnauthorizedRole).
/// The admin address is not a contract party and must not be able to raise.
#[test]
fn raise_dispute_matrix_admin_is_denied() {
    let env = Env::default();
    let (escrow, admin) = make_escrow(&env);
    let (_client_addr, _freelancer_addr, _arbiter_addr, contract_id) = setup_funded(&env, &escrow);

    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &admin),
        Error::UnauthorizedRole,
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Funded
    );
}

/// Matrix row: STRANGER — denied from raising a dispute (UnauthorizedRole).
#[test]
fn raise_dispute_matrix_stranger_is_denied() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, _freelancer_addr, _arbiter_addr, contract_id) = setup_funded(&env, &escrow);
    let stranger = Address::generate(&env);

    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &stranger),
        Error::UnauthorizedRole,
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Funded
    );
}

// ---------------------------------------------------------------------------
// Section 2 – resolve_dispute authorization matrix
// ---------------------------------------------------------------------------

/// Matrix row: ARBITER — allowed to resolve an open dispute.
#[test]
fn resolve_dispute_matrix_arbiter_is_allowed() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, _freelancer_addr, arbiter_addr, contract_id) = setup_disputed(&env, &escrow);

    assert!(
        escrow.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        "arbiter must be allowed to resolve a dispute"
    );
    // Contract is now in a terminal state — Refunded because full balance was refunded.
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Refunded
    );
}

/// Matrix row: CLIENT — denied from resolving a dispute (UnauthorizedRole).
#[test]
fn resolve_dispute_matrix_client_is_denied() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (client_addr, _freelancer_addr, _arbiter_addr, contract_id) = setup_disputed(&env, &escrow);

    assert_contract_error(
        escrow.try_resolve_dispute(&contract_id, &client_addr, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
    // State must remain Disputed.
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// Matrix row: FREELANCER — denied from resolving a dispute (UnauthorizedRole).
#[test]
fn resolve_dispute_matrix_freelancer_is_denied() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, freelancer_addr, _arbiter_addr, contract_id) = setup_disputed(&env, &escrow);

    assert_contract_error(
        escrow.try_resolve_dispute(
            &contract_id,
            &freelancer_addr,
            &DisputeResolution::FullPayout,
        ),
        Error::UnauthorizedRole,
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// Matrix row: ADMIN — denied from resolving a dispute (UnauthorizedRole).
#[test]
fn resolve_dispute_matrix_admin_is_denied() {
    let env = Env::default();
    let (escrow, admin) = make_escrow(&env);
    let (_client_addr, _freelancer_addr, _arbiter_addr, contract_id) =
        setup_disputed(&env, &escrow);

    assert_contract_error(
        escrow.try_resolve_dispute(&contract_id, &admin, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// Matrix row: STRANGER — denied from resolving a dispute (UnauthorizedRole).
#[test]
fn resolve_dispute_matrix_stranger_is_denied() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, _freelancer_addr, _arbiter_addr, contract_id) =
        setup_disputed(&env, &escrow);
    let stranger = Address::generate(&env);

    assert_contract_error(
        escrow.try_resolve_dispute(&contract_id, &stranger, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// A different arbiter (not the one assigned) is also denied (UnauthorizedRole).
#[test]
fn resolve_dispute_matrix_wrong_arbiter_is_denied() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, _freelancer_addr, _arbiter_addr, contract_id) =
        setup_disputed(&env, &escrow);
    let wrong_arbiter = Address::generate(&env);

    assert_contract_error(
        escrow.try_resolve_dispute(&contract_id, &wrong_arbiter, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

// ---------------------------------------------------------------------------
// Section 3 – State-gate matrix
// ---------------------------------------------------------------------------
//
// Even a legitimately authorized caller must be rejected when the contract is
// in the wrong lifecycle state. We test each terminal/non-disputable state.

/// Client cannot raise a dispute on a contract that is in `Created` state
/// (not yet funded — only `Funded` and `PartiallyFunded` are disputable).
#[test]
fn raise_dispute_state_gate_created_state_is_rejected() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128];

    // Create but do NOT deposit — status stays Created.
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Created
    );

    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &client_addr),
        Error::InvalidState,
    );
}

/// Client cannot raise a dispute on a `Completed` contract.
#[test]
fn raise_dispute_state_gate_completed_state_is_rejected() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (client_addr, _freelancer_addr, _arbiter_addr, contract_id) = setup_funded(&env, &escrow);

    // Release the only milestone to reach Completed.
    assert!(escrow.release_milestone(&contract_id, &client_addr, &0));
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Completed
    );

    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &client_addr),
        Error::InvalidState,
    );
}

/// Client cannot raise a dispute on a `Refunded` contract.
#[test]
fn raise_dispute_state_gate_refunded_state_is_rejected() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (client_addr, _freelancer_addr, arbiter_addr, contract_id) = setup_funded(&env, &escrow);

    // Raise and fully refund.
    escrow.raise_dispute(&contract_id, &client_addr);
    escrow.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund);
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Refunded
    );

    // Either party attempting to re-raise must fail with AlreadyFinalized
    // (contract has been resolved and is terminal).
    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &client_addr),
        Error::AlreadyFinalized,
    );
}

/// Client cannot raise a dispute on a `Disputed` contract (already disputed).
#[test]
fn raise_dispute_state_gate_already_disputed_is_rejected() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (client_addr, _freelancer_addr, _arbiter_addr, contract_id) = setup_disputed(&env, &escrow);

    // Attempting to raise again while already in Disputed state.
    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &client_addr),
        Error::InvalidState,
    );
}

/// Arbiter cannot resolve a dispute on a `Funded` (non-disputed) contract.
/// The contract must be in `Disputed` state for resolution to proceed.
#[test]
fn resolve_dispute_state_gate_funded_state_is_rejected() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, _freelancer_addr, arbiter_addr, contract_id) = setup_funded(&env, &escrow);

    // Contract is Funded, not Disputed — resolve must fail.
    assert_contract_error(
        escrow.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

/// Arbiter cannot resolve a dispute on a `Completed` contract.
#[test]
fn resolve_dispute_state_gate_completed_state_is_rejected() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (client_addr, _freelancer_addr, arbiter_addr, contract_id) = setup_funded(&env, &escrow);

    // Complete the contract first.
    escrow.release_milestone(&contract_id, &client_addr, &0);
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Completed
    );

    assert_contract_error(
        escrow.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

/// After resolution succeeds, a second resolve attempt fails with InvalidStatusTransition.
#[test]
fn resolve_dispute_state_gate_double_resolve_is_rejected() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, _freelancer_addr, arbiter_addr, contract_id) = setup_disputed(&env, &escrow);

    // First resolve succeeds.
    assert!(escrow.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund));

    // Second resolve on the now-terminal contract must fail.
    assert_contract_error(
        escrow.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullPayout),
        Error::InvalidStatusTransition,
    );
}

// ---------------------------------------------------------------------------
// Section 4 – Edge cases
// ---------------------------------------------------------------------------

/// Without an arbiter, any party's raise attempt yields `ArbiterRequired`
/// regardless of their role.
#[test]
fn raise_dispute_edge_no_arbiter_client_denied_with_arbiter_required() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (client_addr, _freelancer_addr, contract_id) = setup_funded_no_arbiter(&env, &escrow);

    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &client_addr),
        Error::ArbiterRequired,
    );
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Funded
    );
}

/// Without an arbiter, the freelancer also receives `ArbiterRequired`.
#[test]
fn raise_dispute_edge_no_arbiter_freelancer_denied_with_arbiter_required() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (_client_addr, freelancer_addr, contract_id) = setup_funded_no_arbiter(&env, &escrow);

    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &freelancer_addr),
        Error::ArbiterRequired,
    );
}

/// After finalization of a disputed contract, raise_dispute fails with
/// `AlreadyFinalized` even for contract parties.
#[test]
fn raise_dispute_edge_finalized_contract_denied_with_already_finalized() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (client_addr, freelancer_addr, _arbiter_addr, contract_id) = setup_disputed(&env, &escrow);

    // Finalize the disputed contract (client is a participant).
    assert!(escrow.finalize_contract(&contract_id, &client_addr));

    // Both parties must now get AlreadyFinalized.
    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &client_addr),
        Error::AlreadyFinalized,
    );
    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &freelancer_addr),
        Error::AlreadyFinalized,
    );
}

/// After finalization, even the arbiter cannot resolve — AlreadyFinalized.
#[test]
fn resolve_dispute_edge_finalized_contract_denied_with_already_finalized() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let (client_addr, _freelancer_addr, arbiter_addr, contract_id) = setup_disputed(&env, &escrow);

    // Finalize the disputed contract.
    assert!(escrow.finalize_contract(&contract_id, &client_addr));

    assert_contract_error(
        escrow.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::AlreadyFinalized,
    );
}

/// Verify that the full matrix of resolution variants are all allowed for the arbiter
/// and all denied for non-arbiters — one assertion per variant per role.
#[test]
fn resolve_dispute_matrix_all_resolution_variants_arbiter_allowed() {
    let resolutions = [
        DisputeResolution::FullRefund,
        DisputeResolution::FullPayout,
        DisputeResolution::PartialRefund,
        DisputeResolution::Split(DisputeSplit {
            client_amount: 40,
            freelancer_amount: 60,
        }),
    ];

    for resolution in &resolutions {
        let env = Env::default();
        let (escrow, _admin) = make_escrow(&env);
        let (_client_addr, _freelancer_addr, arbiter_addr, contract_id) =
            setup_disputed(&env, &escrow);

        assert!(
            escrow.resolve_dispute(&contract_id, &arbiter_addr, resolution),
            "arbiter must be allowed for resolution variant {:?}",
            resolution
        );
        // Each resolution variant ends in a terminal state.
        let status = escrow.get_contract(&contract_id).status;
        assert!(
            status == ContractStatus::Completed || status == ContractStatus::Refunded,
            "contract must reach a terminal state after resolution, got {:?}",
            status
        );
    }
}

/// Verify all resolution variants are denied for a stranger — each variant
/// returns UnauthorizedRole regardless of the resolution type.
#[test]
fn resolve_dispute_matrix_all_resolution_variants_stranger_denied() {
    let resolutions = [
        DisputeResolution::FullRefund,
        DisputeResolution::FullPayout,
        DisputeResolution::PartialRefund,
        DisputeResolution::Split(DisputeSplit {
            client_amount: 40,
            freelancer_amount: 60,
        }),
    ];

    for resolution in &resolutions {
        let env = Env::default();
        let (escrow, _admin) = make_escrow(&env);
        let (_client_addr, _freelancer_addr, _arbiter_addr, contract_id) =
            setup_disputed(&env, &escrow);
        let stranger = Address::generate(&env);

        assert_contract_error(
            escrow.try_resolve_dispute(&contract_id, &stranger, resolution),
            Error::UnauthorizedRole,
        );
        assert_eq!(
            escrow.get_contract(&contract_id).status,
            ContractStatus::Disputed,
            "state must not change after rejected resolve for variant {:?}",
            resolution
        );
    }
}

/// The client can raise a dispute but then the freelancer — as a party — can also
/// raise on a *different* fresh funded contract. Tests symmetry of party access.
#[test]
fn raise_dispute_matrix_both_parties_are_independently_allowed() {
    // client raises on contract A
    {
        let env = Env::default();
        let (escrow, _admin) = make_escrow(&env);
        let (client_addr, _freelancer_addr, _arbiter_addr, contract_id) =
            setup_funded(&env, &escrow);
        assert!(escrow.raise_dispute(&contract_id, &client_addr));
    }

    // freelancer raises on contract B
    {
        let env = Env::default();
        let (escrow, _admin) = make_escrow(&env);
        let (_client_addr, freelancer_addr, _arbiter_addr, contract_id) =
            setup_funded(&env, &escrow);
        assert!(escrow.raise_dispute(&contract_id, &freelancer_addr));
    }
}

/// Explicit symmetry check: stranger is rejected for raise AND resolve in the
/// same test — confirms no cross-contamination between the two entrypoints.
#[test]
fn auth_matrix_stranger_denied_for_both_entrypoints() {
    let env = Env::default();
    let (escrow, _admin) = make_escrow(&env);
    let stranger = Address::generate(&env);

    // Test raise on Funded contract.
    let (client_addr, _fl, _arb, contract_id) = setup_funded(&env, &escrow);
    assert_contract_error(
        escrow.try_raise_dispute(&contract_id, &stranger),
        Error::UnauthorizedRole,
    );

    // Advance to Disputed state as client, then test resolve as stranger.
    escrow.raise_dispute(&contract_id, &client_addr);
    assert_contract_error(
        escrow.try_resolve_dispute(&contract_id, &stranger, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
}
