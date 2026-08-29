use super::{default_milestones, generated_participants3, register_client, total_milestones};
use crate::{Error, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn test_only_client_can_deposit_funds() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_deposit_funds(&contract_id, &freelancer_addr, &total_milestones());
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_freelancer_cannot_approve_milestone_release() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_approve_milestone_release(&contract_id, &freelancer_addr, &0);
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_freelancer_cannot_release_milestone() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));

    let result = client.try_release_milestone(&contract_id, &freelancer_addr, &0);
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_only_client_can_issue_reputation() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert!(client.release_milestone(&contract_id, &client_addr, &2));

    let result = client.try_issue_reputation(
        &contract_id,
        &freelancer_addr,
        &5,
        &soroban_sdk::String::from_str(&env, "test"),
    );
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_issue_reputation_rejects_freelancer_mismatch() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);
    let wrong_freelancer = soroban_sdk::Address::generate(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert!(client.release_milestone(&contract_id, &client_addr, &2));

    let result = client.try_issue_reputation(
        &contract_id,
        &client_addr,
        &5,
        &soroban_sdk::String::from_str(&env, "test"),
    );
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_create_rejects_arbiter_modes_without_arbiter() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    super::assert_contract_error(result, Error::MissingArbiter);
}

#[test]
fn test_create_rejects_invalid_arbiter_role_overlap() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(client_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    super::assert_contract_error(result, Error::InvalidArbiter);
}

#[test]
#[should_panic]
fn test_create_contract_requires_authentication_of_roles() {
    let env = Env::default();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    // No env.mock_all_auths() in this test: role addresses must authorize.
    let _ = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
fn test_create_rejects_same_client_and_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let result = client.try_create_contract(
        &client_addr,
        &client_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, Error::InvalidParticipant);
}

#[test]
fn test_create_rejects_empty_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);
    let empty = soroban_sdk::Vec::<i128>::new(&env);

    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &empty,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, Error::EmptyMilestones);
}

#[test]
fn test_deposit_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_deposit_funds(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::AmountMustBePositive);
}

#[test]
fn test_deposit_rejects_when_contract_not_created() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    let result = client.try_deposit_funds(&contract_id, &client_addr, &total_milestones());
    super::assert_contract_error(result, Error::InvalidState);
}

#[test]
fn test_approve_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::InvalidState);
}

#[test]
fn test_approve_rejects_already_released_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::MilestoneAlreadyReleased);
}

#[test]
fn test_approve_rejects_duplicate_client_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::AlreadyApproved);
}

#[test]
fn test_approve_rejects_duplicate_arbiter_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &arbiter_addr, &0));
    let result = client.try_approve_milestone_release(&contract_id, &arbiter_addr, &0);
    super::assert_contract_error(result, Error::AlreadyApproved);
}

#[test]
fn test_release_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::InvalidState);
}

#[test]
fn test_release_rejects_already_released_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    super::assert_contract_error(result, Error::MilestoneAlreadyReleased);
}

#[test]
fn test_issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert!(client.release_milestone(&contract_id, &client_addr, &2));

    let result = client.try_issue_reputation(
        &contract_id,
        &client_addr,
        &0,
        &soroban_sdk::String::from_str(&env, "test"),
    );
    super::assert_contract_error(result, Error::InvalidRating);
}

#[test]
fn test_issue_reputation_requires_completed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_issue_reputation(
        &contract_id,
        &client_addr,
        &5,
        &soroban_sdk::String::from_str(&env, "test"),
    );
    super::assert_contract_error(result, Error::InvalidState);
}

#[test]
fn test_issue_reputation_rejects_duplicate_issuance() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, _arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert!(client.release_milestone(&contract_id, &client_addr, &2));

    assert!(client.issue_reputation(
        &contract_id,
        &client_addr,
        &5,
        &soroban_sdk::String::from_str(&env, "test")
    ));
    let result = client.try_issue_reputation(
        &contract_id,
        &client_addr,
        &4,
        &soroban_sdk::String::from_str(&env, "test2"),
    );
    super::assert_contract_error(result, Error::ReputationAlreadyIssued);
}

#[test]
fn test_client_and_arbiter_mode_rejects_third_party_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);
    let outsider = soroban_sdk::Address::generate(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    let result = client.try_approve_milestone_release(&contract_id, &outsider, &0);
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

#[test]
fn test_arbiter_only_flow_enforces_arbiter_approval_and_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr) = generated_participants3(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestones()));

    // Client cannot approve in ArbiterOnly.
    let client_approval = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    super::assert_contract_error(client_approval, Error::UnauthorizedRole);

    assert!(client.approve_milestone_release(&contract_id, &arbiter_addr, &0));
    assert!(client.release_milestone(&contract_id, &arbiter_addr, &0));
}

