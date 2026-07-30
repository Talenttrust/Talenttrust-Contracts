//! Tests to validate the authorization documentation matrix against source code.
//!
//! This test module ensures that the documented authorization rules in
//! docs/escrow/authorization.md match the actual implementation in
//! contracts/escrow/src/approvals.rs and contracts/escrow/src/lib.rs.
//!
//! The tests verify:
//! - Allowed approvers per mode
//! - Required approval logic per mode
//! - Allowed release callers per mode
//! - Error codes returned for unauthorized attempts

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};
use crate::{Escrow, EscrowClient, EscrowError, ReleaseAuthorization};

use super::assert_contract_error;

fn setup(env: &Env) -> (EscrowClient<'_>, Address, Address, Address) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);

    (client, client_addr, freelancer_addr, arbiter_addr)
}

fn create_funded_contract(
    env: &Env,
    client: &EscrowClient<'_>,
    client_addr: &Address,
    freelancer_addr: &Address,
    arbiter: Option<&Address>,
    auth: &ReleaseAuthorization,
) -> u32 {
    let milestones = vec![env, 500_0000000_i128, 300_0000000_i128];
    let id = client.create_contract(client_addr, freelancer_addr, &arbiter.cloned(), &milestones, auth);
    client.deposit_funds(&id, client_addr, &800_0000000_i128);
    id
}

// ===========================================================================
// ClientOnly Mode Validation
// ===========================================================================

#[test]
fn clientonly_matrix_allowed_approvers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::ClientOnly,
    );

    // Client can approve
    let result = client.try_approve_milestone_release(&id, &client_addr, &0);
    assert!(result.is_ok(), "Client should be allowed to approve in ClientOnly mode");

    // Freelancer cannot approve
    let result = client.try_approve_milestone_release(&id, &freelancer_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);

    // Arbiter cannot approve
    let result = client.try_approve_milestone_release(&id, &arbiter_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn clientonly_matrix_required_approvals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, _) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::ClientOnly,
    );

    // Without approvals, release fails
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert_contract_error(result, EscrowError::InsufficientApprovals);

    // With client approval, release succeeds
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert!(result.is_ok(), "Release should succeed with client approval");
}

#[test]
fn clientonly_matrix_allowed_release_callers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.approve_milestone_release(&id, &client_addr, &0));

    // Client can release
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert!(result.is_ok(), "Client should be allowed to release in ClientOnly mode");

    // Freelancer cannot release
    let result = client.try_release_milestone(&id, &freelancer_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);

    // Arbiter cannot release
    let result = client.try_release_milestone(&id, &arbiter_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ===========================================================================
// ArbiterOnly Mode Validation
// ===========================================================================

#[test]
fn arbiteronly_matrix_allowed_approvers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        Some(&arbiter_addr),
        &ReleaseAuthorization::ArbiterOnly,
    );

    // Arbiter can approve
    let result = client.try_approve_milestone_release(&id, &arbiter_addr, &0);
    assert!(result.is_ok(), "Arbiter should be allowed to approve in ArbiterOnly mode");

    // Client cannot approve
    let result = client.try_approve_milestone_release(&id, &client_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);

    // Freelancer cannot approve
    let result = client.try_approve_milestone_release(&id, &freelancer_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn arbiteronly_matrix_required_approvals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        Some(&arbiter_addr),
        &ReleaseAuthorization::ArbiterOnly,
    );

    // Without approvals, release fails
    let result = client.try_release_milestone(&id, &arbiter_addr, &0);
    assert_contract_error(result, EscrowError::InsufficientApprovals);

    // With arbiter approval, release succeeds
    assert!(client.approve_milestone_release(&id, &arbiter_addr, &0));
    let result = client.try_release_milestone(&id, &arbiter_addr, &0);
    assert!(result.is_ok(), "Release should succeed with arbiter approval");
}

#[test]
fn arbiteronly_matrix_allowed_release_callers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        Some(&arbiter_addr),
        &ReleaseAuthorization::ArbiterOnly,
    );

    assert!(client.approve_milestone_release(&id, &arbiter_addr, &0));

    // Arbiter can release
    let result = client.try_release_milestone(&id, &arbiter_addr, &0);
    assert!(result.is_ok(), "Arbiter should be allowed to release in ArbiterOnly mode");

    // Client cannot release
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);

    // Freelancer cannot release
    let result = client.try_release_milestone(&id, &freelancer_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ===========================================================================
// ClientAndArbiter Mode Validation
// ===========================================================================

#[test]
fn clientandarbiter_matrix_allowed_approvers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        Some(&arbiter_addr),
        &ReleaseAuthorization::ClientAndArbiter,
    );

    // Client can approve
    let result = client.try_approve_milestone_release(&id, &client_addr, &0);
    assert!(result.is_ok(), "Client should be allowed to approve in ClientAndArbiter mode");

    // Arbiter can approve
    let result = client.try_approve_milestone_release(&id, &arbiter_addr, &0);
    assert!(result.is_ok(), "Arbiter should be allowed to approve in ClientAndArbiter mode");

    // Freelancer cannot approve
    let result = client.try_approve_milestone_release(&id, &freelancer_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn clientandarbiter_matrix_required_approvals_or_logic() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    // Test with client approval only
    let id1 = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        Some(&arbiter_addr),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert!(client.approve_milestone_release(&id1, &client_addr, &0));
    let result = client.try_release_milestone(&id1, &client_addr, &0);
    assert!(result.is_ok(), "Release should succeed with only client approval (OR logic)");

    // Test with arbiter approval only
    let id2 = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        Some(&arbiter_addr),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert!(client.approve_milestone_release(&id2, &arbiter_addr, &0));
    let result = client.try_release_milestone(&id2, &arbiter_addr, &0);
    assert!(result.is_ok(), "Release should succeed with only arbiter approval (OR logic)");
}

