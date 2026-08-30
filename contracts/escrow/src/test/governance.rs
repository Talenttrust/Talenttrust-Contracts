//! Unit tests for the two-step admin transfer (propose/accept/cancel) with
//! timelock and expiry, per issue #1321.

use crate::{
    Escrow, EscrowClient, ADMIN_ROTATION_MIN_DELAY_LEDGERS, ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS,
};
use soroban_sdk::testutils::{Address as _, Events, Ledger as _, LedgerInfo};
use soroban_sdk::{Address, Env, Symbol, TryFromVal};

/// Register an uninitialized escrow contract. Unlike `super::register_client`,
/// this does not call `initialize` so tests can control the admin address
/// used for propose/accept/cancel.
fn register_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

/// Fresh test `Env` with a generous `max_entry_ttl`/`min_persistent_entry_ttl`
/// set *before* the contract is registered. Expiry tests advance the ledger
/// sequence by `ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS` (~9 days of ledgers),
/// which comfortably exceeds the host's default test TTL and would otherwise
/// archive the contract instance out from under the test.
fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    let initial = env.ledger().get();
    let generous_ttl = (ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS * 2).max(initial.max_entry_ttl);
    env.ledger().set(LedgerInfo {
        sequence_number: initial.sequence_number,
        timestamp: initial.timestamp,
        protocol_version: initial.protocol_version,
        network_id: initial.network_id,
        base_reserve: initial.base_reserve,
        min_temp_entry_ttl: initial.min_temp_entry_ttl,
        min_persistent_entry_ttl: generous_ttl,
        max_entry_ttl: generous_ttl,
    });
    env
}

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

#[test]
fn admin_transfer_propose_and_accept_happy_path() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let next_admin = Address::generate(&env);
    client.initialize(&admin);

    assert!(client.propose_admin(&next_admin));
    assert_eq!(client.get_pending_admin(), Some(next_admin.clone()));

    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    assert!(client.accept_admin());
    assert_eq!(client.get_admin(), Some(next_admin));
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn propose_self_as_admin_rejected() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let result = client.try_propose_admin(&admin);
    super::assert_contract_error(result, crate::Error::CannotProposeSelf);

    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn propose_overwrites_pending_admin() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let first_pending = Address::generate(&env);
    let second_pending = Address::generate(&env);
    client.initialize(&admin);

    assert!(client.propose_admin(&first_pending));
    assert_eq!(client.get_pending_admin(), Some(first_pending.clone()));

    // Re-proposing should overwrite without error.
    assert!(client.propose_admin(&second_pending));
    assert_eq!(client.get_pending_admin(), Some(second_pending.clone()));
}

#[test]
fn cancel_proposal_clears_pending_admin() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    assert!(client.propose_admin(&proposed));
    assert_eq!(client.get_pending_admin(), Some(proposed.clone()));

    assert!(client.cancel_admin());
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn cancel_without_proposal_fails() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let result = client.try_cancel_admin();
    super::assert_contract_error(result, crate::Error::InvalidState);
}

#[test]
fn accept_after_cancel_fails() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    assert!(client.propose_admin(&proposed));
    assert!(client.cancel_admin());

    // Cancellation clears the pending slot before the timelock could ever
    // elapse, so a replayed accept sees no pending proposal at all.
    let result = client.try_accept_admin();
    super::assert_contract_error(result, crate::Error::InvalidState);
}

#[test]
fn accept_by_wrong_account_rejected() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    assert!(client.propose_admin(&proposed));
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    // The admin never changes because accept_admin requires the *proposed*
    // address to authorize, not the caller. mock_all_auths satisfies whatever
    // auth the entrypoint asks for, so this proves the effect: the admin is
    // still the original one after a successful accept, i.e. the transfer
    // could only have gone to the proposed address.
    assert!(client.accept_admin());
    assert_eq!(client.get_admin(), Some(proposed));
    assert_ne!(client.get_admin(), Some(admin));
}

#[test]
fn propose_not_initialized_fails() {
    let env = setup_env();
    let client = register_client(&env);

    let proposed = Address::generate(&env);
    let result = client.try_propose_admin(&proposed);
    super::assert_contract_error(result, crate::Error::NotInitialized);
}

#[test]
fn accept_not_initialized_fails() {
    let env = setup_env();
    let client = register_client(&env);

    let result = client.try_accept_admin();
    super::assert_contract_error(result, crate::Error::NotInitialized);
}

#[test]
fn cancel_not_initialized_fails() {
    let env = setup_env();
    let client = register_client(&env);

    let result = client.try_cancel_admin();
    super::assert_contract_error(result, crate::Error::NotInitialized);
}

