use super::{
    assert_contract_error, default_milestones, generated_participants, register_client,
    total_milestone_amount,
};
use crate::{ContractStatus, Error, EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env, String};

// ---------------------------------------------------------------------------
// Internal helper: build a funded escrow with a bound SAC token.
// Returns (EscrowClient, escrow_id, client_addr, freelancer_addr).
// ---------------------------------------------------------------------------
fn funded_setup(
    env: &Env,
) -> (crate::EscrowClient<'_>, u32, Address, Address) {
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(env, &escrow_addr);
    let admin = Address::generate(env);
    escrow.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let total = total_milestone_amount();
    StellarAssetClient::new(env, &token).mint(&client_addr, &total);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(env),
        &ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&contract_id, &client_addr, &total);
    (escrow, contract_id, client_addr, freelancer_addr)
}

// ===========================================================================
// Existing access-control tests (fixed: import names, tuple arity, SAC setup)
// ===========================================================================

#[test]
fn test_only_client_can_deposit_funds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let result =
        client.try_deposit_funds(&contract_id, &freelancer_addr, &total_milestone_amount());
    assert_eq!(result, Err(Ok(Error::UnauthorizedRole)));
}

#[test]
fn test_freelancer_cannot_approve_milestone_release() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _client_addr, freelancer_addr) = funded_setup(&env);
    let result = escrow.try_approve_milestone_release(&contract_id, &freelancer_addr, &0);
    assert_eq!(result, Err(Ok(Error::UnauthorizedRole)));
}

#[test]
fn test_freelancer_cannot_release_milestone() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, freelancer_addr) = funded_setup(&env);
    escrow.approve_milestone_release(&contract_id, &client_addr, &0);
    let result = escrow.try_release_milestone(&contract_id, &freelancer_addr, &0);
    assert_eq!(result, Err(Ok(Error::UnauthorizedRole)));
}

#[test]
fn test_only_client_can_issue_reputation() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, freelancer_addr) = funded_setup(&env);
    for i in 0..3u32 {
        escrow.approve_milestone_release(&contract_id, &client_addr, &i);
        escrow.release_milestone(&contract_id, &client_addr, &i);
    }
    let result =
        escrow.try_issue_reputation(&contract_id, &freelancer_addr, &freelancer_addr, &5);
    assert_eq!(result, Err(Ok(Error::UnauthorizedRole)));
}

#[test]
fn test_issue_reputation_rejects_freelancer_mismatch() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, freelancer_addr) = funded_setup(&env);
    let wrong_freelancer = Address::generate(&env);
    for i in 0..3u32 {
        escrow.approve_milestone_release(&contract_id, &client_addr, &i);
        escrow.release_milestone(&contract_id, &client_addr, &i);
    }
    let result =
        escrow.try_issue_reputation(&contract_id, &client_addr, &wrong_freelancer, &5);
    assert_eq!(result, Err(Ok(Error::FreelancerMismatch)));
}

#[test]
fn test_create_rejects_arbiter_modes_without_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    assert_eq!(result, Err(Ok(Error::MissingArbiter)));
}

#[test]
fn test_create_rejects_invalid_arbiter_role_overlap() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(client_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert_eq!(result, Err(Ok(Error::InvalidArbiter)));
}

#[test]
#[should_panic]
fn test_create_contract_requires_authentication_of_roles() {
    let env = Env::default();
    // Intentionally no mock_all_auths — auth must be provided.
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
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
    let (client_addr, _) = generated_participants(&env);
    let result = client.try_create_contract(
        &client_addr,
        &client_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(result, Err(Ok(Error::InvalidParticipants)));
}

#[test]
fn test_create_rejects_empty_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let empty = soroban_sdk::Vec::<i128>::new(&env);
    let result = client.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &empty,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(result, Err(Ok(Error::EmptyMilestones)));
}

#[test]
fn test_deposit_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let result = client.try_deposit_funds(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(Error::AmountMustBePositive)));
}

#[test]
fn test_deposit_rejects_when_contract_already_funded() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, _) = funded_setup(&env);
    // Contract is now Funded; a second deposit must be rejected.
    let result = escrow.try_deposit_funds(&contract_id, &client_addr, &total_milestone_amount());
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_approve_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let result = client.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_approve_rejects_already_released_milestone() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, _) = funded_setup(&env);
    escrow.approve_milestone_release(&contract_id, &client_addr, &0);
    escrow.release_milestone(&contract_id, &client_addr, &0);
    let result = escrow.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(Error::MilestoneAlreadyReleased)));
}