#[test]
fn clientandarbiter_matrix_allowed_release_callers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        Some(&arbiter_addr),
        &ReleaseAuthorization::ClientAndArbiter,
    );

    assert!(client.approve_milestone_release(&id, &client_addr, &0));

    // Client can release
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert!(result.is_ok(), "Client should be allowed to release in ClientAndArbiter mode");

    // Arbiter can release
    let id2 = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        Some(&arbiter_addr),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert!(client.approve_milestone_release(&id2, &arbiter_addr, &0));
    let result = client.try_release_milestone(&id2, &arbiter_addr, &0);
    assert!(result.is_ok(), "Arbiter should be allowed to release in ClientAndArbiter mode");

    // Freelancer cannot release
    let result = client.try_release_milestone(&id, &freelancer_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ===========================================================================
// MultiSig Mode Validation
// ===========================================================================

#[test]
fn multisig_matrix_allowed_approvers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::MultiSig,
    );

    // Client can approve
    let result = client.try_approve_milestone_release(&id, &client_addr, &0);
    assert!(result.is_ok(), "Client should be allowed to approve in MultiSig mode");

    // Freelancer can approve
    let result = client.try_approve_milestone_release(&id, &freelancer_addr, &0);
    assert!(result.is_ok(), "Freelancer should be allowed to approve in MultiSig mode");

    // Arbiter cannot approve
    let result = client.try_approve_milestone_release(&id, &arbiter_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn multisig_matrix_required_approvals_and_logic() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, _) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::MultiSig,
    );

    // With only client approval, release fails
    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert_contract_error(result, EscrowError::InsufficientApprovals);

    // With both approvals, release succeeds
    assert!(client.approve_milestone_release(&id, &freelancer_addr, &0));
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert!(result.is_ok(), "Release should succeed with both client and freelancer approval (AND logic)");
}

#[test]
fn multisig_matrix_allowed_release_callers() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::MultiSig,
    );

    assert!(client.approve_milestone_release(&id, &client_addr, &0));
    assert!(client.approve_milestone_release(&id, &freelancer_addr, &0));

    // Client can release
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert!(result.is_ok(), "Client should be allowed to release in MultiSig mode");

    // Freelancer can release
    let id2 = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::MultiSig,
    );
    assert!(client.approve_milestone_release(&id2, &client_addr, &0));
    assert!(client.approve_milestone_release(&id2, &freelancer_addr, &0));
    let result = client.try_release_milestone(&id2, &freelancer_addr, &0);
    assert!(result.is_ok(), "Freelancer should be allowed to release in MultiSig mode");

    // Arbiter cannot release
    let result = client.try_release_milestone(&id, &arbiter_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ===========================================================================
// Error Code Validation
// ===========================================================================

#[test]
fn matrix_error_codes_unauthorized_role() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, arbiter_addr) = setup(&env);

    // ClientOnly: freelancer unauthorized
    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::ClientOnly,
    );
    let result = client.try_approve_milestone_release(&id, &freelancer_addr, &0);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn matrix_error_codes_already_approved() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, _) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::ClientOnly,
    );

    // First approval succeeds
    assert!(client.approve_milestone_release(&id, &client_addr, &0));

    // Duplicate approval fails
    let result = client.try_approve_milestone_release(&id, &client_addr, &0);
    assert_contract_error(result, EscrowError::AlreadyApproved);
}

#[test]
fn matrix_error_codes_insufficient_approvals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, _) = setup(&env);

    let id = create_funded_contract(
        &env,
        &client,
        &client_addr,
        &freelancer_addr,
        None,
        &ReleaseAuthorization::ClientOnly,
    );

    // Release without approval fails
    let result = client.try_release_milestone(&id, &client_addr, &0);
    assert_contract_error(result, EscrowError::InsufficientApprovals);
}

#[test]
fn matrix_error_codes_missing_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, _) = setup(&env);

    // ArbiterOnly without arbiter should fail at creation
    let milestones = vec![&env, 500_0000000_i128];
    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    );
    assert!(result.is_err(), "ArbiterOnly mode should require arbiter at contract creation");
}

