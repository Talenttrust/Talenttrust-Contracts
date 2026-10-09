//! Focused governance validation tests for `contracts/escrow/src/governance.rs`.
//!
//! # Coverage
//!
//! Every test maps to one or more of the following categories:
//!
//! - **Valid** — happy path / successful state transition
//! - **Invalid** — rejection of bad input (wrong auth, wrong state)
//! - **Duplicate / replay** — idempotency and overwrite semantics
//! - **Boundary** — values at or one off the enforced limits
//! - **Event** — emitted events are present with correct topic structure
//!
//! # Setup note
//!
//! `register_uninitialized_client` registers the contract WITHOUT calling
//! `initialize` so that "not initialized" rejection tests can fire.
//! Tests that need a live admin use `register_initialized_client` instead.

#![cfg(test)]

use crate::{Error, EscrowError, GovernedParameters, ADMIN_ROTATION_MIN_DELAY_LEDGERS};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _, LedgerInfo},
    Address, Env, Symbol, TryFromVal,
};
use crate::Escrow;
use crate::EscrowClient;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Register the contract WITHOUT calling `initialize`. Used for "not
/// initialized" rejection tests.
fn register_uninitialized_client(env: &Env) -> EscrowClient<'_> {
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

/// Register the contract and call `initialize` with a fresh admin.
/// Returns `(client, admin_address)`.
fn register_initialized_client(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Advance the test ledger by `delta` ledgers (and proportionally bump timestamp).
fn advance_ledgers(env: &Env, delta: u32) {
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

/// Assert that a `try_*` call surfaces the expected contract error.
fn assert_contract_error<T: core::fmt::Debug, E: Into<soroban_sdk::Error> + core::fmt::Debug>(
    result: Result<
        Result<T, soroban_sdk::ConversionError>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
    expected: E,
) {
    match result {
        Err(Ok(e)) => {
            let expected_err: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_err, "contract error code mismatch");
        }
        other => panic!(
            "expected contract error {:?}, got: {:?}",
            expected, other
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NotInitialized gate — all mutating governance ops must fire before initialize
// ─────────────────────────────────────────────────────────────────────────────

/// VALID: after `initialize`, a proposal can be created.
#[test]
fn propose_succeeds_after_initialize() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let next_admin = Address::generate(&env);
    assert!(client.propose_governance_admin(&next_admin));
}

/// INVALID: `propose_governance_admin` before `initialize` → `EscrowError::NotInitialized`.
#[test]
fn propose_before_initialize_fails_not_initialized() {
    let env = Env::default();
    let client = register_uninitialized_client(&env);
    let proposed = Address::generate(&env);
    assert_contract_error(
        client.try_propose_governance_admin(&proposed),
        EscrowError::NotInitialized,
    );
    // Pending admin must still be None — no partial state mutation.
    assert_eq!(client.get_pending_governance_admin(), None);
}

/// INVALID: `accept_governance_admin` before `initialize` → `EscrowError::NotInitialized`.
#[test]
fn accept_before_initialize_fails_not_initialized() {
    let env = Env::default();
    let client = register_uninitialized_client(&env);
    assert_contract_error(
        client.try_accept_governance_admin(),
        EscrowError::NotInitialized,
    );
}

/// INVALID: `cancel_governance_admin_proposal` before `initialize` → `EscrowError::NotInitialized`.
#[test]
fn cancel_before_initialize_fails_not_initialized() {
    let env = Env::default();
    let client = register_uninitialized_client(&env);
    assert_contract_error(
        client.try_cancel_governance_admin_proposal(),
        EscrowError::NotInitialized,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Self-proposal guard
// ─────────────────────────────────────────────────────────────────────────────

/// INVALID: proposing current admin as the new admin → `Error::CannotProposeSelf`.
///
/// This closes the trivial timelock bypass: `propose(admin) → accept()`.
#[test]
fn propose_self_as_admin_rejected() {
    let env = Env::default();
    let (client, admin) = register_initialized_client(&env);

    assert_contract_error(
        client.try_propose_governance_admin(&admin),
        Error::CannotProposeSelf,
    );

    // No pending proposal should be stored after the rejection.
    assert_eq!(client.get_pending_governance_admin(), None);
}

// ─────────────────────────────────────────────────────────────────────────────
// Happy-path: propose → [timelock] → accept
// ─────────────────────────────────────────────────────────────────────────────

/// VALID: two-step admin transfer succeeds after minimum delay elapses.
#[test]
fn propose_and_accept_happy_path() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let next_admin = Address::generate(&env);

    assert!(client.propose_governance_admin(&next_admin));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(next_admin.clone())
    );

    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(next_admin));
    // Pending entry is cleared after acceptance.
    assert_eq!(client.get_pending_governance_admin(), None);
}

/// BOUNDARY: accept succeeds exactly at the minimum delay boundary.
#[test]
fn accept_succeeds_at_exact_min_delay_boundary() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let next_admin = Address::generate(&env);

    client.propose_governance_admin(&next_admin);
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS); // exactly at boundary

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(next_admin));
}

