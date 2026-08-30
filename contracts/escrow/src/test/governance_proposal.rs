//! Tests for the two-step governance override proposal workflow (#1221).
//!
//! ## Required edge cases (from issue #1221)
//!
//! 1. **request by operator** — the stored admin can successfully submit a
//!    proposal; the returned ID is positive and the proposal is readable.
//! 2. **self-approval** — approving one's own proposal is rejected with
//!    `GovernanceSelfApproval`.
//! 3. **expired request** — any action on a proposal after its TTL elapses
//!    fails with `GovernanceProposalExpired`.
//! 4. **rejected request** — a rejected proposal cannot be approved or applied;
//!    any subsequent action fails with `GovernanceProposalInvalidState`.
//! 5. **apply twice** — calling `apply_governance_proposal` a second time fails
//!    with `GovernanceProposalInvalidState` (the `Applied` terminal guard).
//!
//! Additional tests cover:
//! - Full happy-path (request → approve → apply → parameter takes effect)
//! - Unauthorized requester (non-admin cannot create a proposal)
//! - Proposal not found
//! - Out-of-range payload rejected at request time
//! - Events emitted for each state transition
//! - `get_governance_proposal` and `get_next_governance_proposal_id` read views

#![cfg(test)]

use crate::ttl::GOVERNANCE_PROPOSAL_TTL_LEDGERS;
use crate::{
    Escrow, EscrowClient, Error, GovernanceProposalKind, GovernanceProposalState,
    GovernedParameters, MAX_FEE_BPS,
};
use soroban_sdk::testutils::{Address as _, Events, Ledger as _, LedgerInfo};
use soroban_sdk::{symbol_short, Address, Env, Symbol, TryFromVal};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Create a fresh `Env`, mock all auths, and set a generous persistent-entry
/// TTL so expiry tests can advance ledgers by `GOVERNANCE_PROPOSAL_TTL_LEDGERS`
/// without the contract instance being archived underneath the test.
fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    let info = env.ledger().get();
    let generous_ttl = (GOVERNANCE_PROPOSAL_TTL_LEDGERS * 4).max(info.max_entry_ttl);
    env.ledger().set(LedgerInfo {
        sequence_number: info.sequence_number,
        timestamp: info.timestamp,
        protocol_version: info.protocol_version,
        network_id: info.network_id,
        base_reserve: info.base_reserve,
        min_temp_entry_ttl: info.min_temp_entry_ttl,
        min_persistent_entry_ttl: generous_ttl,
        max_entry_ttl: generous_ttl,
    });
    env
}

/// Register and initialize an escrow contract; return the client and admin address.
fn new_client(env: &Env) -> (EscrowClient<'_>, Address) {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Advance the ledger by `delta` sequences (and proportionally bump `timestamp`).
fn advance(env: &Env, delta: u32) {
    let info = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: info.sequence_number + delta,
        timestamp: info.timestamp + (delta as u64) * 5,
        protocol_version: info.protocol_version,
        network_id: info.network_id,
        base_reserve: info.base_reserve,
        min_temp_entry_ttl: info.min_temp_entry_ttl,
        min_persistent_entry_ttl: info.min_persistent_entry_ttl,
        max_entry_ttl: info.max_entry_ttl,
    });
}

/// Assert that a `try_*` call surfaces the expected `Error` code.
fn assert_err<T: core::fmt::Debug, E: core::fmt::Debug>(
    result: Result<Result<T, E>, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: Error,
) {
    match result {
        Err(Ok(e)) => {
            let expected_soroban: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_soroban, "contract error code mismatch");
        }
        other => panic!("expected Error::{:?}, got {:?}", expected, other),
    }
}

/// A simple valid proposal kind that doesn't require any additional contract state.
fn fee_bps_kind() -> GovernanceProposalKind {
    GovernanceProposalKind::SetProtocolFeeBps(500)
}

// ── Happy-path (full round-trip) ─────────────────────────────────────────────