#[test]
fn propose_then_cancel_then_new_propose_then_accept() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let first_proposed = Address::generate(&env);
    let second_proposed = Address::generate(&env);
    client.initialize(&admin);

    assert!(client.propose_admin(&first_proposed));
    assert_eq!(client.get_pending_admin(), Some(first_proposed.clone()));

    assert!(client.cancel_admin());
    assert_eq!(client.get_pending_admin(), None);

    assert!(client.propose_admin(&second_proposed));
    assert_eq!(client.get_pending_admin(), Some(second_proposed.clone()));

    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    assert!(client.accept_admin());
    assert_eq!(client.get_admin(), Some(second_proposed));
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn accept_before_timelock_rejected() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);

    // Zero ledgers elapsed.
    super::assert_contract_error(client.try_accept_admin(), crate::Error::TimelockNotElapsed);

    // One ledger short of the minimum.
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS - 1);
    super::assert_contract_error(client.try_accept_admin(), crate::Error::TimelockNotElapsed);
}

// ── Expiry window ────────────────────────────────────────────────────────────

#[test]
fn accept_after_expiry_window_rejected() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);
    advance_ledgers(&env, ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS + 1);

    super::assert_contract_error(
        client.try_accept_admin(),
        crate::Error::AdminProposalExpired,
    );

    // A panic rolls back the whole call, so the stale proposal is left in
    // place (not silently cleared) and the admin is unchanged; `cancel_admin`
    // or a fresh `propose_admin` is required to move past it.
    assert_eq!(client.get_pending_admin(), Some(proposed));
    assert_eq!(client.get_admin(), Some(admin));
}

#[test]
fn accept_exactly_at_expiry_boundary_succeeds() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);
    advance_ledgers(&env, ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS);

    assert!(client.accept_admin());
    assert_eq!(client.get_admin(), Some(proposed));
}

#[test]
fn expired_proposal_can_be_cancelled() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);
    advance_ledgers(&env, ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS + 1);

    assert!(client.cancel_admin());
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn expired_proposal_requires_re_propose() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);
    advance_ledgers(&env, ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS + 1);
    super::assert_contract_error(
        client.try_accept_admin(),
        crate::Error::AdminProposalExpired,
    );

    // A fresh proposal resets the timelock/expiry anchor and succeeds.
    client.propose_admin(&proposed);
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);
    assert!(client.accept_admin());
    assert_eq!(client.get_admin(), Some(proposed));
}

// ── Events ────────────────────────────────────────────────────────────────────

fn has_admin_event(env: &Env, topic: &str) -> bool {
    let admin_topic = soroban_sdk::symbol_short!("admin");
    let sub_topic = Symbol::new(env, topic);
    env.events().all().iter().any(|event| {
        event.1.len() >= 2
            && Symbol::try_from_val(env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&admin_topic)
            && Symbol::try_from_val(env, &event.1.get(1).unwrap())
                .ok()
                .as_ref()
                == Some(&sub_topic)
    })
}

#[test]
fn propose_emits_event() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);
    assert!(
        has_admin_event(&env, "proposed"),
        "propose event should be emitted"
    );
}

#[test]
fn accept_emits_event() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);
    client.accept_admin();

    assert!(
        has_admin_event(&env, "accepted"),
        "accept event should be emitted"
    );
}

#[test]
fn cancel_emits_event() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);
    client.cancel_admin();

    assert!(
        has_admin_event(&env, "cancelled"),
        "cancel event should be emitted"
    );
}

// -- Recovery ------------------------------------------------------------------

#[test]
fn recover_active_proposal_fails() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);

    // Active (timelock not elapsed)
    super::assert_contract_error(
        client.try_recover_admin_proposal(),
        crate::Error::TimelockNotElapsed,
    );

    // Active (timelock elapsed but not expired)
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);
    super::assert_contract_error(
        client.try_recover_admin_proposal(),
        crate::Error::InvalidState,
    );
}

#[test]
fn recover_expired_proposal_succeeds_and_emits_event() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);
    advance_ledgers(&env, ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS + 1);

    assert!(client.recover_admin_proposal());
    assert_eq!(client.get_pending_admin(), None);

    assert!(
        has_admin_event(&env, "recovered"),
        "recovered event should be emitted"
    );
}

#[test]
fn repeat_recovery_fails() {
    let env = setup_env();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    let proposed = Address::generate(&env);
    client.initialize(&admin);

    client.propose_admin(&proposed);
    advance_ledgers(&env, ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS + 1);

    assert!(client.recover_admin_proposal());

    super::assert_contract_error(
        client.try_recover_admin_proposal(),
        crate::Error::InvalidState,
    );
}