#[test]
fn test_approve_rejects_duplicate_client_approval() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, _) = funded_setup(&env);
    escrow.approve_milestone_release(&contract_id, &client_addr, &0);
    let result = escrow.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(Error::AlreadyApproved)));
}

#[test]
fn test_approve_rejects_duplicate_arbiter_approval() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let total = total_milestone_amount();
    StellarAssetClient::new(&env, &token).mint(&client_addr, &total);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    escrow.deposit_funds(&contract_id, &client_addr, &total);
    escrow.approve_milestone_release(&contract_id, &arbiter_addr, &0);
    let result = escrow.try_approve_milestone_release(&contract_id, &arbiter_addr, &0);
    assert_eq!(result, Err(Ok(Error::AlreadyApproved)));
}

#[test]
fn test_release_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let result = client.try_release_milestone(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_release_rejects_already_released_milestone() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, _) = funded_setup(&env);
    escrow.approve_milestone_release(&contract_id, &client_addr, &0);
    escrow.release_milestone(&contract_id, &client_addr, &0);
    let result = escrow.try_release_milestone(&contract_id, &client_addr, &0);
    assert_eq!(result, Err(Ok(Error::MilestoneAlreadyReleased)));
}

#[test]
fn test_issue_reputation_rejects_invalid_rating() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, freelancer_addr) = funded_setup(&env);
    for i in 0..3u32 {
        escrow.approve_milestone_release(&contract_id, &client_addr, &i);
        escrow.release_milestone(&contract_id, &client_addr, &i);
    }
    let result = escrow.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &0);
    assert_eq!(result, Err(Ok(Error::InvalidRating)));
}

#[test]
fn test_issue_reputation_requires_completed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_issue_reputation_rejects_duplicate_issuance() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, freelancer_addr) = funded_setup(&env);
    for i in 0..3u32 {
        escrow.approve_milestone_release(&contract_id, &client_addr, &i);
        escrow.release_milestone(&contract_id, &client_addr, &i);
    }
    assert!(escrow.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5));
    let result = escrow.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &4);
    assert_eq!(result, Err(Ok(Error::ReputationAlreadyIssued)));
}

#[test]
fn test_client_and_arbiter_mode_rejects_third_party_approval() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let outsider = Address::generate(&env);
    let total = total_milestone_amount();
    StellarAssetClient::new(&env, &token).mint(&client_addr, &total);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    escrow.deposit_funds(&contract_id, &client_addr, &total);
    let result = escrow.try_approve_milestone_release(&contract_id, &outsider, &0);
    assert_eq!(result, Err(Ok(Error::UnauthorizedRole)));
}

#[test]
fn test_arbiter_only_flow_enforces_arbiter_approval_and_release() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let total = total_milestone_amount();
    StellarAssetClient::new(&env, &token).mint(&client_addr, &total);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    escrow.deposit_funds(&contract_id, &client_addr, &total);
    // Client cannot approve in ArbiterOnly mode.
    let r = escrow.try_approve_milestone_release(&contract_id, &client_addr, &0);
    assert_eq!(r, Err(Ok(Error::UnauthorizedRole)));
    assert!(escrow.approve_milestone_release(&contract_id, &arbiter_addr, &0));
    assert!(escrow.release_milestone(&contract_id, &arbiter_addr, &0));
}

// ===========================================================================
// submit_work_evidence — #745: caller-identity and milestone-state gates
// ===========================================================================

// ---------------------------------------------------------------------------
// Caller-identity gates
// ---------------------------------------------------------------------------

/// The client must be rejected; only the stored freelancer may submit evidence.
#[test]
fn submit_work_evidence_rejects_client_as_caller() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, _) = funded_setup(&env);
    let evidence = String::from_str(&env, "ipfs://QmClientAttempt");
    let result = escrow.try_submit_work_evidence(&contract_id, &client_addr, &0, &evidence);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// An arbiter must be rejected; they cannot submit evidence on a freelancer's behalf.
#[test]
fn submit_work_evidence_rejects_arbiter_as_caller() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let total = total_milestone_amount();
    StellarAssetClient::new(&env, &token).mint(&client_addr, &total);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&contract_id, &client_addr, &total);
    let evidence = String::from_str(&env, "ipfs://QmArbiterAttempt");
    let result = escrow.try_submit_work_evidence(&contract_id, &arbiter_addr, &0, &evidence);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// A random third party must be rejected with `UnauthorizedRole`.
