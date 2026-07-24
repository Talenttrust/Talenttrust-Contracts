//! Access-control and milestone-state gating tests for `submit_work_evidence`.
//!
//! # Coverage matrix
//!
//! | Scenario                                           | Expected result             |
//! | -------------------------------------------------- | --------------------------- |
//! | Freelancer submits to funded milestone             | `Ok(true)`                  |
//! | Freelancer overwrites evidence before release      | `Ok(true)`, new value       |
//! | Evidence at exactly 256 bytes accepted             | `Ok(true)`                  |
//! | Evidence at 257 bytes rejected                     | `EvidenceTooLong`           |
//! | Empty evidence string rejected                     | `EmptyEvidence`             |
//! | Single-byte evidence accepted                      | `Ok(true)`                  |
//! | Client submits — rejected                          | `UnauthorizedRole`          |
//! | Arbiter submits — rejected                         | `UnauthorizedRole`          |
//! | Third-party submits — rejected                     | `UnauthorizedRole`          |
//! | Contract in Created state — rejected               | `InvalidState`              |
//! | Contract in Cancelled state — rejected             | `InvalidState`              |
//! | Contract in Completed state — rejected             | `InvalidState`              |
//! | Contract in Refunded state — rejected              | `InvalidState`              |
//! | Contract in Disputed state — rejected              | `InvalidState`              |
//! | Milestone already released — rejected              | `MilestoneAlreadyReleased`  |
//! | Milestone already refunded — rejected              | `AlreadyRefunded`           |
//! | Milestone index out of bounds — rejected           | `IndexOutOfBounds`          |
//! | Finalized contract — rejected                      | `AlreadyFinalized`          |
//! | Paused contract — rejected                         | `ContractPaused`            |
//! | Emergency active — rejected                        | `EmergencyActive`           |
//! | Unknown contract ID — rejected                     | `ContractNotFound`          |
//! | Role check fires before length check               | `UnauthorizedRole`          |
//! | State check fires before empty-evidence check      | `InvalidState`              |
//! | Evidence is stored per-milestone independently     | `Ok(true)`                  |
//! | `get_work_evidence` returns `None` before submit   | `None`                      |
//! | `get_work_evidence` returns `None` for OOB index   | `None`                      |

use super::{assert_contract_error, EscrowFixtureBuilder};
use crate::{ContractStatus, Error, EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env, String};

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

/// Build a Soroban `String` from a plain `&str`.
fn ev(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

// ---------------------------------------------------------------------------
// Happy-path: successful submissions
// ---------------------------------------------------------------------------

/// The freelancer can submit evidence for a funded, unreleased milestone.
/// `get_work_evidence` must return the submitted value unchanged.
#[test]
fn freelancer_can_submit_evidence_to_funded_milestone() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let evidence = ev(&fixture.env, "ipfs://QmCorrectFreelancer");
    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &evidence));

    assert_eq!(
        escrow.get_work_evidence(&fixture.escrow_id, &0),
        Some(evidence)
    );
}

/// The freelancer may overwrite evidence on the same milestone before it is
/// released. Only the most recent value must be visible.
#[test]
fn freelancer_can_overwrite_evidence_before_release() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let first = ev(&fixture.env, "ipfs://first-version");
    let second = ev(&fixture.env, "ipfs://second-version");

    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &first));
    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &second));

    // Only the latest value must be visible.
    assert_eq!(
        escrow.get_work_evidence(&fixture.escrow_id, &0),
        Some(second)
    );
}

/// Evidence of exactly 256 bytes (the upper bound) must be accepted.
#[test]
fn evidence_at_exactly_256_bytes_is_accepted() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let boundary = String::from_str(&fixture.env, &"x".repeat(256));
    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &boundary));

    let stored = escrow.get_work_evidence(&fixture.escrow_id, &0);
    assert_eq!(stored.map(|s| s.len()), Some(256_u32));
}

/// A single ASCII character (1 byte) is above the empty-string floor and below
/// the 256-byte ceiling — it must be accepted.
#[test]
fn single_char_evidence_is_accepted() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let one_byte = ev(&fixture.env, "x");
    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &one_byte));

    assert_eq!(
        escrow.get_work_evidence(&fixture.escrow_id, &0),
        Some(one_byte)
    );
}

