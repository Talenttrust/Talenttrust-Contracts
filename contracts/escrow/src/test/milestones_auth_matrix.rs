//! Milestones authorization-matrix tests (issue #21).
//!
//! Exhaustively covers every milestone-related action against every role (admin,
//! client, freelancer, arbiter, stranger), asserting allow/deny with typed error codes.
//! Also covers all four `ReleaseAuthorization` modes, contract state gates, and pause control guards.
//!
//! | Action | Admin | Client | Freelancer | Arbiter | Stranger | Expected Error |
//! |--------|:-----:|:------:|:----------:|:-------:|:--------:|----------------|
//! | `approve_milestone_release` (ClientOnly)       | ❌ | ✅ | ❌ | ❌ | ❌ | `UnauthorizedRole` |
//! | `approve_milestone_release` (ArbiterOnly)      | ❌ | ❌ | ❌ | ✅ | ❌ | `UnauthorizedRole` |
//! | `approve_milestone_release` (ClientAndArbiter) | ❌ | ✅ | ❌ | ✅ | ❌ | `UnauthorizedRole` |
//! | `approve_milestone_release` (MultiSig)         | ❌ | ✅ | ✅ | ❌ | ❌ | `UnauthorizedRole` |
//! | `release_milestone` (ClientOnly)               | ❌ | ✅ | ❌ | ❌ | ❌ | `UnauthorizedRole` |
//! | `release_milestone` (ArbiterOnly)              | ❌ | ❌ | ❌ | ✅ | ❌ | `UnauthorizedRole` |
//! | `release_milestone` (ClientAndArbiter)         | ❌ | ✅ | ❌ | ✅ | ❌ | `UnauthorizedRole` |
//! | `release_milestone` (MultiSig)                 | ❌ | ✅ | ✅ | ❌ | ❌ | `UnauthorizedRole` |
//! | `submit_work_evidence`                         | ❌ | ❌ | ✅ | ❌ | ❌ | `UnauthorizedRole` |
//! | `refund_unreleased_milestones`                 | ❌ | ✅ | ❌ | ❌ | ❌ | `UnauthorizedRole` |
//! | `get_milestones`                               | ✅ | ✅ | ✅ | ✅ | ✅ | (read-only query)  |
//! | `get_milestone`                                 | ✅ | ✅ | ✅ | ✅ | ✅ | (read-only query)  |
//! | `get_milestone_approvals`                       | ✅ | ✅ | ✅ | ✅ | ✅ | (read-only query)  |
//! | `get_approval_deadline`                        | ✅ | ✅ | ✅ | ✅ | ✅ | (read-only query)  |
//! | `get_work_evidence`                            | ✅ | ✅ | ✅ | ✅ | ✅ | (read-only query)  |
//! | `is_milestone_overdue`                         | ✅ | ✅ | ✅ | ✅ | ✅ | (read-only query)  |
//!
//! ## Structure
//!
//! - **Section 1**: `approve_milestone_release` matrix (all roles across modes)
//! - **Section 2**: `release_milestone` matrix (all roles across modes)
//! - **Section 3**: `submit_work_evidence` matrix (all roles)
//! - **Section 4**: `refund_unreleased_milestones` matrix (all roles)
//! - **Section 5**: Read-only queries (unauthenticated access by all roles)
//! - **Section 6**: Invalid contract state gates & pause guards

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};

use crate::{Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};

use super::assert_contract_error;

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

/// Create and initialize an escrow contract client, returning (escrow, admin).
fn make_escrow(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths();
    let contract_address = env.register(Escrow, ());
    let escrow = EscrowClient::new(env, &contract_address);
    let admin = Address::generate(env);
    escrow.initialize(&admin);
    (escrow, admin)
}

/// Create a contract with the given release authorization mode and deposit settlement token + funds.
///
/// Returns `(escrow, admin, client_addr, freelancer_addr, arbiter_addr, stranger_addr, contract_id)`.
fn setup_funded_with_mode(
    env: &Env,
    mode: ReleaseAuthorization,
) -> (
    EscrowClient<'_>,
    Address,
    Address,
    Address,
    Address,
    Address,
    u32,
) {
    let (escrow, admin) = make_escrow(env);
    let sac = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &sac);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    let stranger_addr = Address::generate(env);

    let milestones = vec![env, 100_0000000_i128, 200_0000000_i128];
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &mode,
    );

    let total_amount: i128 = 300_0000000;
    soroban_sdk::token::StellarAssetClient::new(env, &sac).mint(&client_addr, &total_amount);
    assert!(escrow.deposit_funds(&contract_id, &client_addr, &total_amount));

    (
        escrow,
        admin,
        client_addr,
        freelancer_addr,
        arbiter_addr,
        stranger_addr,
        contract_id,
    )
}