#[test]
fn submit_work_evidence_rejects_third_party_as_caller() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _, _) = funded_setup(&env);
    let outsider = Address::generate(&env);
    let evidence = String::from_str(&env, "ipfs://QmOutsider");
    let result = escrow.try_submit_work_evidence(&contract_id, &outsider, &0, &evidence);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ---------------------------------------------------------------------------
// Milestone-state gates
// ---------------------------------------------------------------------------

/// Evidence must not be accepted for a milestone that has already been released.
/// The on-chain audit trail for a settled payment must be immutable.
#[test]
fn submit_work_evidence_rejects_released_milestone() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, freelancer_addr) = funded_setup(&env);
    escrow.approve_milestone_release(&contract_id, &client_addr, &0);
    escrow.release_milestone(&contract_id, &client_addr, &0);
    let evidence = String::from_str(&env, "ipfs://QmAfterRelease");
    let result =
        escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    assert_contract_error(result, Error::MilestoneAlreadyReleased);
}

/// Evidence must not be accepted for a milestone that has already been refunded.
#[test]
fn submit_work_evidence_rejects_refunded_milestone() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // Two milestones; deposit only the first so the contract is PartiallyFunded.
    let partial = 100_i128;
    StellarAssetClient::new(&env, &token).mint(&client_addr, &partial);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&contract_id, &client_addr, &partial);
    // Refund milestone 0 while PartiallyFunded.
    escrow.refund_unreleased_milestones(&contract_id, &vec![&env, 0_u32]);
    let evidence = String::from_str(&env, "ipfs://QmAfterRefund");
    let result =
        escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    assert_contract_error(result, EscrowError::AlreadyRefunded);
}

// ---------------------------------------------------------------------------
// Contract-state gates
// ---------------------------------------------------------------------------

/// A cancelled contract must reject evidence submissions.
#[test]
fn submit_work_evidence_rejects_cancelled_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    // Cancel before any deposit — no SAC transfer needed.
    escrow.cancel_contract(&contract_id, &client_addr);
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Cancelled
    );
    let evidence = String::from_str(&env, "ipfs://QmCancelledContract");
    let result =
        escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    assert_contract_error(result, EscrowError::InvalidState);
}

/// A disputed contract must reject evidence submissions.
#[test]
fn submit_work_evidence_rejects_disputed_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&client_addr, &100_i128);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&contract_id, &client_addr, &100_i128);
    escrow.raise_dispute(&contract_id, &client_addr);
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
    let evidence = String::from_str(&env, "ipfs://QmDisputedContract");
    let result =
        escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    assert_contract_error(result, EscrowError::InvalidState);
}

/// A completed contract (all milestones released) must reject evidence.
#[test]
fn submit_work_evidence_rejects_completed_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, freelancer_addr) = funded_setup(&env);
    for i in 0..3u32 {
        escrow.approve_milestone_release(&contract_id, &client_addr, &i);
        escrow.release_milestone(&contract_id, &client_addr, &i);
    }
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::Completed
    );
    let evidence = String::from_str(&env, "ipfs://QmCompletedContract");
    let result =
        escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    assert_contract_error(result, EscrowError::InvalidState);
}

/// A finalized contract must reject evidence with `AlreadyFinalized`.
#[test]
fn submit_work_evidence_rejects_finalized_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, client_addr, freelancer_addr) = funded_setup(&env);
    for i in 0..3u32 {
        escrow.approve_milestone_release(&contract_id, &client_addr, &i);
        escrow.release_milestone(&contract_id, &client_addr, &i);
    }
    escrow.finalize_contract(&contract_id, &client_addr);
    let evidence = String::from_str(&env, "ipfs://QmFinalizedContract");
    let result =
        escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    assert_contract_error(result, EscrowError::AlreadyFinalized);
}

/// A contract still in `Created` state (never funded) must reject evidence.
#[test]
fn submit_work_evidence_rejects_created_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    let evidence = String::from_str(&env, "ipfs://QmCreatedContract");
    let result = escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    assert_contract_error(result, EscrowError::InvalidState);
}

// ---------------------------------------------------------------------------
// Evidence-content gates
// ---------------------------------------------------------------------------

/// An empty evidence string must be rejected with `EvidenceEmpty`.
/// Empty submissions would silently corrupt a prior valid entry.
#[test]
fn submit_work_evidence_rejects_empty_string() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _, freelancer_addr) = funded_setup(&env);
    let empty = String::from_str(&env, "");
    let result = escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &empty);
    assert_contract_error(result, Error::EvidenceEmpty);
}