/// Evidence can be submitted independently to each milestone in the same contract.
#[test]
fn evidence_can_be_submitted_to_each_milestone_independently() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let e0 = ev(&fixture.env, "ipfs://QmMilestone0");
    let e1 = ev(&fixture.env, "ipfs://QmMilestone1");
    let e2 = ev(&fixture.env, "ipfs://QmMilestone2");

    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &e0));
    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &1, &e1));
    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &2, &e2));

    assert_eq!(escrow.get_work_evidence(&fixture.escrow_id, &0), Some(e0));
    assert_eq!(escrow.get_work_evidence(&fixture.escrow_id, &1), Some(e1));
    assert_eq!(escrow.get_work_evidence(&fixture.escrow_id, &2), Some(e2));
}

// ---------------------------------------------------------------------------
// Caller access control
// ---------------------------------------------------------------------------

/// The client must not be allowed to submit evidence; only the freelancer may.
#[test]
fn client_cannot_submit_evidence() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.client,
        &0,
        &ev(&fixture.env, "ipfs://QmClientBad"),
    );
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// An arbiter must not be allowed to submit evidence.
#[test]
fn arbiter_cannot_submit_evidence() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let arbiter = Address::generate(&env);

    let escrow_address = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_address);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    let milestones = vec![&env, 200_0000000_i128];
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &Some(arbiter.clone()),
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client, &200_0000000_i128);
    escrow.deposit_funds(&id, &client, &200_0000000_i128);

    let result =
        escrow.try_submit_work_evidence(&id, &arbiter, &0, &ev(&env, "ipfs://QmArbiterBad"));
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// A random third-party address that is not a contract participant must be
/// rejected with `UnauthorizedRole`.
#[test]
fn third_party_cannot_submit_evidence() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();
    let outsider = Address::generate(&fixture.env);

    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &outsider,
        &0,
        &ev(&fixture.env, "ipfs://QmOutsider"),
    );
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

// ---------------------------------------------------------------------------
// Evidence string validation
// ---------------------------------------------------------------------------

/// Evidence of 257 bytes must be rejected with `EvidenceTooLong`.
#[test]
fn evidence_at_257_bytes_is_rejected() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let too_long = String::from_str(&fixture.env, &"x".repeat(257));
    let result =
        escrow.try_submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &too_long);
    assert_contract_error(result, Error::EvidenceTooLong);
}

/// An empty evidence string (0 bytes) must be rejected with `EmptyEvidence`.
/// This prevents silently overwriting an existing valid evidence entry with
/// a blank value.
#[test]
fn empty_evidence_string_is_rejected() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let empty = String::from_str(&fixture.env, "");
    let result =
        escrow.try_submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &empty);
    assert_contract_error(result, Error::EmptyEvidence);
}

// ---------------------------------------------------------------------------
// Contract-status gates
// ---------------------------------------------------------------------------

/// Evidence submission is rejected when the contract is still in `Created`
/// state (before the full deposit has been received).
#[test]
fn created_contract_rejects_evidence_submission() {
    // Build without the funded step so the contract stays in Created.
    let fixture = EscrowFixtureBuilder::new().with_settlement_token().build();
    let escrow = fixture.escrow();

    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &0,
        &ev(&fixture.env, "ipfs://QmCreated"),
    );
    assert_contract_error(result, EscrowError::InvalidState);
}

/// Evidence submission is rejected when the contract has been cancelled.
/// A cancelled contract must not allow audit-trail modification.
#[test]
fn cancelled_contract_rejects_evidence_submission() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    // Cancel the fully-funded contract (no milestones released yet).
    escrow.cancel_contract(&fixture.escrow_id, &fixture.client);

    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Cancelled
    );

    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &0,
        &ev(&fixture.env, "ipfs://QmCancelled"),
    );
    assert_contract_error(result, EscrowError::InvalidState);
}

/// Evidence submission is rejected when the contract is in `Completed` state
/// (all milestones released or refunded).
#[test]
fn completed_contract_rejects_evidence_submission() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    // Release all three milestones to reach Completed.
    for idx in 0..3u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &idx);
        escrow.release_milestone(&fixture.escrow_id, &fixture.client, &idx);
    }

    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Completed
    );

    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &0,
        &ev(&fixture.env, "ipfs://QmCompleted"),
    );
    assert_contract_error(result, EscrowError::InvalidState);
}

/// Evidence submission is rejected when the contract is fully `Refunded`.
#[test]
fn refunded_contract_rejects_evidence_submission() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    // Refund all milestones (no deadlines set, so unconditional refund).
    escrow.refund_unreleased_milestones(&fixture.escrow_id, &vec![&fixture.env, 0u32, 1u32, 2u32]);

    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Refunded
    );

    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &0,
        &ev(&fixture.env, "ipfs://QmRefunded"),
    );
    assert_contract_error(result, EscrowError::InvalidState);
}