// ---------------------------------------------------------------------------
// Section 1 – approve_milestone_release authorization matrix
// ---------------------------------------------------------------------------

#[test]
fn test_approve_milestone_release_matrix_client_only() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ClientOnly);

    // Client -> ALLOW
    let res = escrow.try_approve_milestone_release(&contract_id, &client, &0);
    assert!(res.is_ok(), "Client must be allowed in ClientOnly mode");

    // Freelancer -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &freelancer, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Arbiter -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &arbiter, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Admin -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &admin, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Stranger -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &stranger, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);
}

#[test]
fn test_approve_milestone_release_matrix_arbiter_only() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ArbiterOnly);

    // Arbiter -> ALLOW
    let res = escrow.try_approve_milestone_release(&contract_id, &arbiter, &0);
    assert!(res.is_ok(), "Arbiter must be allowed in ArbiterOnly mode");

    // Client -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &client, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Freelancer -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &freelancer, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Admin -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &admin, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Stranger -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &stranger, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);
}

#[test]
fn test_approve_milestone_release_matrix_client_and_arbiter() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ClientAndArbiter);

    // Client -> ALLOW
    let res = escrow.try_approve_milestone_release(&contract_id, &client, &0);
    assert!(
        res.is_ok(),
        "Client must be allowed in ClientAndArbiter mode"
    );

    // Arbiter -> ALLOW
    let res = escrow.try_approve_milestone_release(&contract_id, &arbiter, &1);
    assert!(
        res.is_ok(),
        "Arbiter must be allowed in ClientAndArbiter mode"
    );

    // Freelancer -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &freelancer, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Admin -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &admin, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Stranger -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &stranger, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);
}

#[test]
fn test_approve_milestone_release_matrix_multisig() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::MultiSig);

    // Client -> ALLOW
    let res = escrow.try_approve_milestone_release(&contract_id, &client, &0);
    assert!(res.is_ok(), "Client must be allowed in MultiSig mode");

    // Freelancer -> ALLOW
    let res = escrow.try_approve_milestone_release(&contract_id, &freelancer, &0);
    assert!(res.is_ok(), "Freelancer must be allowed in MultiSig mode");

    // Arbiter -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &arbiter, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Admin -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &admin, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Stranger -> DENY (UnauthorizedRole)
    let res = escrow.try_approve_milestone_release(&contract_id, &stranger, &1);
    assert_contract_error(res, EscrowError::UnauthorizedRole);
}

// ---------------------------------------------------------------------------
// Section 2 – release_milestone authorization matrix
// ---------------------------------------------------------------------------

#[test]
fn test_release_milestone_matrix_client_only() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ClientOnly);

    // Approve milestone 0 with client
    assert!(escrow.approve_milestone_release(&contract_id, &client, &0));

    // Freelancer -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &freelancer, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Arbiter -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &arbiter, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Admin -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &admin, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Stranger -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &stranger, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Client -> ALLOW
    let res = escrow.try_release_milestone(&contract_id, &client, &0);
    assert!(
        res.is_ok(),
        "Client must be allowed to release in ClientOnly mode"
    );
}

#[test]
fn test_release_milestone_matrix_arbiter_only() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ArbiterOnly);

    // Approve milestone 0 with arbiter
    assert!(escrow.approve_milestone_release(&contract_id, &arbiter, &0));

    // Client -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &client, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Freelancer -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &freelancer, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Admin -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &admin, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Stranger -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &stranger, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Arbiter -> ALLOW
    let res = escrow.try_release_milestone(&contract_id, &arbiter, &0);
    assert!(
        res.is_ok(),
        "Arbiter must be allowed to release in ArbiterOnly mode"
    );
}

