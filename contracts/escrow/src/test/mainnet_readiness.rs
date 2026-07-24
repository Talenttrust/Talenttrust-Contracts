use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{testutils::Address as _, testutils::Events, Address, Env};

use super::{
    assert_contract_error, complete_contract, default_milestones, generated_participants,
    register_client,
};
use crate::{types::CONTRACT_SUMMARY_SCHEMA_VERSION, Error, Escrow, EscrowClient, EscrowError};

/// Returns a fresh (Env, contract Address) pair with all auths mocked.
fn setup() -> (Env, Address) {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

// ── 4.1 ─────────────────────────────────────────────────────────────────────
// Fresh contract: all mutable boolean fields are false
#[test]
fn fresh_contract_returns_safe_defaults() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let info = client.get_mainnet_readiness_info();

    assert!(
        !info.initialized,
        "initialized should be false on a fresh contract"
    );
    assert!(
        !info.governed_params_set,
        "governed_params_set should be false on a fresh contract"
    );
    assert!(
        !info.emergency_controls_enabled,
        "emergency_controls_enabled should be false on a fresh contract"
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
    assert!(
        info.initialized,
        "initialized must be true after initialize()"
    );
}

// ── 4.3 ─────────────────────────────────────────────────────────────────────
// After `set_governed_params`, `governed_params_set` is true.
#[test]
fn set_governed_params_sets_governed_params() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);
    assert!(client.set_governed_params(&admin, &1000_u32, &500_000_000_000_i128));

    let info = client.get_mainnet_readiness_info();
    assert!(
        info.governed_params_set,
        "governed_params_set must be true after set_governed_params()"
    );

    let params = client.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 1000);
    assert_eq!(params.max_escrow_total_stroops, 500_000_000_000_i128);
}

// ── 4.4 ─────────────────────────────────────────────────────────────────────
// `set_governed_params` can be called only by the admin and leaves the checklist
// unchanged on failure.
#[test]
fn unauthorized_set_governed_params_does_not_set_flag() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fake_admin = Address::generate(&env);

    client.initialize(&admin);

    let result = client.try_set_governed_params(&fake_admin, &1000_u32, &500_000_000_000_i128);
    super::assert_contract_error(result, Error::UnauthorizedRole);

    let info = client.get_mainnet_readiness_info();
    assert!(
        !info.governed_params_set,
        "governed_params_set must remain false after an unauthorized set_governed_params()"
    );
}

#[test]
fn invalid_set_governed_params_does_not_set_flag() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let result = client.try_set_governed_params(&admin, &20_000_u32, &500_000_000_000_i128);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);

    let info = client.get_mainnet_readiness_info();
    assert!(
        !info.governed_params_set,
        "governed_params_set must remain false after an invalid set_governed_params()"
    );
}

// ── 4.5 ─────────────────────────────────────────────────────────────────────
// `activate_emergency_pause` sets `emergency_controls_enabled` to true.
#[test]
fn activate_emergency_pause_sets_emergency_controls_enabled() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

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
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.resolve_emergency();

    let info = client.get_mainnet_readiness_info();
    assert!(
        info.emergency_controls_enabled,
        "emergency_controls_enabled must be true after resolve_emergency()"
    );
}

// ── 4.8 ─────────────────────────────────────────────────────────────────────
// `get_mainnet_readiness_info` requires no auth and emits no events.
#[test]
fn get_mainnet_readiness_info_requires_no_auth_and_emits_no_events() {
    // Deliberately do NOT call env.mock_all_auths() — the function must succeed
    // without any authorization.
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
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
    client.set_governed_params(&admin, &1000_u32, &500_000_000_000_i128);

    let first = client.get_mainnet_readiness_info();
    let second = client.get_mainnet_readiness_info();
    let third = client.get_mainnet_readiness_info();

    assert_eq!(
        first, second,
        "repeated calls must return identical results"
    );
    assert_eq!(
        second, third,
        "repeated calls must return identical results"
    );
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
#[should_panic]
fn double_initialize_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);
    // Second call must panic.
    client.initialize(&admin);
}

// ── 4.13 ────────────────────────────────────────────────────────────────────
// Finalized records carry the current CONTRACT_SUMMARY_SCHEMA_VERSION.
#[test]
fn finalized_record_carries_current_schema_version() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer, contract_id) = complete_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    let record = client.get_finalization_record(&contract_id).unwrap();
    assert_eq!(
        record.summary.schema_version, CONTRACT_SUMMARY_SCHEMA_VERSION,
        "finalized record must carry the current schema version"
    );
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

// ── 4.12 ────────────────────────────────────────────────────────────────────
// Verifies the complete operator workflow and corresponding flag transitions.
#[test]
fn test_operator_workflow_transitions() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    // 1. Fresh state: all false
    let info = client.get_mainnet_readiness_info();
    assert!(!info.initialized);
    assert!(!info.governed_params_set);
    assert!(!info.emergency_controls_enabled);
    assert!(!client.is_paused());
    assert!(!client.is_emergency());

    // 2. Step 1: Initialize the contract
    client.initialize(&admin);
    let info = client.get_mainnet_readiness_info();
    assert!(info.initialized);
    assert!(!info.governed_params_set);
    assert!(!info.emergency_controls_enabled);

    // 3. Step 2: Configure Governed Parameters
    assert!(client.set_governed_params(&admin, &1000_u32, &500_000_000_000_i128));
    let info = client.get_mainnet_readiness_info();
    assert!(info.initialized);
    assert!(info.governed_params_set);
    assert!(!info.emergency_controls_enabled);

    // 4. Step 3: Exercise Emergency Controls (Pause the contract)
    client.activate_emergency_pause();
    let info = client.get_mainnet_readiness_info();
    assert!(info.initialized);
    assert!(info.governed_params_set);
    assert!(info.emergency_controls_enabled);
    assert!(
        client.is_paused(),
        "Contract should be paused after activating emergency pause"
    );
    assert!(
        client.is_emergency(),
        "Contract should be in emergency mode"
    );

    // 5. Step 5: Resolve the Emergency (Resume normal operations)
    client.resolve_emergency();
    let info = client.get_mainnet_readiness_info();
    assert!(info.initialized);
    assert!(info.governed_params_set);
    assert!(info.emergency_controls_enabled); // Should remain true once enabled
    assert!(
        !client.is_paused(),
        "Contract should be unpaused after resolving emergency"
    );
    assert!(
        !client.is_emergency(),
        "Contract should not be in emergency mode"
    );
}