/// BOUNDARY: accept fails one ledger before the minimum delay.
#[test]
fn accept_fails_one_ledger_before_min_delay() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let next_admin = Address::generate(&env);

    client.propose_governance_admin(&next_admin);
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS - 1); // one short

    assert_contract_error(
        client.try_accept_governance_admin(),
        EscrowError::TimelockNotElapsed,
    );

    // State unchanged: proposal is still pending.
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(next_admin)
    );
}

/// INVALID: accept immediately after proposal → `EscrowError::TimelockNotElapsed`.
#[test]
fn accept_immediately_after_proposal_fails_timelock() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let next_admin = Address::generate(&env);

    client.propose_governance_admin(&next_admin);

    // Zero ledgers elapsed.
    assert_contract_error(
        client.try_accept_governance_admin(),
        EscrowError::TimelockNotElapsed,
    );
}

/// INVALID: accept with no proposal → `Error::InvalidState`.
#[test]
fn accept_with_no_proposal_fails_invalid_state() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);

    assert_contract_error(
        client.try_accept_governance_admin(),
        Error::InvalidState,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Cancel entrypoint
// ─────────────────────────────────────────────────────────────────────────────

/// VALID: cancel clears the pending proposal.
#[test]
fn cancel_proposal_clears_pending_admin() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let proposed = Address::generate(&env);

    assert!(client.propose_governance_admin(&proposed));
    assert_eq!(client.get_pending_governance_admin(), Some(proposed));

    assert!(client.cancel_governance_admin_proposal());
    assert_eq!(client.get_pending_governance_admin(), None);
}

/// INVALID: cancel with no pending proposal → `Error::NoPendingAdminProposal`.
#[test]
fn cancel_with_no_proposal_fails() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);

    assert_contract_error(
        client.try_cancel_governance_admin_proposal(),
        Error::NoPendingAdminProposal,
    );
}

/// INVALID: accept after cancel → `Error::InvalidState` (proposal no longer exists).
#[test]
fn accept_after_cancel_fails_invalid_state() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let proposed = Address::generate(&env);

    client.propose_governance_admin(&proposed);
    client.cancel_governance_admin_proposal();

    assert_contract_error(
        client.try_accept_governance_admin(),
        Error::InvalidState,
    );
}

/// INVALID: double-cancel → second cancel fails with `Error::NoPendingAdminProposal`.
#[test]
fn double_cancel_second_fails() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let proposed = Address::generate(&env);

    client.propose_governance_admin(&proposed);
    assert!(client.cancel_governance_admin_proposal());

    assert_contract_error(
        client.try_cancel_governance_admin_proposal(),
        Error::NoPendingAdminProposal,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Duplicate / overwrite proposal
// ─────────────────────────────────────────────────────────────────────────────

/// DUPLICATE: re-proposing a different candidate overwrites without error.
/// The timelock clock resets to the new proposal ledger.
#[test]
fn re_propose_overwrites_pending_admin() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let first_proposed = Address::generate(&env);
    let second_proposed = Address::generate(&env);

    assert!(client.propose_governance_admin(&first_proposed));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(first_proposed.clone())
    );

    // Re-propose: should overwrite without error
    assert!(client.propose_governance_admin(&second_proposed));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(second_proposed.clone())
    );
}

/// DUPLICATE: overwriting then accepting the second candidate at the boundary.
#[test]
fn overwrite_then_accept_second_candidate() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let first_proposed = Address::generate(&env);
    let second_proposed = Address::generate(&env);

    client.propose_governance_admin(&first_proposed);
    // Overwrite
    client.propose_governance_admin(&second_proposed);
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(second_proposed));
    assert_ne!(client.get_governance_admin(), Some(first_proposed));
}

// ─────────────────────────────────────────────────────────────────────────────
// Composite / sequential flows
// ─────────────────────────────────────────────────────────────────────────────

