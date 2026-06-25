//! Tests for the pause + emergency gate on mutating escrow entrypoints.
//!
//! Issue #452: `create_contract`, `deposit_funds`, `release_milestone`,
//! `refund_unreleased_milestones`, `issue_reputation`, and `cancel_contract`
//! must all honor the `Paused` and `Emergency` flags so the documented
//! behavior holds. Read-only queries must remain available while paused.
//!
//! Each mutating entrypoint is exercised against three states:
//! 1. **Paused only**: `pause()` blocks the call with `ContractPaused`.
//! 2. **Emergency only** (Paused=false, Emergency=true): the call is blocked
//!    with `EmergencyActive`.
//! 3. **Recovered**: `unpause()` (or `resolve_emergency()`) restores the
//!    happy path.
//!
//! Run locally with `cargo test -p escrow --lib pause_controls`.

use crate::{Escrow, EscrowClient, EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn setup_initialized() -> (Env, EscrowClient<'_>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    (env, client, admin)
}

/// Create a fully-funded contract with `ClientOnly` release authorization and
/// three milestones. Returns `(client_addr, freelancer_addr, contract_id)`.
fn setup_funded_contract(
    env: &Env,
    client: &EscrowClient<'_>,
) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 100_i128, 200_i128, 300_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &client_addr, &(100 + 200 + 300));
    (client_addr, freelancer_addr, id)
}

/// Manually flip the `Emergency` flag on the underlying storage WITHOUT
/// flipping the `Paused` flag (so `require_not_paused()` reaches the
/// Emergency check).
fn set_emergency_only(env: &Env, client: &EscrowClient<'_>) {
    let _: bool = client.activate_emergency_pause();
    // The activate helper sets BOTH flags; we now clear Paused so the gate
    // hits the Emergency check first.
    let contract_addr: Address = client.address.clone();
    env.as_contract(&contract_addr, || {
        env.storage().persistent().set(&crate::DataKey::Paused, &false);
    });
}

// ─── initialize ──────────────────────────────────────────────────────────────

#[test]
fn initialize_only_once_fails() {
    let (env, client, admin) = setup_initialized();
    super::assert_contract_error(client.try_initialize(&admin), EscrowError::AlreadyInitialized);
}

// ─── pause / unpause state ──────────────────────────────────────────────────

#[test]
fn pause_then_unpause_toggles_state() {
    let (_env, client, _admin) = setup_initialized();
    assert!(!client.is_paused());
    assert!(client.pause());
    assert!(client.is_paused());
    assert!(client.unpause());
    assert!(!client.is_paused());
}

#[test]
fn pause_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    super::assert_contract_error(client.try_pause(), EscrowError::NotInitialized);
}

// ─── create_contract blocked ─────────────────────────────────────────────────

/// Paused contract refuses `create_contract` with `ContractPaused`.
#[test]
fn pause_blocks_create_contract() {
    let (_env, client, _admin) = setup_initialized();
    client.pause();

    let a = Address::generate(&_env);
    let b = Address::generate(&_env);
    super::assert_contract_error(
        client.try_create_contract(
            &a,
            &b,
            &None,
            &vec![&_env, 50_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::ContractPaused,
    );
}

/// Emergency-only state refuses `create_contract` with `EmergencyActive`.
#[test]
fn emergency_blocks_create_contract() {
    let (env, client, _admin) = setup_initialized();
    set_emergency_only(&env, &client);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    super::assert_contract_error(
        client.try_create_contract(
            &a,
            &b,
            &None,
            &vec![&env, 50_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::EmergencyActive,
    );
}

/// After `unpause()` `create_contract` succeeds again.
#[test]
fn unpause_restores_create_contract() {
    let (env, client, _admin) = setup_initialized();
    client.pause();
    client.unpause();

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let id = client.create_contract(
        &a,
        &b,
        &None,
        &vec![&env, 50_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);
}

/// After `resolve_emergency()` `create_contract` succeeds again.
#[test]
fn resolve_emergency_restores_create_contract() {
    let (env, client, _admin) = setup_initialized();
    set_emergency_only(&env, &client);
    assert!(client.resolve_emergency());

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let id = client.create_contract(
        &a,
        &b,
        &None,
        &vec![&env, 50_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);
}

// ─── deposit_funds blocked ───────────────────────────────────────────────────

/// Paused contract refuses `deposit_funds` with `ContractPaused`.
#[test]
fn pause_blocks_deposit_funds() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _freelancer_addr, contract_id) = setup_funded_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &50_i128),
        EscrowError::ContractPaused,
    );
}

/// Emergency-only state refuses `deposit_funds` with `EmergencyActive`.
#[test]
fn emergency_blocks_deposit_funds() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    // After Set_up the funded contract, flip into emergency-only mode.
    set_emergency_only(&env, &client);

    super::assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &50_i128),
        EscrowError::EmergencyActive,
    );
}

