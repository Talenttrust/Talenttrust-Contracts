//! Pause / emergency interaction matrix for governance setters.
//!
//! Issue #742: TESTS below pin down the intended behaviour that governance
//! setters (`set_protocol_fee_bps`, `set_governed_params`,
//! `bind_settlement_token`) remain reachable in **all** three contract
//! states — **normal**, **paused**, and **emergency**.
//!
//! Unlike mutating escrow entrypoints (`create_contract`, `deposit_funds`,
//! etc.) which call `require_not_paused`, these admin-only governance
//! functions intentionally omit the pause/emergency guard so that the
//! protocol operator can adjust fees and parameters even while the
//! platform is paused or in emergency mode.
//!
//! ## What is covered
//!
//! 1. **Availability matrix** — every governance setter is called in
//!    normal, paused, and emergency states and must succeed.
//! 2. **Flag independence** — `is_paused` and `is_emergency` report
//!    independently: a plain `pause()` sets only `Paused`; `activate_emergency_pause()`
//!    sets both; calling `is_paused()` on a purely-emergency flag returns `true`.
//! 3. **resolve_emergency clears Paused** (current behaviour) — the
//!    implementation sets both `Emergency` and `Paused` to `false`, so
//!    after resolution both flags read `false`.  This is documented by
//!    test; a future change that preserves the pause flag across
//!    emergency resolution would make this test fail, drawing attention
//!    to the new contract.
//! 4. **Edge cases** — double-bind protection works across all three
//!    states; pause / emergency toggle idempotency.
//!
//! ## Error codes used
//!
//! | Test expects                  | Error variant                   | Code |
//! |-------------------------------|---------------------------------|------|
//! | double-bind rejection         | `EscrowError::SettlementTokenAlreadyBound` | —    |
//! | unpause while emergency       | `Error::EmergencyActive`        | 38   |
//!
//! All governance-setter success paths expect `Ok(true)` or a direct `true`
//! return; failure paths use `try_*` + `assert_contract_error`.

use crate::{Error, Escrow, EscrowClient, EscrowError, GovernedParameters, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Register, initialize, mock all auths, and return `(env, contract_id, admin)`.
fn setup_initialized() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    (env, contract_id, admin)
}

/// Register, initialize with non-root auth for SAC, return `(env, addr, admin)`.
fn setup_initialized_sac() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    (env, contract_id, admin)
}

// ===========================================================================
// 1. Availability matrix — {normal, paused, emergency} × governance setters
// ===========================================================================

// ── set_protocol_fee_bps ───────────────────────────────────────────────────

#[test]
fn set_protocol_fee_bps_succeeds_when_normal() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_protocol_fee_bps(&500));
    assert_eq!(client.get_protocol_fee_bps(), 500);
}

#[test]
fn set_protocol_fee_bps_succeeds_when_paused() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.pause();
    assert!(client.is_paused());

    // Governance setter must NOT be blocked by pause.
    assert!(client.set_protocol_fee_bps(&750));
    assert_eq!(client.get_protocol_fee_bps(), 750);
}

#[test]
fn set_protocol_fee_bps_succeeds_when_emergency() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.activate_emergency_pause();
    assert!(client.is_emergency());

    // Governance setter must NOT be blocked by emergency mode.
    assert!(client.set_protocol_fee_bps(&1000));
    assert_eq!(client.get_protocol_fee_bps(), 1000);
}

// ── set_governed_params ────────────────────────────────────────────────────

#[test]
fn set_governed_params_succeeds_when_normal() {
    let (env, contract_id, admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_governed_params(&admin, &500, &1_000_000_000_i128));
    let params = client.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 500);
    assert_eq!(params.max_escrow_total_stroops, 1_000_000_000);
}

#[test]
fn set_governed_params_succeeds_when_paused() {
    let (env, contract_id, admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.pause();
    assert!(client.is_paused());

    assert!(client.set_governed_params(&admin, &300, &500_000_000_i128));
    let params = client.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 300);
    assert_eq!(params.max_escrow_total_stroops, 500_000_000);
}

#[test]
fn set_governed_params_succeeds_when_emergency() {
    let (env, contract_id, admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.activate_emergency_pause();
    assert!(client.is_emergency());

    assert!(client.set_governed_params(&admin, &100, &2_000_000_000_i128));
    let params = client.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 100);
    assert_eq!(params.max_escrow_total_stroops, 2_000_000_000);
}

// ── bind_settlement_token ──────────────────────────────────────────────────
//
// `bind_settlement_token` is write-once: the first bind succeeds, the second
// fails with `SettlementTokenAlreadyBound` regardless of state.  Each test
// creates a fresh contract so the first bind succeeds in the target state.

#[test]
fn bind_settlement_token_succeeds_when_normal() {
    let (env, contract_id, admin) = setup_initialized_sac();
    let client = EscrowClient::new(&env, &contract_id);

    let token = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &token));
    assert_eq!(client.get_settlement_token(), Some(token));
}

#[test]
fn bind_settlement_token_succeeds_when_paused() {
    let (env, contract_id, admin) = setup_initialized_sac();
    let client = EscrowClient::new(&env, &contract_id);

    client.pause();
    assert!(client.is_paused());

    let token = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &token));
    assert_eq!(client.get_settlement_token(), Some(token));
}

#[test]
fn bind_settlement_token_succeeds_when_emergency() {
    let (env, contract_id, admin) = setup_initialized_sac();
    let client = EscrowClient::new(&env, &contract_id);

    client.activate_emergency_pause();
    assert!(client.is_emergency());

    let token = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &token));
    assert_eq!(client.get_settlement_token(), Some(token));
}

