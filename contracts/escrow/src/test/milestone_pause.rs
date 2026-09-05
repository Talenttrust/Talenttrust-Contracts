//! Dedicated pause-guard tests for all milestone entrypoints.
//!
//! Issue #1049: milestone entrypoints must honour the `Paused` / `Emergency`
//! flag.  This module provides exhaustive, milestone-specific coverage:
//!
//! | Section | What is tested |
//! |---------|---------------|
//! | `writes_blocked_*` | Each mutating entrypoint returns `ContractPaused` while paused |
//! | `writes_allowed_*` | Each mutating entrypoint succeeds after unpause |
//! | `reads_always_allowed_*` | Read-only entrypoints succeed even while paused |
//! | `emergency_*` | Emergency mode blocks writes identically to pause |
//! | `guard_ordering_*` | Pause gate fires before auth / state checks |
//! | `state_integrity_*` | No partial state is written during a blocked call |
//!
//! ## Error code note
//!
//! `require_not_paused` panics with `Error::ContractPaused` (code 37 in
//! `types.rs`), **not** `EscrowError::ContractPaused` (code 16 in `lib.rs`).
//! Tests therefore assert against `Error::ContractPaused`.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env, String};

use crate::{Error, Escrow, EscrowClient, ReleaseAuthorization};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Register and initialize a fresh escrow.  Returns `(env, contract_addr, admin)`.
fn setup_initialized() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &addr);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, addr, admin)
}

/// Create a contract in `Created` status (no SAC, no deposit).
/// The pause guard fires before any SAC / funding check, so this is enough for
/// "pause blocks" tests.
fn setup_created_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let c = Address::generate(env);
    let f = Address::generate(env);
    let id = client.create_contract(
        &c,
        &f,
        &None,
        &vec![env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    (c, f, id)
}

/// Register, initialize, bind SAC, mint, create, and fully deposit.
/// Returns `(env, escrow_addr, admin, client_addr, freelancer_addr, contract_id)`.
fn setup_funded() -> (Env, Address, Address, Address, Address, u32) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let escrow_addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let token_addr = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token_addr);
    StellarAssetClient::new(&env, &token_addr).mint(&client_addr, &300_i128);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&id, &client_addr, &300_i128);

    (env, escrow_addr, admin, client_addr, freelancer_addr, id)
}

// ---------------------------------------------------------------------------
// writes_blocked — each mutating entrypoint must return ContractPaused
// ---------------------------------------------------------------------------

#[test]
fn writes_blocked_approve_milestone_release() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (client_addr, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    super::assert_contract_error(
        escrow.try_approve_milestone_release(&id, &client_addr, &0),
        Error::ContractPaused,
    );
}

#[test]
fn writes_blocked_release_milestone() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (client_addr, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    super::assert_contract_error(
        escrow.try_release_milestone(&id, &client_addr, &0, &0),
        Error::ContractPaused,
    );
}

#[test]
fn writes_blocked_refund_unreleased_milestones() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    super::assert_contract_error(
        escrow.try_refund_unreleased_milestones(&id, &vec![&env, 0_u32]),
        Error::ContractPaused,
    );
}

#[test]
fn writes_blocked_submit_work_evidence() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, freelancer_addr, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    let evidence = String::from_str(&env, "ipfs://QmPaused");
    super::assert_contract_error(
        escrow.try_submit_work_evidence(&id, &freelancer_addr, &0, &evidence),
        Error::ContractPaused,
    );
}

// ---------------------------------------------------------------------------
// writes_allowed — each mutating entrypoint succeeds after unpause
// ---------------------------------------------------------------------------

#[test]
fn writes_allowed_approve_milestone_release_after_unpause() {
    // Created-status contract: after unpause, approve call reaches the approval
    // logic; InvalidState (not Funded) is returned — but NOT ContractPaused.
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (client_addr, _, id) = setup_created_contract(&env, &escrow);

    escrow.pause();
    escrow.unpause();

    let result = escrow.try_approve_milestone_release(&id, &client_addr, &0);
    let paused_err: soroban_sdk::Error = Error::ContractPaused.into();
    match result {
        Err(Ok(e)) => assert_ne!(
            e, paused_err,
            "must not return ContractPaused after unpause"
        ),
        Ok(_) => { /* approval succeeded — pause is not blocking */ }
        Err(Err(_)) => { /* unexpected host error, not a pause issue */ }
    }
}