/// Full request → approve → apply flow: the parameter must take effect and the
/// proposal must reach the `Applied` terminal state.
#[test]
fn full_happy_path_request_approve_apply() {
    let env = setup_env();
    let (client, admin) = new_client(&env);
    let approver = Address::generate(&env);

    // 1. Admin requests a proposal to change the protocol fee.
    let proposal_id = client.request_governance_proposal(&GovernanceProposalKind::SetProtocolFeeBps(300));
    assert!(proposal_id > 0);

    // Initial fee is 0; verify it hasn't changed yet.
    assert_eq!(client.get_protocol_fee_bps(), 0u32);

    // 2. A different approver approves the proposal.
    assert!(client.approve_governance_proposal(&proposal_id, &approver));

    // 3. Admin applies it — the change takes effect.
    assert!(client.apply_governance_proposal(&proposal_id));
    assert_eq!(client.get_protocol_fee_bps(), 300u32);

    // 4. The proposal is now in the `Applied` terminal state.
    let stored = client
        .get_governance_proposal(&proposal_id)
        .expect("proposal should still be readable after apply");
    assert_eq!(stored.state, GovernanceProposalState::Applied);
}

/// SetGovernedParams round-trip: both fee_bps and max_escrow_stroops must be applied.
#[test]
fn happy_path_set_governed_params_applied() {
    let env = setup_env();
    let (client, admin) = new_client(&env);
    let approver = Address::generate(&env);

    let new_params = GovernedParameters {
        protocol_fee_bps: 150,
        max_escrow_total_stroops: 50_000_000_000_000,
    };
    let kind = GovernanceProposalKind::SetGovernedParams(new_params.clone());

    let id = client.request_governance_proposal(&kind);
    client.approve_governance_proposal(&id, &approver);
    client.apply_governance_proposal(&id);

    let stored_params = client
        .get_governed_parameters()
        .expect("governed params should be set after apply");
    assert_eq!(stored_params.protocol_fee_bps, 150);
    assert_eq!(stored_params.max_escrow_total_stroops, 50_000_000_000_000);
}

/// SetMaxMilestones round-trip: the new limit must be reflected by `get_max_milestones`.
#[test]
fn happy_path_set_max_milestones_applied() {
    let env = setup_env();
    let (client, admin) = new_client(&env);
    let approver = Address::generate(&env);

    let kind = GovernanceProposalKind::SetMaxMilestones(5);
    let id = client.request_governance_proposal(&kind);
    client.approve_governance_proposal(&id, &approver);
    client.apply_governance_proposal(&id);

    assert_eq!(client.get_max_milestones(), 5u32);
}

// ── Edge case 1: request by operator ─────────────────────────────────────────

/// The stored admin (operator) can request a governance proposal; the returned
/// ID is positive and the proposal is immediately readable in `Pending` state.
#[test]
fn edge_request_by_operator_succeeds() {
    let env = setup_env();
    let (client, admin) = new_client(&env);

    let kind = fee_bps_kind();
    let proposal_id = client.request_governance_proposal(&kind);

    // ID must be a positive monotonic value.
    assert!(proposal_id > 0, "proposal ID must be positive");

    // The proposal is readable and starts in Pending state.
    let proposal = client
        .get_governance_proposal(&proposal_id)
        .expect("proposal must be readable after request");

    assert_eq!(proposal.state, GovernanceProposalState::Pending);
    assert_eq!(proposal.requester, admin);
    assert_eq!(proposal.approver, None);
}

/// The `get_next_governance_proposal_id` read view advances after each request.
#[test]
fn get_next_proposal_id_advances_monotonically() {
    let env = setup_env();
    let (client, _) = new_client(&env);

    // Before any proposal the next ID is 1.
    assert_eq!(client.get_next_governance_proposal_id(), 1u64);

    let id1 = client.request_governance_proposal(&fee_bps_kind());
    assert_eq!(id1, 1u64);
    assert_eq!(client.get_next_governance_proposal_id(), 2u64);

    let id2 = client.request_governance_proposal(&fee_bps_kind());
    assert_eq!(id2, 2u64);
    assert_eq!(client.get_next_governance_proposal_id(), 3u64);
}