/// Evidence submission is rejected when the contract is in `Disputed` state.
/// A disputed contract must not allow the freelancer to rewrite evidence while
/// the arbiter is deliberating.
#[test]
fn disputed_contract_rejects_evidence_submission() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let arbiter = Address::generate(&env);

    let escrow_address = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_address);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &Some(arbiter),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let total = 600_0000000_i128;
    StellarAssetClient::new(&env, &token).mint(&client, &total);
    escrow.deposit_funds(&id, &client, &total);

    // Open a dispute to move the contract to Disputed.
    escrow.raise_dispute(&id, &client);
    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Disputed);

    let result =
        escrow.try_submit_work_evidence(&id, &freelancer, &0, &ev(&env, "ipfs://QmDisputed"));
    assert_contract_error(result, EscrowError::InvalidState);
}

// ---------------------------------------------------------------------------
// Milestone-state gates
// ---------------------------------------------------------------------------

/// A milestone that has already been released must not accept new evidence.
/// This is the primary audit-trail integrity guard: releasing a milestone
/// finalizes the payment; allowing evidence writes afterwards would
/// silently corrupt the on-chain audit record.
#[test]
fn released_milestone_rejects_evidence_submission() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    // Release milestone 0.
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0);

    // The contract is still Funded (milestones 1 and 2 remain).
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );

    // Attempting to submit evidence to the released milestone must be rejected.
    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &0,
        &ev(&fixture.env, "ipfs://QmReleasedBad"),
    );
    assert_contract_error(result, Error::MilestoneAlreadyReleased);
}

/// A milestone that has already been refunded must not accept new evidence.
/// Refunded milestones have settled back to the client; their audit trail
/// must not be modified.
#[test]
fn refunded_milestone_rejects_evidence_submission() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);

    let escrow_address = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_address);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    // Two-milestone contract: refund the first, leave the second pending.
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let total = 300_0000000_i128;
    StellarAssetClient::new(&env, &token).mint(&client, &total);
    escrow.deposit_funds(&id, &client, &total);

    // Refund only milestone 0; milestone 1 remains unreleased → contract stays Funded.
    escrow.refund_unreleased_milestones(&id, &vec![&env, 0u32]);

    // Contract is still Funded (milestone 1 pending).
    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Funded);

    // Attempt to submit evidence for the refunded milestone must be rejected.
    let result = escrow.try_submit_work_evidence(
        &id,
        &freelancer,
        &0,
        &ev(&env, "ipfs://QmRefundedMilestone"),
    );
    assert_contract_error(result, EscrowError::AlreadyRefunded);
}

/// An out-of-bounds milestone index must be rejected with `IndexOutOfBounds`.
/// This prevents storage writes to non-existent milestone slots.
#[test]
fn out_of_bounds_milestone_index_is_rejected() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    // The default contract has 3 milestones (indices 0–2); index 99 is out of bounds.
    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &99,
        &ev(&fixture.env, "ipfs://QmOOB"),
    );
    assert_contract_error(result, Error::IndexOutOfBounds);
}

// ---------------------------------------------------------------------------
// Contract finalization gate
// ---------------------------------------------------------------------------

/// A finalized contract must reject evidence submission with `AlreadyFinalized`.
/// Finalization creates an immutable close record; allowing evidence writes
/// afterwards would contradict that immutability guarantee.
#[test]
fn finalized_contract_rejects_evidence_submission() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    // Release all milestones to reach Completed, then finalize.
    for idx in 0..3u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &idx);
        escrow.release_milestone(&fixture.escrow_id, &fixture.client, &idx);
    }
    escrow.finalize_contract(&fixture.escrow_id, &fixture.client);

    // Any subsequent submit_work_evidence must be rejected.
    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &0,
        &ev(&fixture.env, "ipfs://QmFinalized"),
    );
    // require_not_finalized in finalize.rs fires Error::AlreadyFinalized (types.rs, code 46)
    assert_contract_error(result, Error::AlreadyFinalized);
}

// ---------------------------------------------------------------------------
// Pause / emergency gates
// ---------------------------------------------------------------------------

/// A paused contract must reject evidence submission with `ContractPaused`.
/// Pause is a safety rail that must block all mutating operations.
#[test]
fn paused_contract_rejects_evidence_submission() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    escrow.pause();

    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &0,
        &ev(&fixture.env, "ipfs://QmPaused"),
    );
    // require_not_paused in finalize.rs fires Error::ContractPaused (types.rs, code 37)
    assert_contract_error(result, Error::ContractPaused);
}