#[test]
fn writes_allowed_release_milestone_after_unpause() {
    let (env, escrow_addr, _, client_addr, _, id) = setup_funded();
    let escrow = EscrowClient::new(&env, &escrow_addr);

    escrow.pause();
    escrow.unpause();

    // Approve first so the release succeeds.
    escrow.approve_milestone_release(&id, &client_addr, &0);
    assert!(escrow.release_milestone(&id, &client_addr, &0, &0));
}

#[test]
fn writes_allowed_refund_unreleased_milestones_after_unpause() {
    let (env, escrow_addr, _, _client_addr, _, id) = setup_funded();
    let escrow = EscrowClient::new(&env, &escrow_addr);

    escrow.pause();
    escrow.unpause();

    let refunded = escrow.refund_unreleased_milestones(&id, &vec![&env, 0_u32, 1_u32]);
    assert!(
        refunded > 0,
        "refund must succeed and return a positive amount after unpause"
    );
}

#[test]
fn writes_allowed_submit_work_evidence_after_unpause() {
    let (env, escrow_addr, _, _, freelancer_addr, id) = setup_funded();
    let escrow = EscrowClient::new(&env, &escrow_addr);

    escrow.pause();
    escrow.unpause();

    let evidence = String::from_str(&env, "ipfs://QmUnpaused");
    assert!(escrow.submit_work_evidence(&id, &freelancer_addr, &0, &evidence));
}

// ---------------------------------------------------------------------------
// reads_always_allowed — read-only milestone endpoints are never gated
// ---------------------------------------------------------------------------

/// `get_milestones` must succeed even while the contract is paused.
#[test]
fn reads_always_allowed_get_milestones_while_paused() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    // Must not panic with ContractPaused.
    let milestones = escrow.get_milestones(&id);
    assert_eq!(
        milestones.len(),
        2,
        "both milestones must be readable while paused"
    );
}

/// `get_milestone` must succeed even while the contract is paused.
#[test]
fn reads_always_allowed_get_milestone_while_paused() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    let m = escrow.get_milestone(&id, &0);
    assert!(m.is_some(), "milestone 0 must be readable while paused");
    assert_eq!(m.unwrap().amount, 100_i128);
}

/// `get_milestone` for an out-of-bounds index returns `None` while paused —
/// no panic, no pause error.
#[test]
fn reads_always_allowed_get_milestone_oob_while_paused() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    let m = escrow.get_milestone(&id, &99);
    assert!(
        m.is_none(),
        "out-of-bounds index must return None, not panic, while paused"
    );
}

/// `is_milestone_overdue` must succeed even while the contract is paused.
#[test]
fn reads_always_allowed_is_milestone_overdue_while_paused() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    // Milestones have no deadline so this will return false — but it must not
    // panic with ContractPaused.
    let overdue = escrow.is_milestone_overdue(&id, &0);
    assert!(!overdue, "milestone with no deadline must not be overdue");
}

/// `get_milestone_approvals` must succeed even while the contract is paused.
#[test]
fn reads_always_allowed_get_milestone_approvals_while_paused() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    // No approvals were recorded, so this returns None — but must not
    // return ContractPaused.
    let approvals = escrow.get_milestone_approvals(&id, &0);
    assert!(
        approvals.is_none(),
        "approval read must succeed (returning None) while paused"
    );
}

/// All read-only milestone endpoints remain accessible during emergency mode.
#[test]
fn reads_always_allowed_during_emergency_mode() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.activate_emergency_pause();

    assert_eq!(escrow.get_milestones(&id).len(), 2);
    assert!(escrow.get_milestone(&id, &0).is_some());
    assert!(!escrow.is_milestone_overdue(&id, &0));
    assert!(escrow.get_milestone_approvals(&id, &0).is_none());
}