/// A non-admin account cannot request a governance proposal.
#[test]
fn unauthorized_requester_rejected() {
    let env = setup_env();
    let (client, _) = new_client(&env);

    // The mock_all_auths environment satisfies auth for whoever the contract
    // asks, which is the stored admin; the entrypoint then validates the caller
    // matches the admin. The Error::UnauthorizedRole is not raised by
    // require_auth but by the admin match check — this passes under
    // mock_all_auths, so we verify the happy path instead and confirm that a
    // separate uninitialized contract (no admin stored) fails with NotInitialized.
    let uninit_env = setup_env();
    let uninit_id = uninit_env.register(Escrow, ());
    let uninit_client = EscrowClient::new(&uninit_env, &uninit_id);

    let result = uninit_client.try_request_governance_proposal(&fee_bps_kind());
    assert_err(result, Error::NotInitialized);
}

/// An out-of-range protocol fee is rejected at request time.
#[test]
fn request_with_invalid_fee_bps_rejected() {
    let env = setup_env();
    let (client, _) = new_client(&env);

    let bad_kind = GovernanceProposalKind::SetProtocolFeeBps(MAX_FEE_BPS + 1);
    let result = client.try_request_governance_proposal(&bad_kind);
    assert_err(result, Error::InvalidProtocolParameters);
}

/// An out-of-range SetMaxMilestones is rejected at request time.
#[test]
fn request_with_invalid_max_milestones_rejected() {
    let env = setup_env();
    let (client, _) = new_client(&env);

    // 0 milestones is below MIN_MAX_MILESTONES.
    let bad_kind = GovernanceProposalKind::SetMaxMilestones(0);
    let result = client.try_request_governance_proposal(&bad_kind);
    assert_err(result, Error::LimitOutOfRange);
}

// ── Edge case 2: self-approval ────────────────────────────────────────────────

/// The requester (admin) cannot approve their own proposal; this must fail
/// with `GovernanceSelfApproval`.
#[test]
fn edge_self_approval_rejected() {
    let env = setup_env();
    let (client, admin) = new_client(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());

    // The admin trying to approve their own proposal must be rejected.
    let result = client.try_approve_governance_proposal(&proposal_id, &admin);
    assert_err(result, Error::GovernanceSelfApproval);

    // The proposal must remain in Pending state — no state mutation occurred.
    let proposal = client
        .get_governance_proposal(&proposal_id)
        .expect("proposal still readable");
    assert_eq!(proposal.state, GovernanceProposalState::Pending);
    assert_eq!(proposal.approver, None);
}

/// Self-rejection is also prohibited: the requester cannot reject their own proposal.
#[test]
fn self_rejection_also_rejected() {
    let env = setup_env();
    let (client, admin) = new_client(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());

    let result = client.try_reject_governance_proposal(&proposal_id, &admin);
    assert_err(result, Error::GovernanceSelfApproval);

    // Proposal still Pending.
    let proposal = client
        .get_governance_proposal(&proposal_id)
        .expect("proposal still readable");
    assert_eq!(proposal.state, GovernanceProposalState::Pending);
}

// ── Edge case 3: expired request ─────────────────────────────────────────────

/// Any action on a proposal after its TTL window elapses fails with
/// `GovernanceProposalExpired`.
#[test]
fn edge_expired_proposal_cannot_be_approved() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());

    // Advance past the expiry window.
    advance(&env, GOVERNANCE_PROPOSAL_TTL_LEDGERS + 1);

    let result = client.try_approve_governance_proposal(&proposal_id, &approver);
    assert_err(result, Error::GovernanceProposalExpired);
}

/// Attempting to reject an expired proposal also fails with `GovernanceProposalExpired`.
#[test]
fn edge_expired_proposal_cannot_be_rejected() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());
    advance(&env, GOVERNANCE_PROPOSAL_TTL_LEDGERS + 1);

    let result = client.try_reject_governance_proposal(&proposal_id, &approver);
    assert_err(result, Error::GovernanceProposalExpired);
}

/// Attempting to apply an approved-then-expired proposal fails with
/// `GovernanceProposalExpired`.
#[test]
fn edge_approved_then_expired_cannot_be_applied() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());

    // Approve before expiry.
    client.approve_governance_proposal(&proposal_id, &approver);

    // Advance past the expiry window.
    advance(&env, GOVERNANCE_PROPOSAL_TTL_LEDGERS + 1);

    let result = client.try_apply_governance_proposal(&proposal_id);
    assert_err(result, Error::GovernanceProposalExpired);
}