// ===========================================================================
// submit_work_evidence — security gating (issue #745)
// ===========================================================================
//
// Coverage matrix:
//   Caller gates     : freelancer ✓  |  client ✗  |  arbiter ✗  |  third-party ✗
//   Contract state   : Funded ✓  |  Created ✗  |  Cancelled ✗  |  Disputed ✗
//                    | Completed ✗  |  Refunded ✗
//   Milestone state  : unreleased ✓  |  released ✗  |  refunded (via full
//                      contract refund) ✗
//   Evidence string  : valid ✓  |  empty ✗  |  1 byte ✓  |  256 bytes ✓
//                    | 257 bytes ✗
//   Paused           : blocks all ✗  |  unpaused accepts ✓
//   Unknown contract : ContractNotFound ✗
//   Index OOB        : IndexOutOfBounds ✗
//   Multi-milestone  : per-slot isolation ✓  |  overwrite ✓

use crate::{ContractStatus, EscrowError};
use soroban_sdk::{token::StellarAssetClient, String};

use super::{assert_contract_error, EscrowFixtureBuilder, MILESTONE_ONE};

/// Convenience: build a Soroban `String` from a plain `&str`.
fn s(env: &soroban_sdk::Env, text: &str) -> String {
    String::from_str(env, text)
}

// ── caller gates ─────────────────────────────────────────────────────────────

/// The freelancer (the only valid caller) successfully submits evidence.
#[test]
fn submit_work_evidence_freelancer_succeeds() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    let evidence = s(&f.env, "ipfs://QmValid");
    assert!(escrow.submit_work_evidence(&f.escrow_id, &f.freelancer, &0, &evidence));
    assert_eq!(escrow.get_work_evidence(&f.escrow_id, &0), Some(evidence));
}

/// The client is not the freelancer — must be rejected with `UnauthorizedRole`.
#[test]
fn submit_work_evidence_client_rejected() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    let evidence = s(&f.env, "ipfs://QmClient");
    let result = escrow.try_submit_work_evidence(&f.escrow_id, &f.client, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// An assigned arbiter is not the freelancer — must be rejected.
#[test]
fn submit_work_evidence_arbiter_rejected() {
    // Build a funded contract with an explicitly assigned arbiter and verify
    // that the arbiter cannot submit evidence (only the freelancer can).
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let client = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);
    let arbiter = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &Some(arbiter.clone()),
        &soroban_sdk::vec![&env, MILESTONE_ONE],
        &crate::ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client, &MILESTONE_ONE);
    escrow.deposit_funds(&contract_id, &client, &MILESTONE_ONE);

    let evidence = s(&env, "ipfs://QmArbiter");
    let result = escrow.try_submit_work_evidence(&contract_id, &arbiter, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// A random third party must be rejected with `UnauthorizedRole`.
#[test]
fn submit_work_evidence_third_party_rejected() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    let outsider = soroban_sdk::Address::generate(&f.env);
    let evidence = s(&f.env, "ipfs://QmOutsider");
    let result = escrow.try_submit_work_evidence(&f.escrow_id, &outsider, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ── contract-state gates ──────────────────────────────────────────────────────

/// `Created` (unfunded) contract rejects evidence with `InvalidState`.
#[test]
fn submit_work_evidence_rejects_created_state() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let client = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &soroban_sdk::vec![&env, MILESTONE_ONE],
        &crate::ReleaseAuthorization::ClientOnly,
    );
    // Intentionally NOT depositing — contract remains in Created state.
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Created
    );

    let evidence = s(&env, "ipfs://QmCreated");
    let result = escrow.try_submit_work_evidence(&contract_id, &freelancer, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::InvalidState);
}

/// `Cancelled` contract rejects evidence with `InvalidState`.
///
/// An unfunded contract can be cancelled without a SAC transfer.
#[test]
fn submit_work_evidence_rejects_cancelled_state() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let client = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &soroban_sdk::vec![&env, MILESTONE_ONE],
        &crate::ReleaseAuthorization::ClientOnly,
    );
    // Cancel without funding — no token transfer required.
    assert!(escrow.cancel_contract(&contract_id, &client));
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Cancelled
    );

    let evidence = s(&env, "ipfs://QmCancelled");
    let result = escrow.try_submit_work_evidence(&contract_id, &freelancer, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::InvalidState);
}

/// `Disputed` contract rejects evidence with `InvalidState`.
///
/// A funded contract with an arbiter can be raised into `Disputed` without
/// resolving it, so any evidence submitted after that point would rewrite
/// the audit trail of an in-flight dispute.
#[test]
fn submit_work_evidence_rejects_disputed_state() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let client = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);
    let arbiter = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &Some(arbiter.clone()),
        &soroban_sdk::vec![&env, MILESTONE_ONE],
        &crate::ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client, &MILESTONE_ONE);
    escrow.deposit_funds(&contract_id, &client, &MILESTONE_ONE);
    assert!(escrow.raise_dispute(&contract_id, &client));
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );

    let evidence = s(&env, "ipfs://QmDisputed");
    let result = escrow.try_submit_work_evidence(&contract_id, &freelancer, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::InvalidState);
}