/// Evidence exceeding 256 bytes must be rejected with `EvidenceTooLong`.
#[test]
fn submit_work_evidence_rejects_257_bytes() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _, freelancer_addr) = funded_setup(&env);
    let too_long = String::from_str(&env, &"x".repeat(257));
    let result =
        escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &too_long);
    assert_contract_error(result, Error::EvidenceTooLong);
}

/// Exactly 256 bytes must be accepted (upper boundary).
#[test]
fn submit_work_evidence_accepts_exactly_256_bytes() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _, freelancer_addr) = funded_setup(&env);
    let boundary = String::from_str(&env, &"x".repeat(256));
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer_addr, &0, &boundary));
    assert_eq!(
        escrow.get_work_evidence(&contract_id, &0).map(|s| s.len()),
        Some(256)
    );
}

/// An out-of-bounds milestone index must be rejected with `IndexOutOfBounds`.
#[test]
fn submit_work_evidence_rejects_out_of_bounds_index() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _, freelancer_addr) = funded_setup(&env);
    // Default fixture has 3 milestones (indices 0–2); index 3 is out of bounds.
    let evidence = String::from_str(&env, "ipfs://QmBoundsCheck");
    let result =
        escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &3, &evidence);
    assert_contract_error(result, Error::IndexOutOfBounds);
}

// ---------------------------------------------------------------------------
// Happy-path and functional tests
// ---------------------------------------------------------------------------

/// The freelancer can submit evidence for a fully-funded contract and the
/// value is readable back via `get_work_evidence`.
#[test]
fn submit_work_evidence_succeeds_for_funded_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _, freelancer_addr) = funded_setup(&env);
    let evidence = String::from_str(&env, "ipfs://QmHappyPath");
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence));
    assert_eq!(
        escrow.get_work_evidence(&contract_id, &0),
        Some(evidence)
    );
}

/// Evidence submission succeeds when the contract is only PartiallyFunded
/// (a valid pre-release state where work may already be in progress).
#[test]
fn submit_work_evidence_succeeds_for_partially_funded_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // Deposit less than the full total so the contract stays PartiallyFunded.
    let partial = 100_i128;
    StellarAssetClient::new(&env, &token).mint(&client_addr, &partial);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&contract_id, &client_addr, &partial);
    assert_eq!(
        escrow.get_contract(&contract_id).status,
        ContractStatus::PartiallyFunded
    );
    let evidence = String::from_str(&env, "ipfs://QmPartialFunded");
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence));
    assert_eq!(
        escrow.get_work_evidence(&contract_id, &0),
        Some(evidence)
    );
}

/// The freelancer may overwrite evidence before a milestone is released;
/// only the latest value must be visible.
#[test]
fn submit_work_evidence_overwrites_previous_value() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _, freelancer_addr) = funded_setup(&env);
    let first = String::from_str(&env, "ipfs://QmFirst");
    let second = String::from_str(&env, "ipfs://QmSecond");
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer_addr, &0, &first));
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer_addr, &0, &second));
    assert_eq!(
        escrow.get_work_evidence(&contract_id, &0),
        Some(second)
    );
}

/// Evidence for different milestones is stored independently.
#[test]
fn submit_work_evidence_stores_per_milestone() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _, freelancer_addr) = funded_setup(&env);
    let ev0 = String::from_str(&env, "ipfs://QmMilestone0");
    let ev1 = String::from_str(&env, "ipfs://QmMilestone1");
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer_addr, &0, &ev0));
    assert!(escrow.submit_work_evidence(&contract_id, &freelancer_addr, &1, &ev1));
    assert_eq!(escrow.get_work_evidence(&contract_id, &0), Some(ev0));
    assert_eq!(escrow.get_work_evidence(&contract_id, &1), Some(ev1));
    // Milestone 2 was never touched — must return None.
    assert_eq!(escrow.get_work_evidence(&contract_id, &2), None);
}

/// An emergency-mode contract must reject evidence submissions.
///
/// `activate_emergency_pause` sets both the `Paused` and `Emergency` flags.
/// The `require_not_paused` guard checks `Paused` first, so `ContractPaused`
/// is the expected error regardless of whether emergency mode is active alone
/// or combined with the paused flag.
#[test]
fn submit_work_evidence_rejects_emergency_mode() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, contract_id, _, freelancer_addr) = funded_setup(&env);
    // Activate emergency mode (also sets Paused=true).
    escrow.activate_emergency_pause();
    let evidence = String::from_str(&env, "ipfs://QmEmergency");
    let result =
        escrow.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    // `require_not_paused` checks the Paused flag first, so ContractPaused is emitted.
    assert_contract_error(result, Error::ContractPaused);
}