/// VALID: propose → cancel → propose new candidate → accept.
#[test]
fn propose_cancel_re_propose_accept() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let first_proposed = Address::generate(&env);
    let second_proposed = Address::generate(&env);

    client.propose_governance_admin(&first_proposed);
    client.cancel_governance_admin_proposal();

    assert_eq!(client.get_pending_governance_admin(), None);

    client.propose_governance_admin(&second_proposed);
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(second_proposed));
    assert_eq!(client.get_pending_governance_admin(), None);
}

/// VALID: after admin transfer, old admin can no longer propose (new admin owns the slot).
/// This verifies that after acceptance, DataKey::Admin points to new_admin.
#[test]
fn after_transfer_new_admin_is_stored() {
    let env = Env::default();
    let (client, old_admin) = register_initialized_client(&env);
    let new_admin = Address::generate(&env);

    client.propose_governance_admin(&new_admin);
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);
    client.accept_governance_admin();

    // get_governance_admin must reflect the new admin
    assert_eq!(client.get_governance_admin(), Some(new_admin.clone()));
    // Old admin is no longer the stored admin
    assert_ne!(client.get_governance_admin(), Some(old_admin));
}

// ─────────────────────────────────────────────────────────────────────────────
// Event emission
// ─────────────────────────────────────────────────────────────────────────────

/// EVENT: `propose_governance_admin` emits `(admin, "proposed")`.
#[test]
fn propose_emits_event() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let proposed = Address::generate(&env);

    client.propose_governance_admin(&proposed);

    let events = env.events().all();
    let admin_topic = soroban_sdk::symbol_short!("admin");
    let proposed_topic = Symbol::new(&env, "proposed");
    let found = events.iter().any(|event| {
        event.1.len() >= 2
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&admin_topic)
            && Symbol::try_from_val(&env, &event.1.get(1).unwrap())
                .ok()
                .as_ref()
                == Some(&proposed_topic)
    });
    assert!(found, "propose event should be emitted with correct topics");
}

/// EVENT: `accept_governance_admin` emits `(admin, "accepted")`.
#[test]
fn accept_emits_event() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let next_admin = Address::generate(&env);

    client.propose_governance_admin(&next_admin);
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);
    client.accept_governance_admin();

    let events = env.events().all();
    let admin_topic = soroban_sdk::symbol_short!("admin");
    let accepted_topic = Symbol::new(&env, "accepted");
    let found = events.iter().any(|event| {
        event.1.len() >= 2
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&admin_topic)
            && Symbol::try_from_val(&env, &event.1.get(1).unwrap())
                .ok()
                .as_ref()
                == Some(&accepted_topic)
    });
    assert!(found, "accept event should be emitted with correct topics");
}

/// EVENT: `cancel_governance_admin_proposal` emits `(admin, "cancelled")`.
#[test]
fn cancel_emits_event() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let proposed = Address::generate(&env);

    client.propose_governance_admin(&proposed);
    client.cancel_governance_admin_proposal();

    let events = env.events().all();
    let admin_topic = soroban_sdk::symbol_short!("admin");
    let cancelled_topic = Symbol::new(&env, "cancelled");
    let found = events.iter().any(|event| {
        event.1.len() >= 2
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&admin_topic)
            && Symbol::try_from_val(&env, &event.1.get(1).unwrap())
                .ok()
                .as_ref()
                == Some(&cancelled_topic)
    });
    assert!(found, "cancel event should be emitted with correct topics");
}

// ─────────────────────────────────────────────────────────────────────────────
// set_governed_params — parameter boundary tests
// ─────────────────────────────────────────────────────────────────────────────

/// VALID: `set_governed_params` accepts fee_bps = 0 (no protocol fee).
#[test]
fn set_governed_params_fee_zero_accepted() {
    let env = Env::default();
    let (client, admin) = register_initialized_client(&env);

    assert!(client.set_governed_params(&admin, &0u32, &1_000_000_i128));
    let params = client.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 0);
}

/// VALID: `set_governed_params` accepts fee_bps = 10_000 (100% — maximum allowed).
#[test]
fn set_governed_params_fee_max_boundary_accepted() {
    let env = Env::default();
    let (client, admin) = register_initialized_client(&env);

    assert!(client.set_governed_params(&admin, &10_000u32, &1_000_000_i128));
    let params = client.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 10_000);
}

/// BOUNDARY: `set_governed_params` rejects fee_bps = 10_001 (one over 100%).
#[test]
fn set_governed_params_fee_above_max_rejected() {
    let env = Env::default();
    let (client, admin) = register_initialized_client(&env);

    assert_contract_error(
        client.try_set_governed_params(&admin, &10_001u32, &1_000_000_i128),
        Error::InvalidProtocolParameters,
    );
}