/// `Completed` contract rejects evidence with `InvalidState`.
///
/// Once all milestones are released the contract transitions to `Completed`;
/// any further evidence submission must be blocked to protect the settled
/// audit trail.
#[test]
fn submit_work_evidence_rejects_completed_state() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let client = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &soroban_sdk::vec![&env, MILESTONE_ONE],
        &crate::ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client, &MILESTONE_ONE);
    escrow.deposit_funds(&contract_id, &client, &MILESTONE_ONE);
    escrow.approve_milestone_release(&contract_id, &client, &0);
    escrow.release_milestone(&contract_id, &client, &0);
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Completed
    );

    let evidence = s(&env, "ipfs://QmCompleted");
    let result = escrow.try_submit_work_evidence(&contract_id, &freelancer, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::InvalidState);
}

/// `Refunded` contract rejects evidence with `InvalidState`.
///
/// After all milestones are refunded the contract is in `Refunded` state;
/// further evidence must not be accepted.
#[test]
fn submit_work_evidence_rejects_refunded_state() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let client = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &soroban_sdk::vec![&env, MILESTONE_ONE],
        &crate::ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client, &MILESTONE_ONE);
    escrow.deposit_funds(&contract_id, &client, &MILESTONE_ONE);
    escrow.refund_unreleased_milestones(&contract_id, &soroban_sdk::vec![&env, 0_u32]);
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Refunded
    );

    let evidence = s(&env, "ipfs://QmRefunded");
    let result = escrow.try_submit_work_evidence(&contract_id, &freelancer, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::InvalidState);
}

// ── milestone-state gates ─────────────────────────────────────────────────────

/// A milestone that has been released must reject evidence.
#[test]
fn submit_work_evidence_rejects_released_milestone() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let client = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    // Two milestones — release the first, then try to write evidence to it.
    let amount_a = MILESTONE_ONE;
    let amount_b = MILESTONE_ONE;
    let total = amount_a + amount_b;
    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &soroban_sdk::vec![&env, amount_a, amount_b],
        &crate::ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client, &total);
    escrow.deposit_funds(&contract_id, &client, &total);
    escrow.approve_milestone_release(&contract_id, &client, &0);
    escrow.release_milestone(&contract_id, &client, &0);

    // Contract is still Funded (one remaining milestone). But milestone 0 is released.
    let evidence = s(&env, "ipfs://QmPostRelease");
    let result = escrow.try_submit_work_evidence(&contract_id, &freelancer, &0, &evidence);
    crate::test::assert_contract_error(result, crate::Error::MilestoneAlreadyReleased);
}

/// A milestone that has been individually refunded must reject evidence.
#[test]
fn submit_work_evidence_rejects_refunded_milestone() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let client = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    // Single milestone — refund it, then attempt to write evidence.
    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &soroban_sdk::vec![&env, MILESTONE_ONE],
        &crate::ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client, &MILESTONE_ONE);
    escrow.deposit_funds(&contract_id, &client, &MILESTONE_ONE);

    // Refund only milestone 0 — this also drives the contract to Refunded state.
    let milestone_indices = soroban_sdk::vec![&env, 0_u32];
    escrow.refund_unreleased_milestones(&contract_id, &milestone_indices);

    // Contract is now Refunded; the contract-state gate fires first.
    let evidence = s(&env, "ipfs://QmPostRefund");
    let result = escrow.try_submit_work_evidence(&contract_id, &freelancer, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::InvalidState);
}

// ── evidence string validation ────────────────────────────────────────────────

/// An empty evidence string is rejected with `EmptyEvidence`.
#[test]
fn submit_work_evidence_rejects_empty_string() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    let empty = s(&f.env, "");
    let result = escrow.try_submit_work_evidence(&f.escrow_id, &f.freelancer, &0, &empty);
    crate::test::assert_contract_error(result, crate::Error::EmptyEvidence);
}

/// A single-byte evidence string is the minimum valid length.
#[test]
fn submit_work_evidence_accepts_single_byte() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    let one_byte = s(&f.env, "x");
    assert!(escrow.submit_work_evidence(&f.escrow_id, &f.freelancer, &0, &one_byte));
    assert_eq!(escrow.get_work_evidence(&f.escrow_id, &0), Some(one_byte));
}

/// Exactly 256 bytes is the upper boundary — must be accepted.
#[test]
fn submit_work_evidence_accepts_256_byte_boundary() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    let boundary = String::from_str(&f.env, &"a".repeat(256));
    assert!(escrow.submit_work_evidence(&f.escrow_id, &f.freelancer, &0, &boundary));
    assert_eq!(
        escrow.get_work_evidence(&f.escrow_id, &0).map(|s| s.len()),
        Some(256)
    );
}