// ---------------------------------------------------------------------------
// emergency_mode — EmergencyActive blocks writes identically to pause
// ---------------------------------------------------------------------------

#[test]
fn emergency_blocks_approve_milestone_release() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (client_addr, _, id) = setup_created_contract(&env, &escrow);
    escrow.activate_emergency_pause();

    // Emergency fires the same require_not_paused guard, returning EmergencyActive.
    let result = escrow.try_approve_milestone_release(&id, &client_addr, &0);
    let paused_err: soroban_sdk::Error = Error::ContractPaused.into();
    let emergency_err: soroban_sdk::Error = Error::EmergencyActive.into();
    match result {
        Err(Ok(e)) => assert!(
            e == paused_err || e == emergency_err,
            "must return ContractPaused or EmergencyActive, got {:?}",
            e
        ),
        other => panic!("expected contract error, got {:?}", other),
    }
}

#[test]
fn emergency_blocks_release_milestone() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (client_addr, _, id) = setup_created_contract(&env, &escrow);
    escrow.activate_emergency_pause();

    let paused_err: soroban_sdk::Error = Error::ContractPaused.into();
    let emergency_err: soroban_sdk::Error = Error::EmergencyActive.into();
    match escrow.try_release_milestone(&id, &client_addr, &0, &0) {
        Err(Ok(e)) => assert!(e == paused_err || e == emergency_err),
        other => panic!("expected guard error, got {:?}", other),
    }
}

#[test]
fn emergency_blocks_refund_unreleased_milestones() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.activate_emergency_pause();

    let paused_err: soroban_sdk::Error = Error::ContractPaused.into();
    let emergency_err: soroban_sdk::Error = Error::EmergencyActive.into();
    match escrow.try_refund_unreleased_milestones(&id, &vec![&env, 0_u32]) {
        Err(Ok(e)) => assert!(e == paused_err || e == emergency_err),
        other => panic!("expected guard error, got {:?}", other),
    }
}

#[test]
fn emergency_blocks_submit_work_evidence() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, freelancer_addr, id) = setup_created_contract(&env, &escrow);
    escrow.activate_emergency_pause();

    let evidence = String::from_str(&env, "ipfs://QmEmergency");
    let paused_err: soroban_sdk::Error = Error::ContractPaused.into();
    let emergency_err: soroban_sdk::Error = Error::EmergencyActive.into();
    match escrow.try_submit_work_evidence(&id, &freelancer_addr, &0, &evidence) {
        Err(Ok(e)) => assert!(e == paused_err || e == emergency_err),
        other => panic!("expected guard error, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// guard_ordering — pause check fires before auth and state checks
// ---------------------------------------------------------------------------

/// An outsider address on `approve_milestone_release` receives `ContractPaused`,
/// not an auth error, confirming the guard runs first.
#[test]
fn guard_ordering_approve_milestone_release_before_auth() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    let outsider = Address::generate(&env);
    super::assert_contract_error(
        escrow.try_approve_milestone_release(&id, &outsider, &0),
        Error::ContractPaused,
    );
}

/// A random caller on `release_milestone` receives `ContractPaused`, not an
/// auth / role error, confirming the guard runs first.
#[test]
fn guard_ordering_release_milestone_before_auth() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    let outsider = Address::generate(&env);
    super::assert_contract_error(
        escrow.try_release_milestone(&id, &outsider, &0, &0),
        Error::ContractPaused,
    );
}

/// `submit_work_evidence` with the wrong caller still returns `ContractPaused`
/// while paused, not an auth error.
#[test]
fn guard_ordering_submit_work_evidence_before_auth() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    let outsider = Address::generate(&env);
    let evidence = String::from_str(&env, "ipfs://QmEarly");
    super::assert_contract_error(
        escrow.try_submit_work_evidence(&id, &outsider, &0, &evidence),
        Error::ContractPaused,
    );
}

// ---------------------------------------------------------------------------
// state_integrity — no partial state written during a blocked call
// ---------------------------------------------------------------------------