/// BOUNDARY: `set_governed_params` rejects max_escrow_total_stroops = 0.
#[test]
fn set_governed_params_zero_max_escrow_rejected() {
    let env = Env::default();
    let (client, admin) = register_initialized_client(&env);

    assert_contract_error(
        client.try_set_governed_params(&admin, &500u32, &0_i128),
        Error::InvalidProtocolParameters,
    );
}

/// BOUNDARY: `set_governed_params` rejects max_escrow_total_stroops = -1 (negative).
#[test]
fn set_governed_params_negative_max_escrow_rejected() {
    let env = Env::default();
    let (client, admin) = register_initialized_client(&env);

    assert_contract_error(
        client.try_set_governed_params(&admin, &500u32, &(-1_i128)),
        Error::InvalidProtocolParameters,
    );
}

/// VALID: `set_governed_params` accepts max_escrow_total_stroops = 1 (minimum positive).
#[test]
fn set_governed_params_min_positive_max_escrow_accepted() {
    let env = Env::default();
    let (client, admin) = register_initialized_client(&env);

    assert!(client.set_governed_params(&admin, &100u32, &1_i128));
    let params = client.get_governed_parameters().unwrap();
    assert_eq!(params.max_escrow_total_stroops, 1);
}

/// INVALID: `set_governed_params` called before `initialize` → `EscrowError::NotInitialized`.
#[test]
fn set_governed_params_before_initialize_fails() {
    let env = Env::default();
    let client = register_uninitialized_client(&env);
    let admin = Address::generate(&env);

    assert_contract_error(
        client.try_set_governed_params(&admin, &500u32, &1_000_000_i128),
        EscrowError::NotInitialized,
    );
}

/// INVALID: `set_governed_params` with wrong admin → `EscrowError::UnauthorizedRole`.
#[test]
fn set_governed_params_wrong_admin_rejected() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);
    let wrong_admin = Address::generate(&env);

    assert_contract_error(
        client.try_set_governed_params(&wrong_admin, &500u32, &1_000_000_i128),
        EscrowError::UnauthorizedRole,
    );
}

/// VALID: successive `set_governed_params` calls update the stored parameters.
#[test]
fn set_governed_params_update_overwrites() {
    let env = Env::default();
    let (client, admin) = register_initialized_client(&env);

    client.set_governed_params(&admin, &100u32, &5_000_000_i128);
    client.set_governed_params(&admin, &200u32, &10_000_000_i128);

    let params = client.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 200);
    assert_eq!(params.max_escrow_total_stroops, 10_000_000);
}

/// VALID: `set_governed_params` sets `governed_params_set` flag in readiness checklist.
#[test]
fn set_governed_params_updates_readiness_checklist() {
    let env = Env::default();
    let (client, admin) = register_initialized_client(&env);

    let before = client.get_mainnet_readiness_info();
    assert!(!before.governed_params_set, "flag must start false");

    client.set_governed_params(&admin, &100u32, &1_000_000_i128);

    let after = client.get_mainnet_readiness_info();
    assert!(after.governed_params_set, "flag must be set after call");
}

// ─────────────────────────────────────────────────────────────────────────────
// set_protocol_fee_bps — dedicated tests
// ─────────────────────────────────────────────────────────────────────────────

/// VALID: `set_protocol_fee_bps` succeeds with a valid fee.
#[test]
fn set_protocol_fee_bps_valid() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);

    assert!(client.set_protocol_fee_bps(&250u32));
    assert_eq!(client.get_protocol_fee_bps_view(), 250);
}

/// VALID: `set_protocol_fee_bps` to 0 clears the fee.
#[test]
fn set_protocol_fee_bps_zero_accepted() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);

    client.set_protocol_fee_bps(&100u32);
    assert!(client.set_protocol_fee_bps(&0u32));
    assert_eq!(client.get_protocol_fee_bps_view(), 0);
}

/// EVENT: `set_protocol_fee_bps` emits a `protocol_fee_bps` event.
#[test]
fn set_protocol_fee_bps_emits_event() {
    let env = Env::default();
    let (client, _admin) = register_initialized_client(&env);

    client.set_protocol_fee_bps(&100u32);

    let events = env.events().all();
    let fee_topic = Symbol::new(&env, "protocol_fee_bps");
    let found = events.iter().any(|event| {
        event.1.len() > 0
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&fee_topic)
    });
    assert!(found, "protocol_fee_bps event must be emitted");
}