/// After `unpause()` `deposit_funds` succeeds again.
#[test]
fn unpause_restores_deposit_funds() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    client.pause();
    client.unpause();
    assert!(client.deposit_funds(&contract_id, &client_addr, &50_i128));
}

/// After `resolve_emergency()` `deposit_funds` succeeds again.
#[test]
fn resolve_emergency_restores_deposit_funds() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    set_emergency_only(&env, &client);
    assert!(client.resolve_emergency());
    assert!(client.deposit_funds(&contract_id, &client_addr, &50_i128));
}

// ─── release_milestone blocked ───────────────────────────────────────────────

/// Paused contract refuses `release_milestone` with `ContractPaused`.
#[test]
fn pause_blocks_release_milestone() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_release_milestone(&contract_id, &client_addr, &0),
        EscrowError::ContractPaused,
    );
}

/// Emergency-only state refuses `release_milestone` with `EmergencyActive`.
#[test]
fn emergency_blocks_release_milestone() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    set_emergency_only(&env, &client);

    super::assert_contract_error(
        client.try_release_milestone(&contract_id, &client_addr, &0),
        EscrowError::EmergencyActive,
    );
}

/// After `unpause()` `release_milestone` succeeds again.
#[test]
fn unpause_restores_release_milestone() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    client.pause();
    client.unpause();
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
}

/// After `resolve_emergency()` `release_milestone` succeeds again.
#[test]
fn resolve_emergency_restores_release_milestone() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    set_emergency_only(&env, &client);
    assert!(client.resolve_emergency());
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
}

// ─── refund_unreleased_milestones blocked ───────────────────────────────────

/// Paused contract refuses `refund_unreleased_milestones` with `ContractPaused`.
#[test]
fn pause_blocks_refund_unreleased_milestones() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_refund_unreleased_milestones(&contract_id, &vec![&env, 1_u32]),
        EscrowError::ContractPaused,
    );
}

/// Emergency-only state refuses `refund_unreleased_milestones` with
/// `EmergencyActive`.
#[test]
fn emergency_blocks_refund_unreleased_milestones() {
    let (env, client, _admin) = setup_initialized();
    let (_client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    set_emergency_only(&env, &client);

    super::assert_contract_error(
        client.try_refund_unreleased_milestones(&contract_id, &vec![&env, 1_u32]),
        EscrowError::EmergencyActive,
    );
}

/// After `unpause()` `refund_unreleased_milestones` succeeds again.
#[test]
fn unpause_restores_refund_unreleased_milestones() {
    let (env, client, _admin) = setup_initialized();
    let (_client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    client.pause();
    client.unpause();
    let total = client.refund_unreleased_milestones(&contract_id, &vec![&env, 2_u32]);
    assert_eq!(total, 300_i128);
}

/// After `resolve_emergency()` `refund_unreleased_milestones` succeeds again.
#[test]
fn resolve_emergency_restores_refund_unreleased_milestones() {
    let (env, client, _admin) = setup_initialized();
    let (_client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    set_emergency_only(&env, &client);
    assert!(client.resolve_emergency());
    let total = client.refund_unreleased_milestones(&contract_id, &vec![&env, 2_u32]);
    assert_eq!(total, 300_i128);
}

// ─── issue_reputation blocked ────────────────────────────────────────────────

/// Paused contract refuses `issue_reputation` with `ContractPaused`.
#[test]
fn pause_blocks_issue_reputation() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, freelancer_addr, contract_id) = setup_funded_contract(&env, &client);
    // Drive the contract to Completed.
    client.approve_milestone_release(&contract_id, &client_addr, &0);
    client.release_milestone(&contract_id, &client_addr, &0);
    client.approve_milestone_release(&contract_id, &client_addr, &1);
    client.release_milestone(&contract_id, &client_addr, &1);
    client.approve_milestone_release(&contract_id, &client_addr, &2);
    client.release_milestone(&contract_id, &client_addr, &2);
    client.pause();

    super::assert_contract_error(
        client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5_i128),
        EscrowError::ContractPaused,
    );
}

/// Emergency-only state refuses `issue_reputation` with `EmergencyActive`.
#[test]
fn emergency_blocks_issue_reputation() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, freelancer_addr, contract_id) = setup_funded_contract(&env, &client);
    client.approve_milestone_release(&contract_id, &client_addr, &0);
    client.release_milestone(&contract_id, &client_addr, &0);
    client.approve_milestone_release(&contract_id, &client_addr, &1);
    client.release_milestone(&contract_id, &client_addr, &1);
    client.approve_milestone_release(&contract_id, &client_addr, &2);
    client.release_milestone(&contract_id, &client_addr, &2);
    set_emergency_only(&env, &client);

    super::assert_contract_error(
        client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5_i128),
        EscrowError::EmergencyActive,
    );
}