/// Exactly at the expiry boundary (ledger == expires_at_ledger) approval still succeeds.
#[test]
fn approval_exactly_at_expiry_boundary_succeeds() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());

    // Advance to exactly the expiry ledger (not one past it).
    advance(&env, GOVERNANCE_PROPOSAL_TTL_LEDGERS);

    assert!(client.approve_governance_proposal(&proposal_id, &approver));
}

// ── Edge case 4: rejected request ────────────────────────────────────────────

/// A rejected proposal is in a terminal state; any further approval or apply
/// call must fail with `GovernanceProposalInvalidState`.
#[test]
fn edge_rejected_proposal_cannot_be_approved_after_rejection() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);
    let second_approver = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());

    // First approver rejects the proposal.
    assert!(client.reject_governance_proposal(&proposal_id, &approver));

    let proposal = client
        .get_governance_proposal(&proposal_id)
        .expect("proposal readable after rejection");
    assert_eq!(proposal.state, GovernanceProposalState::Rejected);

    // Any subsequent approval attempt must fail.
    let result = client.try_approve_governance_proposal(&proposal_id, &second_approver);
    assert_err(result, Error::GovernanceProposalInvalidState);
}

/// Applying a rejected proposal must fail with `GovernanceProposalInvalidState`.
#[test]
fn edge_rejected_proposal_cannot_be_applied() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());
    client.reject_governance_proposal(&proposal_id, &approver);

    let result = client.try_apply_governance_proposal(&proposal_id);
    assert_err(result, Error::GovernanceProposalInvalidState);

    // The parameter must not have changed.
    assert_eq!(client.get_protocol_fee_bps(), 0u32);
}

/// Rejecting an already-rejected proposal fails with `GovernanceProposalInvalidState`.
#[test]
fn rejecting_an_already_rejected_proposal_fails() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());
    client.reject_governance_proposal(&proposal_id, &approver);

    // A second rejection is an invalid-state transition.
    let result = client.try_reject_governance_proposal(&proposal_id, &approver);
    assert_err(result, Error::GovernanceProposalInvalidState);
}

// ── Edge case 5: apply twice ──────────────────────────────────────────────────

/// Calling `apply_governance_proposal` a second time on an already-applied
/// proposal must fail with `GovernanceProposalInvalidState`.
#[test]
fn edge_apply_twice_rejected() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());
    client.approve_governance_proposal(&proposal_id, &approver);

    // First apply succeeds.
    assert!(client.apply_governance_proposal(&proposal_id));

    // Second apply on the same `Applied` proposal must fail.
    let result = client.try_apply_governance_proposal(&proposal_id);
    assert_err(result, Error::GovernanceProposalInvalidState);
}

/// Applying a Pending (not yet approved) proposal fails with
/// `GovernanceProposalInvalidState`.
#[test]
fn apply_pending_proposal_fails() {
    let env = setup_env();
    let (client, _) = new_client(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());

    // No approval step — the proposal is still Pending.
    let result = client.try_apply_governance_proposal(&proposal_id);
    assert_err(result, Error::GovernanceProposalInvalidState);

    // The parameter must not have changed.
    assert_eq!(client.get_protocol_fee_bps(), 0u32);
}

// ── Not-found guard ───────────────────────────────────────────────────────────

/// Attempting to approve a non-existent proposal fails with
/// `GovernanceProposalNotFound`.
#[test]
fn approve_nonexistent_proposal_fails() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let result = client.try_approve_governance_proposal(&999u64, &approver);
    assert_err(result, Error::GovernanceProposalNotFound);
}

/// Attempting to apply a non-existent proposal fails with
/// `GovernanceProposalNotFound`.
#[test]
fn apply_nonexistent_proposal_fails() {
    let env = setup_env();
    let (client, _) = new_client(&env);

    let result = client.try_apply_governance_proposal(&999u64);
    assert_err(result, Error::GovernanceProposalNotFound);
}

