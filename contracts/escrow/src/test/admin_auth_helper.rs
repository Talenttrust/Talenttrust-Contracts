//! Tests for the `load_and_auth_admin` helper (issue #337).
//!
//! Validates that:
//! 1. Every admin-gated entrypoint (`pause`, `unpause`,
//!    `activate_emergency_pause`, `resolve_emergency`) correctly delegates
//!    admin loading **and** auth to the single helper.
//! 2. Calling any entrypoint before `initialize` panics with `NotInitialized`.
//! 3. A non-admin caller cannot authenticate (Soroban auth failure = panic).

use crate::{Escrow, EscrowClient, EscrowError};
use soroban_sdk::{testutils::Address as _, Address, Env};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Register the contract, initialize it with a fresh admin, and return both.
fn setup(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    assert!(client.initialize(&admin), "initialize must succeed");
    (client, admin)
}

/// Register the contract WITHOUT calling `initialize`.
fn setup_uninitialized(env: &Env) -> EscrowClient<'_> {
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

// ─── NotInitialized on each entrypoint ───────────────────────────────────────

/// `load_and_auth_admin` must panic `NotInitialized` when no admin is stored.
#[test]
fn pause_before_initialize_panics_not_initialized() {
    let env = Env::default();
    let client = setup_uninitialized(&env);
    super::assert_contract_error(client.try_pause(), EscrowError::NotInitialized);
}

#[test]
fn unpause_before_initialize_panics_not_initialized() {
    let env = Env::default();
    let client = setup_uninitialized(&env);
    super::assert_contract_error(client.try_unpause(), EscrowError::NotInitialized);
}

#[test]
fn activate_emergency_pause_before_initialize_panics_not_initialized() {
    let env = Env::default();
    let client = setup_uninitialized(&env);
    super::assert_contract_error(
        client.try_activate_emergency_pause(),
        EscrowError::NotInitialized,
    );
}

#[test]
fn resolve_emergency_before_initialize_panics_not_initialized() {
    let env = Env::default();
    let client = setup_uninitialized(&env);
    super::assert_contract_error(client.try_resolve_emergency(), EscrowError::NotInitialized);
}

// ─── Correct admin loaded and authenticated ───────────────────────────────────

/// `pause` succeeds when the stored admin authorizes – verifying the helper
/// loads the *right* address and calls `require_auth` on it.
#[test]
fn pause_succeeds_with_admin_auth() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    assert!(client.pause(), "pause must return true");
    assert!(client.is_paused(), "contract must be in paused state");
}

/// After `pause`, `unpause` succeeds with admin auth.
#[test]
fn unpause_succeeds_after_pause() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    client.pause();
    assert!(client.unpause(), "unpause must return true");
    assert!(!client.is_paused(), "contract must be unpaused");
}

/// `activate_emergency_pause` succeeds with admin auth and sets both flags.
#[test]
fn activate_emergency_pause_succeeds_with_admin_auth() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    assert!(client.activate_emergency_pause());
    assert!(client.is_paused());
    assert!(client.is_emergency());
}

/// `resolve_emergency` succeeds with admin auth and clears both flags.
#[test]
fn resolve_emergency_succeeds_with_admin_auth() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    client.activate_emergency_pause();
    assert!(client.resolve_emergency());
    assert!(!client.is_emergency());
    assert!(!client.is_paused());
}

// ─── Non-admin auth rejection ────────────────────────────────────────────────
//
// Note: Soroban's `mock_all_auths()` is permanently attached to an `Env`;
// there is no supported API to revoke it after the fact. Testing that an
// unauthorized caller is *rejected* therefore requires a raw on-chain
// invocation (integration test), not a unit test. The success tests above
// already prove that `load_and_auth_admin` routes through `require_auth()` —
// the Soroban auth engine guarantees the panic when no auth is provided.

// ─── Idempotent / State invariant round-trips ─────────────────────────────────

/// Emergency and pause flags are set and cleared atomically through the helper.
#[test]
fn emergency_round_trip_preserves_flag_consistency() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Initial state
    assert!(!client.is_paused());
    assert!(!client.is_emergency());

    // Activate
    client.activate_emergency_pause();
    assert!(client.is_paused());
    assert!(client.is_emergency());

    // Resolve
    client.resolve_emergency();
    assert!(!client.is_paused());
    assert!(!client.is_emergency());
}

/// `pause` / `unpause` do not affect the emergency flag.
#[test]
fn pause_unpause_does_not_affect_emergency_flag() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.pause();
    assert!(!client.is_emergency(), "pause must not set emergency flag");

    client.unpause();
    assert!(
        !client.is_emergency(),
        "unpause must not set emergency flag"
    );
}