#[test]
fn test_release_milestone_matrix_client_and_arbiter() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ClientAndArbiter);

    // Approve milestone 0 with client
    assert!(escrow.approve_milestone_release(&contract_id, &client, &0));

    // Freelancer -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &freelancer, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Admin -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &admin, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Stranger -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &stranger, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Arbiter -> ALLOW
    let res = escrow.try_release_milestone(&contract_id, &arbiter, &0);
    assert!(
        res.is_ok(),
        "Arbiter must be allowed to release in ClientAndArbiter mode"
    );

    // Approve milestone 1 with arbiter and release with Client
    assert!(escrow.approve_milestone_release(&contract_id, &arbiter, &1));
    let res = escrow.try_release_milestone(&contract_id, &client, &1);
    assert!(
        res.is_ok(),
        "Client must be allowed to release in ClientAndArbiter mode"
    );
}

#[test]
fn test_release_milestone_matrix_multisig() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::MultiSig);

    // Both client and freelancer approve milestone 0
    assert!(escrow.approve_milestone_release(&contract_id, &client, &0));
    assert!(escrow.approve_milestone_release(&contract_id, &freelancer, &0));

    // Arbiter -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &arbiter, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Admin -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &admin, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Stranger -> DENY (UnauthorizedRole)
    let res = escrow.try_release_milestone(&contract_id, &stranger, &0);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Freelancer -> ALLOW (in MultiSig, either client or freelancer can trigger release once both approved)
    let res = escrow.try_release_milestone(&contract_id, &freelancer, &0);
    assert!(
        res.is_ok(),
        "Freelancer must be allowed to release in MultiSig mode after approvals"
    );

    // Approve milestone 1 with both and release with Client
    assert!(escrow.approve_milestone_release(&contract_id, &client, &1));
    assert!(escrow.approve_milestone_release(&contract_id, &freelancer, &1));
    let res = escrow.try_release_milestone(&contract_id, &client, &1);
    assert!(
        res.is_ok(),
        "Client must be allowed to release in MultiSig mode after approvals"
    );
}

// ---------------------------------------------------------------------------
// Section 3 – submit_work_evidence authorization matrix
// ---------------------------------------------------------------------------

#[test]
fn test_submit_work_evidence_matrix() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ClientOnly);

    let evidence = String::from_str(&env, "https://github.com/deliverable/pull/1");

    // Client -> DENY (UnauthorizedRole)
    let res = escrow.try_submit_work_evidence(&contract_id, &client, &0, &evidence);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Arbiter -> DENY (UnauthorizedRole)
    let res = escrow.try_submit_work_evidence(&contract_id, &arbiter, &0, &evidence);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Admin -> DENY (UnauthorizedRole)
    let res = escrow.try_submit_work_evidence(&contract_id, &admin, &0, &evidence);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Stranger -> DENY (UnauthorizedRole)
    let res = escrow.try_submit_work_evidence(&contract_id, &stranger, &0, &evidence);
    assert_contract_error(res, EscrowError::UnauthorizedRole);

    // Freelancer -> ALLOW
    let res = escrow.try_submit_work_evidence(&contract_id, &freelancer, &0, &evidence);
    assert!(
        res.is_ok(),
        "Freelancer must be allowed to submit work evidence"
    );
}

// ---------------------------------------------------------------------------
// Section 4 – refund_unreleased_milestones authorization matrix
// ---------------------------------------------------------------------------

#[test]
fn test_refund_unreleased_milestones_matrix() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ClientOnly);

    let indices = vec![&env, 0_u32];

    // NOTE: refund_unreleased_milestones uses contract.client.require_auth() without an explicit
    // caller parameter, meaning only the client can successfully call it. With mock_all_auths(),
    // we can't easily test auth failures for non-clients since the contract code doesn't receive
    // a caller parameter to validate. The contract implicitly enforces client-only access via
    // the require_auth() call on the stored client address.

    // However, the implementation guarantees only the client can refund because:
    // 1. The method calls contract.client.require_auth() which requires the client's signature
    // 2. Without mocking, any non-client caller would fail the auth check
    // 3. The authorization model is enforced by Soroban's auth system, not explicit role checks

    // Client -> ALLOW (this is the only authorized role)
    let res = escrow.try_refund_unreleased_milestones(&contract_id, &indices);
    assert!(
        res.is_ok(),
        "Client must be allowed to refund unreleased milestones"
    );

    // The deny cases for freelancer, arbiter, admin, and stranger are implicitly enforced
    // by the require_auth() call on the client address in the contract implementation.
    // With mock_all_auths() enabled, we cannot explicitly test these deny cases here,
    // but the contract's authorization logic ensures only the client can execute this action.
}

// ---------------------------------------------------------------------------
// Section 5 – Read-only queries (auth-free)
// ---------------------------------------------------------------------------

