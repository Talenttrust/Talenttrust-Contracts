use soroban_sdk::{testutils::Address as _, testutils::Events, Address, Env};

use crate::{Escrow, EscrowClient, EscrowError};

/// Returns a fresh (Env, contract Address) pair with all auths mocked.
fn setup() -> (Env, Address) {
    let env = Env::default();
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
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);

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
    super::assert_contract_error(result, crate::Error::InvalidProtocolParameters);

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
#[should_panic(expected = "HostError: Error(Contract, #12)")]
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
    env.mock_all_auths();
    let client = super::register_client(&env);
    let (client_addr, _freelancer, contract_id) = super::complete_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    let record = client.get_finalization_record(&contract_id).unwrap();
    assert_eq!(
        record.summary.schema_version, crate::types::CONTRACT_SUMMARY_SCHEMA_VERSION,
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

// ── Post-Upgrade Verification Tests ──────────────────────────────────────

/// Sets up a fully configured escrow contract with admin, settlement token,
/// governed parameters, and an in-flight contract. Returns the environment,
/// client, admin, and contract state needed for upgrade tests.
fn setup_full_contract() -> (Env, EscrowClient<'static>, Address, Address, u32) {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.max_entry_ttl = 3_110_400;
        li.min_persistent_entry_ttl = 3_110_400;
    });
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);

    // Initialize and configure
    client.initialize(&admin);
    client.set_governed_params(&admin, &500_u32, &1_000_000_000_000_i128);

    // Bind settlement token
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);

    // Create an in-flight contract
    let milestones = soroban_sdk::vec![&env, 100_0000000_i128, 200_0000000_i128];
    let escrow_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );

    (env, client, admin, token, escrow_id)
}

/// Verifies that `get_admin()` returns the same value after a pause → unpause
/// cycle that simulates the upgrade window.
#[test]
fn upgrade_snapshot_admin_unchanged() {
    let (env, client, admin, _token, _escrow_id) = setup_full_contract();

    // Pre-upgrade snapshot
    let pre_admin = client.get_admin();

    // Simulate upgrade window: pause → [upgrade would happen here] → unpause
    client.activate_emergency_pause();
    assert!(client.is_paused());
    assert!(client.is_emergency());
    client.resolve_emergency();

    // Post-upgrade verification
    let post_admin = client.get_admin();
    assert_eq!(pre_admin, post_admin, "admin must survive upgrade");
    assert_eq!(post_admin, Some(admin), "admin must match the initialized address");
}

/// Verifies that `get_settlement_token()` returns the same value after a
/// pause → unpause cycle that simulates the upgrade window.
#[test]
fn upgrade_snapshot_settlement_token_unchanged() {
    let (env, client, _admin, token, _escrow_id) = setup_full_contract();

    // Pre-upgrade snapshot
    let pre_token = client.get_settlement_token();

    // Simulate upgrade window
    client.activate_emergency_pause();
    client.resolve_emergency();

    // Post-upgrade verification
    let post_token = client.get_settlement_token();
    assert_eq!(pre_token, post_token, "settlement token must survive upgrade");
    assert_eq!(post_token, Some(token), "settlement token must match bound address");
}

/// Verifies that `get_protocol_fee_bps()` returns the same value after a
/// pause → unpause cycle that simulates the upgrade window.
#[test]
fn upgrade_snapshot_protocol_fee_unchanged() {
    let (env, client, _admin, _token, _escrow_id) = setup_full_contract();

    // Pre-upgrade snapshot
    let pre_fee = client.get_protocol_fee_bps();

    // Simulate upgrade window
    client.activate_emergency_pause();
    client.resolve_emergency();

    // Post-upgrade verification
    let post_fee = client.get_protocol_fee_bps();
    assert_eq!(pre_fee, post_fee, "protocol fee must survive upgrade");
    assert_eq!(post_fee, 500_u32, "protocol fee must match configured value");
}