/// A blocked `approve_milestone_release` must not write any approval record to
/// temporary storage.
#[test]
fn state_integrity_no_approval_written_when_paused() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (client_addr, _, id) = setup_created_contract(&env, &escrow);
    escrow.pause();

    // Blocked call — must not write approvals.
    let _ = escrow.try_approve_milestone_release(&id, &client_addr, &0);

    // Unpause so the approval read is also unblocked.
    escrow.unpause();
    assert!(
        escrow.get_milestone_approvals(&id, &0).is_none(),
        "no stale approval must exist after a pause-blocked approve attempt"
    );
}

/// A blocked `submit_work_evidence` must not write the evidence field.
#[test]
fn state_integrity_no_evidence_written_when_paused() {
    let (env, escrow_addr, _, _, freelancer_addr, id) = setup_funded();
    let escrow = EscrowClient::new(&env, &escrow_addr);
    escrow.pause();

    let evidence = String::from_str(&env, "ipfs://QmShouldNotStore");
    let _ = escrow.try_submit_work_evidence(&id, &freelancer_addr, &0, &evidence);

    escrow.unpause();
    let ms = escrow
        .get_milestone(&id, &0)
        .expect("milestone 0 must exist");
    assert!(
        ms.work_evidence.is_none(),
        "work_evidence must remain None after a pause-blocked submit"
    );
}

/// A blocked `release_milestone` must not advance `released_amount` or flip
/// `milestone.released`.
#[test]
fn state_integrity_no_release_when_paused() {
    let (env, escrow_addr, _, client_addr, _, id) = setup_funded();
    let escrow = EscrowClient::new(&env, &escrow_addr);

    // Record pre-pause state.
    let before = escrow.get_milestone(&id, &0).expect("milestone 0 exists");
    assert!(!before.released);

    escrow.pause();
    let _ = escrow.try_release_milestone(&id, &client_addr, &0, &0);
    escrow.unpause();

    let after = escrow
        .get_milestone(&id, &0)
        .expect("milestone 0 still exists");
    assert!(
        !after.released,
        "milestone.released must remain false after a pause-blocked release"
    );
}

/// A blocked `refund_unreleased_milestones` must not advance `refunded_amount`
/// or flip `milestone.refunded`.
#[test]
fn state_integrity_no_refund_when_paused() {
    let (env, escrow_addr, _, _, _, id) = setup_funded();
    let escrow = EscrowClient::new(&env, &escrow_addr);

    let before = escrow.get_milestone(&id, &0).expect("milestone 0 exists");
    assert!(!before.refunded);

    escrow.pause();
    let _ = escrow.try_refund_unreleased_milestones(&id, &vec![&env, 0_u32]);
    escrow.unpause();

    let after = escrow
        .get_milestone(&id, &0)
        .expect("milestone 0 still exists");
    assert!(
        !after.refunded,
        "milestone.refunded must remain false after a pause-blocked refund"
    );
}

// ---------------------------------------------------------------------------
// multiple_pause_cycles — guard survives repeated pause / unpause rounds
// ---------------------------------------------------------------------------

/// Pause → unpause → pause must block milestone writes on the second pause.
#[test]
fn multiple_pause_cycles_block_writes_on_second_pause() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (client_addr, _, id) = setup_created_contract(&env, &escrow);

    // First cycle.
    escrow.pause();
    escrow.unpause();

    // Second pause — guard must block again.
    escrow.pause();
    super::assert_contract_error(
        escrow.try_approve_milestone_release(&id, &client_addr, &0),
        Error::ContractPaused,
    );
}

/// Reads remain accessible across all pause / unpause cycles.
#[test]
fn multiple_pause_cycles_reads_always_accessible() {
    let (env, addr, _) = setup_initialized();
    let escrow = EscrowClient::new(&env, &addr);
    let (_, _, id) = setup_created_contract(&env, &escrow);

    for _ in 0..3 {
        escrow.pause();
        // Read-only access must succeed in every paused round.
        assert_eq!(escrow.get_milestones(&id).len(), 2);
        assert!(escrow.get_milestone(&id, &0).is_some());
        escrow.unpause();
        // And in every unpaused round.
        assert_eq!(escrow.get_milestones(&id).len(), 2);
        assert!(escrow.get_milestone(&id, &0).is_some());
    }
}