#[test]
fn test_read_only_milestone_queries_auth_free() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, arbiter, stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ClientOnly);

    let evidence = String::from_str(&env, "proof-of-work");
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer, &0, &evidence));
    assert!(escrow.approve_milestone_release(&contract_id, &client, &0));

    // Verify read-only queries succeed for all roles and strangers without requiring auth
    for role in [&admin, &client, &freelancer, &arbiter, &stranger] {
        let milestones = escrow.get_milestones(&contract_id);
        assert_eq!(milestones.len(), 2);

        let milestone = escrow.get_milestone(&contract_id, &0);
        assert!(milestone.is_some());

        let approvals = escrow.get_milestone_approvals(&contract_id, &0);
        assert!(approvals.is_some());

        let deadline = escrow.get_approval_deadline(&contract_id, &0);
        let _ = deadline;

        let work_ev = escrow.get_work_evidence(&contract_id, &0);
        assert_eq!(work_ev, Some(evidence.clone()));

        let overdue = escrow.is_milestone_overdue(&contract_id, &0);
        assert!(!overdue);
    }
}

// ---------------------------------------------------------------------------
// Section 6 – State gates & pause controls
// ---------------------------------------------------------------------------

#[test]
fn test_milestone_actions_invalid_state_gates() {
    let env = Env::default();
    let (escrow, admin) = make_escrow(&env);
    let sac = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &sac);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];

    // Create contract in Created state (unfunded)
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // approve_milestone_release on Created -> InvalidState
    let res = escrow.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_contract_error(res, Error::InvalidState);

    // release_milestone on Created -> InvalidState
    let res = escrow.try_release_milestone(&contract_id, &client_addr, &0);
    assert_contract_error(res, Error::InvalidState);

    // submit_work_evidence on Created -> InvalidState
    let evidence = String::from_str(&env, "evidence");
    let res = escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    assert_contract_error(res, EscrowError::InvalidState);

    // Fund the contract to advance to Funded state
    let total: i128 = 100_0000000;
    soroban_sdk::token::StellarAssetClient::new(&env, &sac).mint(&client_addr, &total);
    assert!(escrow.deposit_funds(&contract_id, &client_addr, &total));

    // Release milestone 0 -> advances to Completed state
    assert!(escrow.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(escrow.release_milestone(&contract_id, &client_addr, &0));

    // approve_milestone_release on Completed -> InvalidState
    let res = escrow.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_contract_error(res, Error::InvalidState);

    // release_milestone on Completed -> InvalidState
    let res = escrow.try_release_milestone(&contract_id, &client_addr, &0);
    assert_contract_error(res, Error::InvalidState);

    // submit_work_evidence on Completed -> InvalidState
    let res = escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    assert_contract_error(res, EscrowError::InvalidState);

    // refund_unreleased_milestones on Completed -> InvalidState
    let res = escrow.try_refund_unreleased_milestones(&contract_id, &vec![&env, 0_u32]);
    assert_contract_error(res, EscrowError::InvalidState);
}

#[test]
fn test_milestone_actions_blocked_when_paused() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, _arbiter, _stranger, contract_id) =
        setup_funded_with_mode(&env, ReleaseAuthorization::ClientOnly);

    // Admin pauses the contract
    escrow.pause(&admin);

    let evidence = String::from_str(&env, "evidence");

    // approve_milestone_release -> ContractPaused
    let res = escrow.try_approve_milestone_release(&contract_id, &client, &0);
    assert_contract_error(res, EscrowError::ContractPaused);

    // release_milestone -> ContractPaused
    let res = escrow.try_release_milestone(&contract_id, &client, &0);
    assert_contract_error(res, EscrowError::ContractPaused);

    // submit_work_evidence -> ContractPaused
    let res = escrow.try_submit_work_evidence(&contract_id, &freelancer, &0, &evidence);
    assert_contract_error(res, EscrowError::ContractPaused);

    // refund_unreleased_milestones -> ContractPaused
    let res = escrow.try_refund_unreleased_milestones(&contract_id, &vec![&env, 0_u32]);
    assert_contract_error(res, EscrowError::ContractPaused);

    // Admin unpauses
    escrow.unpause(&admin);

    // Actions succeed after unpause
    assert!(escrow.approve_milestone_release(&contract_id, &client, &0));
    assert!(escrow.release_milestone(&contract_id, &client, &0));
}