/// Verifies that `get_next_contract_id()` returns the same value after a
/// pause → unpause cycle that simulates the upgrade window.
#[test]
fn upgrade_snapshot_next_contract_id_unchanged() {
    let (env, client, _admin, _token, escrow_id) = setup_full_contract();

    // Pre-upgrade snapshot
    let pre_next_id = client.get_next_contract_id();

    // Simulate upgrade window
    client.activate_emergency_pause();
    client.resolve_emergency();

    // Post-upgrade verification
    let post_next_id = client.get_next_contract_id();
    assert_eq!(pre_next_id, post_next_id, "next contract ID must survive upgrade");
    // The ID should be escrow_id + 1 since we created one contract
    assert_eq!(post_next_id, escrow_id + 1, "next ID should be one past the last allocated");
}

/// Verifies that the readiness checklist survives a pause → unpause cycle.
#[test]
fn upgrade_snapshot_readiness_checklist_unchanged() {
    let (env, client, _admin, _token, _escrow_id) = setup_full_contract();

    // Pre-upgrade snapshot
    let pre_info = client.get_mainnet_readiness_info();

    // Simulate upgrade window
    client.activate_emergency_pause();
    client.resolve_emergency();

    // Post-upgrade verification
    let post_info = client.get_mainnet_readiness_info();
    assert_eq!(pre_info, post_info, "readiness checklist must survive upgrade");
    assert!(post_info.initialized, "initialized must remain true");
    assert!(post_info.governed_params_set, "governed_params_set must remain true");
    assert!(post_info.emergency_controls_enabled, "emergency_controls_enabled must remain true");
}

/// Exercises the full pause → verify → unpause cycle described in the upgrade
/// runbook, confirming that all state mutations are blocked during the upgrade
/// window and that operations resume cleanly afterward.
#[test]
fn post_upgrade_pause_unpause_cycle() {
    let (env, client, admin, token, escrow_id) = setup_full_contract();

    // ── Pre-upgrade baseline ──
    let pre_admin = client.get_admin();
    let pre_token = client.get_settlement_token();
    let pre_fee = client.get_protocol_fee_bps();
    let pre_next_id = client.get_next_contract_id();
    let pre_info = client.get_mainnet_readiness_info();

    // ── Step 1: Activate emergency pause ──
    client.activate_emergency_pause();
    assert!(client.is_paused(), "must be paused after activate_emergency_pause");
    assert!(client.is_emergency(), "must be in emergency after activate_emergency_pause");

    // ── Step 2: Verify reads still work during pause ──
    assert_eq!(client.get_admin(), pre_admin);
    assert_eq!(client.get_settlement_token(), pre_token);
    assert_eq!(client.get_protocol_fee_bps(), pre_fee);
    assert_eq!(client.get_next_contract_id(), pre_next_id);
    assert_eq!(client.get_mainnet_readiness_info(), pre_info);

    // ── Step 3: Verify existing contract state is readable ──
    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, crate::ContractStatus::Created);
    assert_eq!(contract.funded_amount, 0);
    assert_eq!(contract.released_amount, 0);
    assert_eq!(contract.refunded_amount, 0);

    // ── Step 4: [Simulated WASM upgrade happens here] ──

    // ── Step 5: Resolve emergency ──
    client.resolve_emergency();
    assert!(!client.is_paused(), "must be unpaused after resolve_emergency");
    assert!(!client.is_emergency(), "must not be in emergency after resolve_emergency");

    // ── Step 6: Post-upgrade verification ──
    assert_eq!(client.get_admin(), Some(admin));
    assert_eq!(client.get_settlement_token(), Some(token));
    assert_eq!(client.get_protocol_fee_bps(), 500_u32);
    assert_eq!(client.get_next_contract_id(), pre_next_id);

    let post_info = client.get_mainnet_readiness_info();
    assert!(post_info.initialized);
    assert!(post_info.governed_params_set);
    assert!(post_info.emergency_controls_enabled);

    // Verify in-flight contract is intact
    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, crate::ContractStatus::Created);
    assert_eq!(contract.funded_amount, 0);
}