/// 257 bytes exceeds the cap — must be rejected with `EvidenceTooLong`.
#[test]
fn submit_work_evidence_rejects_257_byte_string() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    let too_long = String::from_str(&f.env, &"a".repeat(257));
    let result = escrow.try_submit_work_evidence(&f.escrow_id, &f.freelancer, &0, &too_long);
    crate::test::assert_contract_error(result, crate::Error::EvidenceTooLong);
}

// ── overwrite and read-back ───────────────────────────────────────────────────

/// Evidence can be overwritten before milestone release; only the latest
/// value is visible via `get_work_evidence`.
#[test]
fn submit_work_evidence_overwrite_stores_latest_only() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    let first = s(&f.env, "ipfs://QmFirst");
    let second = s(&f.env, "ipfs://QmSecond");
    assert!(escrow.submit_work_evidence(&f.escrow_id, &f.freelancer, &0, &first));
    assert!(escrow.submit_work_evidence(&f.escrow_id, &f.freelancer, &0, &second));
    assert_eq!(escrow.get_work_evidence(&f.escrow_id, &0), Some(second));
}

/// `get_work_evidence` returns `None` before any submission.
#[test]
fn get_work_evidence_returns_none_before_any_submission() {
    let f = EscrowFixtureBuilder::new().funded().build();
    assert!(f.escrow().get_work_evidence(&f.escrow_id, &0).is_none());
}

/// `get_work_evidence` returns `None` for an out-of-bounds milestone index.
#[test]
fn get_work_evidence_returns_none_for_out_of_bounds_index() {
    let f = EscrowFixtureBuilder::new().funded().build();
    assert!(f.escrow().get_work_evidence(&f.escrow_id, &99).is_none());
}

// ── unknown contract ──────────────────────────────────────────────────────────

/// A completely unknown `contract_id` produces `ContractNotFound`.
#[test]
fn submit_work_evidence_rejects_unknown_contract_id() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let evidence = s(&env, "ipfs://QmUnknown");
    let result = escrow.try_submit_work_evidence(&9999, &freelancer, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::ContractNotFound);
}

// ── pause gate ────────────────────────────────────────────────────────────────

/// A paused contract blocks `submit_work_evidence` with `ContractPaused`.
#[test]
fn submit_work_evidence_blocked_while_paused() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    // Pause requires admin auth; mock_all_auths covers it.
    escrow.pause();

    let evidence = s(&f.env, "ipfs://QmPaused");
    let result = escrow.try_submit_work_evidence(&f.escrow_id, &f.freelancer, &0, &evidence);
    crate::test::assert_contract_error(result, EscrowError::ContractPaused);
}

/// After unpausing the same call is accepted.
#[test]
fn submit_work_evidence_accepted_after_unpause() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    escrow.pause();
    escrow.unpause();

    let evidence = s(&f.env, "ipfs://QmUnpaused");
    assert!(escrow.submit_work_evidence(&f.escrow_id, &f.freelancer, &0, &evidence));
}

// ── index-out-of-bounds ───────────────────────────────────────────────────────

/// Submitting evidence for a non-existent milestone index is rejected.
#[test]
fn submit_work_evidence_rejects_out_of_bounds_index() {
    let f = EscrowFixtureBuilder::new().funded().build();
    let escrow = f.escrow();
    let evidence = s(&f.env, "ipfs://QmBadIndex");
    let result = escrow.try_submit_work_evidence(&f.escrow_id, &f.freelancer, &99, &evidence);
    crate::test::assert_contract_error(result, crate::Error::IndexOutOfBounds);
}

// ── multi-milestone correctness ───────────────────────────────────────────────

/// Evidence is stored per-milestone; writing to index 1 does not overwrite
/// index 0, and vice-versa.
#[test]
fn submit_work_evidence_independent_per_milestone() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = soroban_sdk::Address::generate(&env);
    let client = soroban_sdk::Address::generate(&env);
    let freelancer = soroban_sdk::Address::generate(&env);

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    let total = MILESTONE_ONE * 2;
    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &soroban_sdk::vec![&env, MILESTONE_ONE, MILESTONE_ONE],
        &crate::ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client, &total);
    escrow.deposit_funds(&contract_id, &client, &total);

    let ev0 = s(&env, "ipfs://QmMilestone0");
    let ev1 = s(&env, "ipfs://QmMilestone1");
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer, &0, &ev0));
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer, &1, &ev1));

    assert_eq!(escrow.get_work_evidence(&contract_id, &0), Some(ev0));
    assert_eq!(escrow.get_work_evidence(&contract_id, &1), Some(ev1));
}