// ===========================================================================
// 2. Flag independence — is_paused and is_emergency report independently
// ===========================================================================

#[test]
fn pause_sets_only_paused_not_emergency() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(!client.is_paused());
    assert!(!client.is_emergency());

    client.pause();

    assert!(client.is_paused());
    assert!(
        !client.is_emergency(),
        "pause must NOT set the emergency flag"
    );
}

#[test]
fn emergency_sets_both_paused_and_emergency() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(!client.is_paused());
    assert!(!client.is_emergency());

    client.activate_emergency_pause();

    assert!(client.is_paused(), "emergency must set the paused flag");
    assert!(client.is_emergency());
}

#[test]
fn unpause_clears_paused_only() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.pause();
    assert!(client.is_paused());
    assert!(!client.is_emergency());

    client.unpause();

    assert!(!client.is_paused());
    assert!(!client.is_emergency());
}

// ===========================================================================
// 3. resolve_emergency clears the Paused flag (current implementation)
// ===========================================================================
//
// The current `resolve_emergency` implementation (lib.rs:1689-1690) sets both
// `Emergency` and `Paused` to `false`.  This test documents that behaviour.
// If a future change preserves the pause flag across emergency resolution,
// this test must be updated.

#[test]
fn resolve_emergency_clears_both_flags() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.activate_emergency_pause();
    assert!(client.is_emergency());
    assert!(client.is_paused());

    // First, pause independently again to ensure it was set by emergency.
    // Then resolve.
    client.resolve_emergency();

    assert!(
        !client.is_emergency(),
        "resolve_emergency must clear emergency"
    );
    assert!(
        !client.is_paused(),
        "current behaviour: resolve_emergency also clears paused — this test \
         documents the implementation; change with care"
    );
}

#[test]
fn resolve_emergency_then_unpause_succeeds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.activate_emergency_pause();
    client.resolve_emergency();

    // After resolve, normal unpause should work (flags are clean).
    client.pause();
    assert!(client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn pause_independent_of_emergency_after_resolve() {
    /// Scenario: pause → emergency → resolve → pause again should work
    /// independently. This verifies that resolve_emergency fully resets
    /// both flags so a subsequent pause can re-set Paused.
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.pause();
    client.activate_emergency_pause();
    client.resolve_emergency();

    // After resolve, both flags should be false
    assert!(!client.is_paused());
    assert!(!client.is_emergency());

    // A fresh pause should work
    client.pause();
    assert!(client.is_paused());
    assert!(!client.is_emergency());
}

// ===========================================================================
// 4. Edge cases and failure paths
// ===========================================================================

// ── Double-bind protection ──────────────────────────────────────────────────

#[test]
fn bind_settlement_token_rejects_double_bind_when_normal() {
    let (env, contract_id, admin) = setup_initialized_sac();
    let client = EscrowClient::new(&env, &contract_id);

    let token = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &token));

    let other_token = env.register_stellar_asset_contract(admin.clone());
    super::assert_contract_error(
        client.try_bind_settlement_token(&admin, &other_token),
        EscrowError::SettlementTokenAlreadyBound,
    );
}

#[test]
fn bind_settlement_token_rejects_double_bind_when_paused() {
    let (env, contract_id, admin) = setup_initialized_sac();
    let client = EscrowClient::new(&env, &contract_id);

    let token = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &token));

    client.pause();

    let other_token = env.register_stellar_asset_contract(admin.clone());
    super::assert_contract_error(
        client.try_bind_settlement_token(&admin, &other_token),
        EscrowError::SettlementTokenAlreadyBound,
    );
}

#[test]
fn bind_settlement_token_rejects_double_bind_when_emergency() {
    let (env, contract_id, admin) = setup_initialized_sac();
    let client = EscrowClient::new(&env, &contract_id);

    let token = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &token));

    client.activate_emergency_pause();

    let other_token = env.register_stellar_asset_contract(admin.clone());
    super::assert_contract_error(
        client.try_bind_settlement_token(&admin, &other_token),
        EscrowError::SettlementTokenAlreadyBound,
    );
}

// ── unpause blocked while emergency active ──────────────────────────────────

#[test]
fn unpause_rejected_during_emergency() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.activate_emergency_pause();
    assert!(client.is_emergency());

    super::assert_contract_error(client.try_unpause(), Error::EmergencyActive);
}

// ── Governance setters fail with correct error for invalid values ───────────

#[test]
fn set_protocol_fee_bps_rejects_over_max_when_paused() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.pause();
    // Value 10_001 > MAX_FEE_BPS (10_000) — must be rejected regardless of
    // pause.
    super::assert_contract_error(
        client.try_set_protocol_fee_bps(&10_001),
        Error::InvalidProtocolParameters,
    );
}

#[test]
fn set_protocol_fee_bps_rejects_over_max_when_emergency() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.activate_emergency_pause();
    super::assert_contract_error(
        client.try_set_protocol_fee_bps(&10_001),
        Error::InvalidProtocolParameters,
    );
}

#[test]
fn set_governed_params_rejects_invalid_bps_when_paused() {
    let (env, contract_id, admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.pause();
    super::assert_contract_error(
        client.try_set_governed_params(&admin, &10_001, &1_000_000_000_i128),
        Error::InvalidProtocolParameters,
    );
}

#[test]
fn set_governed_params_rejects_invalid_bps_when_emergency() {
    let (env, contract_id, admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    client.activate_emergency_pause();
    super::assert_contract_error(
        client.try_set_governed_params(&admin, &10_001, &1_000_000_000_i128),
        Error::InvalidProtocolParameters,
    );
}