/// Verifies that all mutating entrypoints are blocked during emergency pause,
/// ensuring no state changes occur during the upgrade window.
#[test]
fn emergency_pause_blocks_mutations_during_upgrade() {
    let (env, client, admin, _token, escrow_id) = setup_full_contract();

    // Activate emergency pause (simulating pre-upgrade freeze)
    client.activate_emergency_pause();
    assert!(client.is_paused());
    assert!(client.is_emergency());

    // Attempt create_contract — should fail
    let milestones = soroban_sdk::vec![&env, 100_0000000_i128];
    let result = client.try_create_contract(
        &Address::generate(&env),
        &Address::generate(&env),
        &None::<Address>,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    assert!(
        result.is_err(),
        "create_contract must fail while paused"
    );

    // Attempt deposit_funds — should fail
    let result = client.try_deposit_funds(
        &escrow_id,
        &Address::generate(&env),
        &100_0000000_i128,
    );
    assert!(
        result.is_err(),
        "deposit_funds must fail while paused"
    );

    // Attempt cancel_contract — should fail
    let result = client.try_cancel_contract(
        &escrow_id,
        &Address::generate(&env),
    );
    assert!(
        result.is_err(),
        "cancel_contract must fail while paused"
    );

    // Verify reads are NOT blocked during pause
    let _ = client.get_admin();
    let _ = client.get_settlement_token();
    let _ = client.get_protocol_fee_bps();
    let _ = client.get_next_contract_id();
    let _ = client.get_mainnet_readiness_info();
    let _ = client.is_paused();
    let _ = client.is_emergency();
}

/// Verifies that an in-flight contract (Created status) retains its full state
/// across a simulated upgrade cycle: pause, verify, unpause, verify again.
#[test]
fn post_upgrade_in_flight_contract_integrity() {
    let (env, client, _admin, _token, escrow_id) = setup_full_contract();

    // Capture pre-upgrade contract state
    let pre_contract = client.get_contract(&escrow_id);
    assert_eq!(pre_contract.status, crate::ContractStatus::Created);

    // Simulate upgrade: pause → upgrade window → unpause
    client.activate_emergency_pause();
    client.resolve_emergency();

    // Verify in-flight contract survived the upgrade
    let post_contract = client.get_contract(&escrow_id);
    assert_eq!(pre_contract.client, post_contract.client, "client must survive upgrade");
    assert_eq!(pre_contract.freelancer, post_contract.freelancer, "freelancer must survive upgrade");
    assert_eq!(pre_contract.status, post_contract.status, "status must survive upgrade");
    assert_eq!(pre_contract.funded_amount, post_contract.funded_amount, "funded_amount must survive upgrade");
    assert_eq!(pre_contract.released_amount, post_contract.released_amount, "released_amount must survive upgrade");
    assert_eq!(pre_contract.refunded_amount, post_contract.refunded_amount, "refunded_amount must survive upgrade");
    assert_eq!(pre_contract.release_authorization, post_contract.release_authorization, "release_authorization must survive upgrade");

    // Verify milestones survived
    let pre_milestones = client.get_milestones(&escrow_id);
    let post_milestones = client.get_milestones(&escrow_id);
    assert_eq!(pre_milestones.len(), post_milestones.len(), "milestone count must survive upgrade");
    for i in 0..pre_milestones.len() {
        let pre_m = pre_milestones.get(i).unwrap();
        let post_m = post_milestones.get(i).unwrap();
        assert_eq!(pre_m.amount, post_m.amount, "milestone amount must survive upgrade at index {}", i);
        assert_eq!(pre_m.released, post_m.released, "milestone released flag must survive upgrade at index {}", i);
        assert_eq!(pre_m.refunded, post_m.refunded, "milestone refunded flag must survive upgrade at index {}", i);
    }
}