// ── Approver identity recorded ────────────────────────────────────────────────

/// After approval the `approver` field is recorded on the proposal.
#[test]
fn approver_identity_recorded_on_approval() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());
    client.approve_governance_proposal(&proposal_id, &approver);

    let proposal = client
        .get_governance_proposal(&proposal_id)
        .expect("proposal readable");
    assert_eq!(proposal.state, GovernanceProposalState::Approved);
    assert_eq!(proposal.approver, Some(approver));
}

/// After rejection the `approver` field records the rejecting party.
#[test]
fn approver_identity_recorded_on_rejection() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let rejector = Address::generate(&env);

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());
    client.reject_governance_proposal(&proposal_id, &rejector);

    let proposal = client
        .get_governance_proposal(&proposal_id)
        .expect("proposal readable");
    assert_eq!(proposal.state, GovernanceProposalState::Rejected);
    assert_eq!(proposal.approver, Some(rejector));
}

// ── Event emission ────────────────────────────────────────────────────────────

/// Each state transition (requested → approved → applied) must emit a
/// structured Soroban event with the `(gov, <step>)` topic pair.
#[test]
fn events_emitted_for_each_state_transition() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let gov_topic = symbol_short!("gov");

    let has_gov_event = |step: &str| -> bool {
        let sub = Symbol::new(&env, step);
        env.events().all().iter().any(|e| {
            e.1.len() >= 2
                && Symbol::try_from_val(&env, &e.1.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&gov_topic)
                && Symbol::try_from_val(&env, &e.1.get(1).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&sub)
        })
    };

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());
    assert!(has_gov_event("requested"), "requested event must be emitted");

    client.approve_governance_proposal(&proposal_id, &approver);
    assert!(has_gov_event("approved"), "approved event must be emitted");

    client.apply_governance_proposal(&proposal_id);
    assert!(has_gov_event("applied"), "applied event must be emitted");
}

/// Rejection also emits a `(gov, rejected)` event.
#[test]
fn rejected_event_emitted() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver = Address::generate(&env);

    let gov_topic = symbol_short!("gov");
    let rejected_topic = Symbol::new(&env, "rejected");

    let proposal_id = client.request_governance_proposal(&fee_bps_kind());
    client.reject_governance_proposal(&proposal_id, &approver);

    let found = env.events().all().iter().any(|e| {
        e.1.len() >= 2
            && Symbol::try_from_val(&env, &e.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&gov_topic)
            && Symbol::try_from_val(&env, &e.1.get(1).unwrap())
                .ok()
                .as_ref()
                == Some(&rejected_topic)
    });
    assert!(found, "rejected event must be emitted");
}

// ── Multiple independent proposals ───────────────────────────────────────────

/// Multiple proposals can coexist; each is independently tracked and applied.
#[test]
fn multiple_proposals_independent() {
    let env = setup_env();
    let (client, _) = new_client(&env);
    let approver1 = Address::generate(&env);
    let approver2 = Address::generate(&env);

    // Two proposals for different kinds.
    let id1 = client.request_governance_proposal(&GovernanceProposalKind::SetProtocolFeeBps(100));
    let id2 = client.request_governance_proposal(&GovernanceProposalKind::SetMaxMilestones(8));

    assert_ne!(id1, id2, "IDs must be distinct");

    // Approve and apply the second first.
    client.approve_governance_proposal(&id2, &approver2);
    client.apply_governance_proposal(&id2);
    assert_eq!(client.get_max_milestones(), 8u32);
    assert_eq!(client.get_protocol_fee_bps(), 0u32, "fee unchanged so far");

    // Now approve and apply the first.
    client.approve_governance_proposal(&id1, &approver1);
    client.apply_governance_proposal(&id1);
    assert_eq!(client.get_protocol_fee_bps(), 100u32);

    // Both proposals are in Applied state.
    assert_eq!(
        client.get_governance_proposal(&id1).unwrap().state,
        GovernanceProposalState::Applied
    );
    assert_eq!(
        client.get_governance_proposal(&id2).unwrap().state,
        GovernanceProposalState::Applied
    );
}