/// An active emergency pause must reject evidence submission.
/// `activate_emergency_pause` sets both Paused and Emergency flags.
/// `require_not_paused` checks Paused first → `Error::ContractPaused`.
#[test]
fn emergency_active_rejects_evidence_submission() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    escrow.activate_emergency_pause();

    let result = escrow.try_submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &0,
        &ev(&fixture.env, "ipfs://QmEmergency"),
    );
    // require_not_paused checks Paused before Emergency, so ContractPaused fires
    // (both flags are set by activate_emergency_pause).
    assert_contract_error(result, Error::ContractPaused);
}

// ---------------------------------------------------------------------------
// Unknown contract
// ---------------------------------------------------------------------------

/// Submitting evidence for a contract ID that has never been allocated must
/// panic with `ContractNotFound`.
#[test]
fn unknown_contract_id_rejects_evidence_submission() {
    let fixture = EscrowFixtureBuilder::new().build();
    let escrow = fixture.escrow();
    let caller = Address::generate(&fixture.env);

    // Contract ID 9999 was never created.
    let result =
        escrow.try_submit_work_evidence(&9999, &caller, &0, &ev(&fixture.env, "ipfs://QmNotFound"));
    assert_contract_error(result, EscrowError::ContractNotFound);
}

// ---------------------------------------------------------------------------
// Error ordering: guards fire in documented order
// ---------------------------------------------------------------------------

/// When the caller is wrong AND the evidence is too long, the role check fires
/// first — the caller gate must not be skipped just because evidence is invalid.
#[test]
fn role_check_fires_before_length_check() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let too_long = String::from_str(&fixture.env, &"x".repeat(300));
    // Client (wrong role) + oversized evidence → UnauthorizedRole, not EvidenceTooLong.
    let result =
        escrow.try_submit_work_evidence(&fixture.escrow_id, &fixture.client, &0, &too_long);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// When the contract is in the wrong state AND the evidence is empty, the state
/// check fires before the empty-string check.
#[test]
fn state_check_fires_before_empty_evidence_check() {
    // Build without funding so the contract stays in Created.
    let fixture = EscrowFixtureBuilder::new().with_settlement_token().build();
    let escrow = fixture.escrow();

    let empty = String::from_str(&fixture.env, "");
    // Created contract (wrong state) + empty evidence → InvalidState, not EmptyEvidence.
    let result =
        escrow.try_submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &empty);
    assert_contract_error(result, EscrowError::InvalidState);
}

/// When the evidence is empty, `EmptyEvidence` fires before `EvidenceTooLong`
/// (a zero-length string also trivially passes the length bound; we verify
/// that the empty check is not accidentally skipped).
#[test]
fn empty_check_fires_independently_for_empty_string() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let empty = String::from_str(&fixture.env, "");
    let result =
        escrow.try_submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &empty);
    assert_contract_error(result, Error::EmptyEvidence);
}

// ---------------------------------------------------------------------------
// Read-back helpers: get_work_evidence boundary conditions
// ---------------------------------------------------------------------------

/// `get_work_evidence` returns `None` before any evidence has been submitted
/// for a milestone.
#[test]
fn get_work_evidence_returns_none_before_submission() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    assert!(escrow.get_work_evidence(&fixture.escrow_id, &0).is_none());
}

/// `get_work_evidence` returns `None` for an out-of-bounds milestone index
/// without panicking (non-panicking boundary for the read path).
#[test]
fn get_work_evidence_returns_none_for_out_of_bounds_index() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    assert!(escrow.get_work_evidence(&fixture.escrow_id, &100).is_none());
}

/// Evidence submitted to one milestone must not appear on any other milestone.
/// Milestones must be isolated from each other's evidence store.
#[test]
fn evidence_is_stored_per_milestone_independently() {
    let fixture = EscrowFixtureBuilder::new().funded().build();
    let escrow = fixture.escrow();

    let e1 = ev(&fixture.env, "ipfs://QmOnlyForMilestone1");
    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &1, &e1));

    // Milestones 0 and 2 must have no evidence.
    assert!(escrow.get_work_evidence(&fixture.escrow_id, &0).is_none());
    assert!(escrow.get_work_evidence(&fixture.escrow_id, &2).is_none());

    // Milestone 1 must have the submitted evidence.
    assert_eq!(escrow.get_work_evidence(&fixture.escrow_id, &1), Some(e1));
}
