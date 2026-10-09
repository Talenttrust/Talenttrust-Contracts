//! Tests for governance admin transfer and cancel operations.
//!
//! Uses local helpers to avoid the double-initialize issue that would occur
//! if `super::register_client` (which calls `initialize`) were combined with
//! a second `initialize` call in the test body.

#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events, Ledger as _, LedgerInfo};
use soroban_sdk::{Address, Env, Symbol, TryFromVal};

use crate::{Error, EscrowError, Escrow, EscrowClient, ADMIN_ROTATION_MIN_DELAY_LEDGERS};

// ── Local helpers ─────────────────────────────────────────────────────────────

/// Register the contract WITHOUT calling `initialize`.
/// Used for "not initialized" rejection tests.
fn register_uninitialized(env: &Env) -> EscrowClient<'_> {
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

/// Register the contract and call `initialize` with a fresh admin.
/// Returns `(client, admin_address)`.
fn register_initialized(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Advance the test ledger by `delta` ledgers.
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

// ── Happy path ────────────────────────────────────────────────────────────────

/// Propose a next admin, advance past timelock, accept.
#[test]
fn admin_transfer_propose_and_accept_happy_path() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);
    let next_admin = Address::generate(&env);

    assert!(client.propose_governance_admin(&next_admin));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(next_admin.clone())
    );

    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(next_admin));
    assert_eq!(client.get_pending_governance_admin(), None);
}

// ── Self-proposal guard ───────────────────────────────────────────────────────

/// Proposing the current admin's own address as next admin must fail.
#[test]
fn propose_self_as_admin_rejected() {
    let env = Env::default();
    let (client, admin) = register_initialized(&env);

    let result = client.try_propose_governance_admin(&admin);
    super::assert_contract_error(result, Error::CannotProposeSelf);

    // Pending admin should remain None — no partial state mutation.
    assert_eq!(client.get_pending_governance_admin(), None);
}

// ── Re-propose (overwrite) ────────────────────────────────────────────────────

/// Re-proposing a different candidate silently overwrites the first.
#[test]
fn propose_overwrites_pending_admin() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);
    let first_pending = Address::generate(&env);
    let second_pending = Address::generate(&env);

    assert!(client.propose_governance_admin(&first_pending));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(first_pending.clone())
    );

    // Re-proposing should overwrite without error.
    assert!(client.propose_governance_admin(&second_pending));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(second_pending.clone())
    );
}

// ── Cancel ────────────────────────────────────────────────────────────────────

/// Cancelling a live proposal clears the pending entry.
#[test]
fn cancel_proposal_clears_pending_admin() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);
    let proposed = Address::generate(&env);

    assert!(client.propose_governance_admin(&proposed));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(proposed.clone())
    );

    assert!(client.cancel_governance_admin_proposal());
    assert_eq!(client.get_pending_governance_admin(), None);
}

/// Cancelling when no proposal is pending returns `Error::NoPendingAdminProposal`.
#[test]
fn cancel_without_proposal_fails() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);

    let result = client.try_cancel_governance_admin_proposal();
    super::assert_contract_error(result, Error::NoPendingAdminProposal);
}

/// Trying to accept after a cancel returns `Error::InvalidState`.
#[test]
fn accept_after_cancel_fails() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);
    let proposed = Address::generate(&env);

    assert!(client.propose_governance_admin(&proposed));
    assert!(client.cancel_governance_admin_proposal());

    let result = client.try_accept_governance_admin();
    super::assert_contract_error(result, Error::InvalidState);
}

// ── Not-initialized gate ──────────────────────────────────────────────────────

/// `propose_governance_admin` before `initialize` → `EscrowError::NotInitialized`.
#[test]
fn propose_not_initialized_fails() {
    let env = Env::default();
    let client = register_uninitialized(&env);

    let proposed = Address::generate(&env);
    let result = client.try_propose_governance_admin(&proposed);
    super::assert_contract_error(result, EscrowError::NotInitialized);
}

/// `accept_governance_admin` before `initialize` → `EscrowError::NotInitialized`.
#[test]
fn accept_not_initialized_fails() {
    let env = Env::default();
    let client = register_uninitialized(&env);

    let result = client.try_accept_governance_admin();
    super::assert_contract_error(result, EscrowError::NotInitialized);
}

/// `cancel_governance_admin_proposal` before `initialize` → `EscrowError::NotInitialized`.
#[test]
fn cancel_not_initialized_fails() {
    let env = Env::default();
    let client = register_uninitialized(&env);

    let result = client.try_cancel_governance_admin_proposal();
    super::assert_contract_error(result, EscrowError::NotInitialized);
}

// ── Composite flow ────────────────────────────────────────────────────────────

/// Full lifecycle: propose → cancel → propose different → accept.
#[test]
fn propose_then_cancel_then_new_propose_then_accept() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);
    let first_proposed = Address::generate(&env);
    let second_proposed = Address::generate(&env);

    // Propose first candidate
    assert!(client.propose_governance_admin(&first_proposed));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(first_proposed.clone())
    );

    // Cancel
    assert!(client.cancel_governance_admin_proposal());
    assert_eq!(client.get_pending_governance_admin(), None);

    // Propose second candidate
    assert!(client.propose_governance_admin(&second_proposed));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(second_proposed.clone())
    );

    // Accept moves second candidate to admin
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);
    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(second_proposed));
    assert_eq!(client.get_pending_governance_admin(), None);
}

// ── Events ────────────────────────────────────────────────────────────────────

/// `cancel_governance_admin_proposal` emits `(admin, "cancelled")` topics.
#[test]
fn cancel_emits_event() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);
    let proposed = Address::generate(&env);

    client.propose_governance_admin(&proposed);
    client.cancel_governance_admin_proposal();

    let events = env.events().all();
    let admin_topic = soroban_sdk::symbol_short!("admin");
    let cancelled_topic = soroban_sdk::Symbol::new(&env, "cancelled");
    let found_cancelled = events.iter().any(|event| {
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
    assert!(found_cancelled, "cancel event should be emitted");
}

/// `propose_governance_admin` emits `(admin, "proposed")` topics.
#[test]
fn propose_emits_event() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);
    let proposed = Address::generate(&env);

    client.propose_governance_admin(&proposed);

    let events = env.events().all();
    let admin_topic = soroban_sdk::symbol_short!("admin");
    let proposed_topic = soroban_sdk::Symbol::new(&env, "proposed");
    let found_proposed = events.iter().any(|event| {
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
    assert!(found_proposed, "propose event should be emitted");
}