/// After `unpause()` `issue_reputation` succeeds again.
#[test]
fn unpause_restores_issue_reputation() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, freelancer_addr, contract_id) = setup_funded_contract(&env, &client);
    for i in 0..3u32 {
        client.approve_milestone_release(&contract_id, &client_addr, &i);
        client.release_milestone(&contract_id, &client_addr, &i);
    }
    client.pause();
    client.unpause();
    assert!(client.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5_i128));
}

/// After `resolve_emergency()` `issue_reputation` succeeds again.
#[test]
fn resolve_emergency_restores_issue_reputation() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, freelancer_addr, contract_id) = setup_funded_contract(&env, &client);
    for i in 0..3u32 {
        client.approve_milestone_release(&contract_id, &client_addr, &i);
        client.release_milestone(&contract_id, &client_addr, &i);
    }
    set_emergency_only(&env, &client);
    assert!(client.resolve_emergency());
    assert!(client.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5_i128));
}

// ─── cancel_contract blocked ─────────────────────────────────────────────────

/// Paused contract refuses `cancel_contract` with `ContractPaused`.
#[test]
fn pause_blocks_cancel_contract() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_cancel_contract(&contract_id, &client_addr),
        EscrowError::ContractPaused,
    );
}

/// Emergency-only state refuses `cancel_contract` with `EmergencyActive`.
#[test]
fn emergency_blocks_cancel_contract() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    set_emergency_only(&env, &client);

    super::assert_contract_error(
        client.try_cancel_contract(&contract_id, &client_addr),
        EscrowError::EmergencyActive,
    );
}

/// After `unpause()` `cancel_contract` succeeds again.
#[test]
fn unpause_restores_cancel_contract() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    client.pause();
    client.unpause();
    assert!(client.cancel_contract(&contract_id, &client_addr));
}

/// After `resolve_emergency()` `cancel_contract` succeeds again.
#[test]
fn resolve_emergency_restores_cancel_contract() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _, contract_id) = setup_funded_contract(&env, &client);
    set_emergency_only(&env, &client);
    assert!(client.resolve_emergency());
    assert!(client.cancel_contract(&contract_id, &client_addr));
}

// ─── Read-only queries remain available while paused ────────────────────────

/// Pause does NOT block read-only queries.
#[test]
fn read_only_queries_unaffected_by_pause() {
    let (env, client, _admin) = setup_initialized();
    let (_client_addr, _freelancer_addr, contract_id) = setup_funded_contract(&env, &client);
    client.pause();

    // All reads must succeed while paused.
    let _ = client.is_paused();
    let _ = client.is_emergency();
    let _ = client.get_contract(&contract_id);
    let _ = client.get_milestones(&contract_id);
    let _ = client.get_refundable_balance(&contract_id);
    let _ = client.get_admin();
}

/// Emergency does NOT block read-only queries.
#[test]
fn read_only_queries_unaffected_by_emergency() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _freelancer_addr, contract_id) = setup_funded_contract(&env, &client);
    set_emergency_only(&env, &client);

    assert!(client.is_emergency());
    assert!(!client.is_paused());
    let _ = client.get_contract(&contract_id);
    let _ = client.get_milestones(&contract_id);
    let _ = client.get_refundable_balance(&contract_id);
    let _ = client.get_admin();
    let _ = client_addr;
}

// ─── Cross-check: pause runs BEFORE auth (cycle-safe) ──────────────────────

/// `create_contract` rejects paused state BEFORE invoking `require_auth`,
/// i.e. the paused-state check appears in the panic code path BEFORE the
/// unauthorized-role check. This is asserted by attempting create with the
/// wrong caller; the test must surface ContractPaused, NOT UnauthorizedRole.
#[test]
fn pause_gate_runs_before_auth_on_create_contract() {
    let (env, client, _admin) = setup_initialized();
    client.pause();

    let outsider = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let milestones = vec![&env, 50_i128];

    super::assert_contract_error(
        client.try_create_contract(
            &outsider, // wrong caller
            &client_addr,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::ContractPaused,
    );
}

/// `deposit_funds` rejects paused state BEFORE invoking `require_auth`.
#[test]
fn pause_gate_runs_before_auth_on_deposit_funds() {
    let (env, client, _admin) = setup_initialized();
    let (client_addr, _freelancer_addr, contract_id) = setup_funded_contract(&env, &client);
    client.pause();

    let outsider = Address::generate(&env);
    super::assert_contract_error(
        client.try_deposit_funds(&contract_id, &outsider, &50_i128),
        EscrowError::ContractPaused,
    );
}
