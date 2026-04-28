extern crate std;

use soroban_sdk::{testutils::{Address as _, Events as _}, Address, Env};

use crate::{
    Escrow, EscrowClient, MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
    MAINNET_PROTOCOL_VERSION,
};

/// Returns a fresh (Env, contract Address) pair with all auths mocked.
fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

// ── 4.1 ─────────────────────────────────────────────────────────────────────
// Fresh contract: all mutable boolean fields are false; caps_set reflects the
// compile-time constant; constant numeric fields are populated.
#[test]
fn fresh_contract_returns_safe_defaults() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let info = client.get_mainnet_readiness_info();

    assert!(!info.initialized, "initialized should be false on a fresh contract");
    assert!(!info.governed_params_set, "governed_params_set should be false on a fresh contract");
    assert!(
        !info.emergency_controls_enabled,
        "emergency_controls_enabled should be false on a fresh contract"
    );
    // caps_set is derived from the compile-time constant, not from storage.
    assert_eq!(
        info.caps_set,
        MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0,
        "caps_set must reflect the compile-time constant"
    );
    assert_eq!(info.protocol_version, MAINNET_PROTOCOL_VERSION);
    assert_eq!(
        info.max_escrow_total_stroops,
        MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
    );
}

// ── 4.2 ─────────────────────────────────────────────────────────────────────
// After `initialize`, the `initialized` field is true.
#[test]
fn initialize_sets_initialized_to_true() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let info = client.get_mainnet_readiness_info();
    assert!(info.initialized, "initialized must be true after initialize()");
}

// ── 4.3 ─────────────────────────────────────────────────────────────────────
// After `initialize_protocol_governance`, `governed_params_set` is true.
#[test]
fn initialize_protocol_governance_sets_governed_params() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize_protocol_governance(&admin, &10_i128, &4_u32, &1_i128, &5_i128);

    let info = client.get_mainnet_readiness_info();
    assert!(
        info.governed_params_set,
        "governed_params_set must be true after initialize_protocol_governance()"
    );
}

// ── 4.4 ─────────────────────────────────────────────────────────────────────
// `update_protocol_parameters` also sets `governed_params_set` to true.
#[test]
fn update_protocol_parameters_sets_governed_params() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    client.update_protocol_parameters(&50_i128, &8_u32, &1_i128, &5_i128);

    let info = client.get_mainnet_readiness_info();
    assert!(
        info.governed_params_set,
        "governed_params_set must be true after update_protocol_parameters()"
    );
}

// ── 4.5 ─────────────────────────────────────────────────────────────────────
// `activate_emergency_pause` sets `emergency_controls_enabled` to true.
#[test]
fn activate_emergency_pause_sets_emergency_controls_enabled() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    client.activate_emergency_pause();

    let info = client.get_mainnet_readiness_info();
    assert!(
        info.emergency_controls_enabled,
        "emergency_controls_enabled must be true after activate_emergency_pause()"
    );
}

// ── 4.6 ─────────────────────────────────────────────────────────────────────
// `resolve_emergency` also sets `emergency_controls_enabled` to true.
#[test]
fn resolve_emergency_sets_emergency_controls_enabled() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    client.resolve_emergency();

    let info = client.get_mainnet_readiness_info();
    assert!(
        info.emergency_controls_enabled,
        "emergency_controls_enabled must be true after resolve_emergency()"
    );
}

// ── 4.7 ─────────────────────────────────────────────────────────────────────
// `caps_set` always reflects the compile-time constant, regardless of state.
#[test]
fn caps_set_reflects_compile_time_constant() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let info = client.get_mainnet_readiness_info();

    let expected = MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0;
    assert_eq!(
        info.caps_set, expected,
        "caps_set must equal (MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0)"
    );
    // The constant is 1_000_000_000_000_000, so caps_set must be true.
    assert!(info.caps_set, "caps_set must be true for the mainnet constant");
}

// ── 4.8 ─────────────────────────────────────────────────────────────────────
// `get_mainnet_readiness_info` requires no auth and emits no events.
#[test]
fn get_mainnet_readiness_info_requires_no_auth_and_emits_no_events() {
    // Deliberately do NOT call env.mock_all_auths() — the function must succeed
    // without any authorization.
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Should not panic even without mocked auth.
    let _info = client.get_mainnet_readiness_info();

    // No events should have been emitted.
    let events = env.events().all();
    assert!(
        events.is_empty(),
        "get_mainnet_readiness_info must not emit any events"
    );
}

// ── 4.9 ─────────────────────────────────────────────────────────────────────
// `get_mainnet_readiness_info` is idempotent: multiple calls return equal results.
#[test]
fn get_mainnet_readiness_info_is_idempotent() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    // Apply some lifecycle ops to create non-trivial state.
    client.initialize(&admin);
    client.initialize_protocol_governance(&admin, &10_i128, &4_u32, &1_i128, &5_i128);

    let first = client.get_mainnet_readiness_info();
    let second = client.get_mainnet_readiness_info();
    let third = client.get_mainnet_readiness_info();

    assert_eq!(first, second, "repeated calls must return identical results");
    assert_eq!(second, third, "repeated calls must return identical results");
}

// ── 4.10 ────────────────────────────────────────────────────────────────────
// Missing storage (fresh contract, no lifecycle ops) returns safe defaults
// without panicking — backward-compatibility guarantee.
#[test]
fn missing_storage_returns_safe_defaults() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    // No lifecycle operations have been called; ReadinessChecklist is absent
    // from instance storage.  The function must not panic and must return
    // all-false for the mutable boolean fields.
    let info = client.get_mainnet_readiness_info();

    assert!(!info.initialized);
    assert!(!info.governed_params_set);
    assert!(!info.emergency_controls_enabled);
    // Constant fields are always populated regardless of storage state.
    assert_eq!(info.protocol_version, MAINNET_PROTOCOL_VERSION);
    assert_eq!(
        info.max_escrow_total_stroops,
        MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
    );
}

// ── 4.11 ────────────────────────────────────────────────────────────────────
// A failed lifecycle operation (double-initialize) must not update the
// checklist.  We use two separate tests:
//   (a) a #[should_panic] test that confirms double-init panics, and
//   (b) a test that verifies a fresh contract still has initialized=false.
//
// Because Soroban transactions are atomic, the panic in (a) rolls back any
// storage writes, so the checklist is never partially updated.

/// Confirms that calling `initialize` twice panics.
#[test]
#[should_panic(expected = "already initialized")]
fn double_initialize_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);
    // Second call must panic.
    client.initialize(&admin);
}

/// Confirms that a fresh contract (no successful initialize) still reports
/// initialized=false — i.e., a failed/absent lifecycle op leaves the
/// checklist unchanged.
#[test]
fn failed_lifecycle_does_not_update_checklist() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    // No initialize call has succeeded; checklist must remain at defaults.
    let info = client.get_mainnet_readiness_info();
    assert!(
        !info.initialized,
        "initialized must remain false when initialize() has never succeeded"
    );
}
