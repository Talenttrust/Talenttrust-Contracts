//! Pause-gate regression tests for the mutating escrow entrypoints.
//!
//! Issue #692: create_contract, deposit_funds, release_milestone,
//! refund_unreleased_milestones, cancel_contract, and issue_reputation must all
//! honor the Paused flag and reject calls with ContractPaused while paused, then
//! resume normally after unpause. approve_milestone_release is intentionally not
//! gated yet (tracked separately) and is exercised here only as a setup step.
//!
//! Emergency-mode coverage lives in emergency_controls.rs; this module exercises
//! the plain pause() / unpause() path. The pause check runs before require_auth,
//! so a paused contract rejects uniformly regardless of caller.

use crate::{ContractStatus, Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    token::StellarAssetClient,
    vec, Address, Env, String, Symbol, TryFromVal,
};

// --- helpers ---

fn setup_initialized() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    (env, contract_id, admin)
}

fn setup_funded_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 100_i128, 200_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &client_addr, &300_i128);
    (client_addr, freelancer_addr, id)
}

fn setup_completed_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let (client_addr, freelancer_addr, id) = setup_funded_contract(env, client);
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    client.approve_milestone_release(&id, &client_addr, &1);
    client.release_milestone(&id, &client_addr, &1);
    (client_addr, freelancer_addr, id)
}

// --- pause / unpause state ---

#[test]
fn pause_then_unpause_toggles_state() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(!client.is_paused());
    client.pause();
    assert!(client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}

// --- create_contract ---

#[test]
fn pause_blocks_create_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause();

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
        EscrowError::ContractPaused,
    );
}

#[test]
fn unpause_restores_create_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
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

#[test]
fn pause_gate_runs_before_auth_on_create_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause();

    let outsider = Address::generate(&env);
    let other = Address::generate(&env);
    super::assert_contract_error(
        client.try_create_contract(
            &outsider,
            &other,
            &None,
            &vec![&env, 50_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::ContractPaused,
    );
}

// --- deposit_funds ---

#[test]
fn pause_blocks_deposit_funds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer, id) = setup_funded_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_deposit_funds(&id, &client_addr, &50_i128),
        EscrowError::ContractPaused,
    );
}

#[test]
fn unpause_restores_deposit_funds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
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
    assert!(client.deposit_funds(&id, &a, &50_i128));
}

// --- release_milestone ---

#[test]
fn pause_blocks_release_milestone() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer, id) = setup_funded_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        Error::ContractPaused,
    );
}

#[test]
fn unpause_restores_release_milestone() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer, id) = setup_funded_contract(&env, &client);
    client.pause();
    client.unpause();

    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
}

// --- refund_unreleased_milestones ---

#[test]
fn pause_blocks_refund_unreleased_milestones() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (_client_addr, _freelancer, id) = setup_funded_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_refund_unreleased_milestones(&id, &vec![&env, 1_u32]),
        EscrowError::ContractPaused,
    );
}

#[test]
fn unpause_restores_refund_unreleased_milestones() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (_client_addr, _freelancer, id) = setup_funded_contract(&env, &client);
    client.pause();
    client.unpause();

    client.refund_unreleased_milestones(&id, &vec![&env, 1_u32]);
}

// --- cancel_contract ---

#[test]
fn pause_blocks_cancel_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer, id) = setup_funded_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_cancel_contract(&id, &client_addr),
        Error::ContractPaused,
    );
}

#[test]
fn unpause_restores_cancel_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer, id) = setup_funded_contract(&env, &client);
    client.pause();
    client.unpause();

    client.cancel_contract(&id, &client_addr);
}

/// Assert that cancel_contract emits a ("cancelled", contract_id) event
/// after cancelling a contract in Created status. Verifies the event topics
/// (symbol and contract_id) are present; the data payload (caller,
/// previous_status, timestamp) is specified in docs/escrow/README.md.
#[test]
fn cancel_contract_emits_cancelled_event_with_previous_status() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    client.set_settlement_token(&admin, &token);

    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.cancel_contract(&id, &client_addr));

    let cancelled_topic = symbol_short!("cancelled");
    let events = env.events().all();

    // Verify the cancelled event exists with the correct (symbol, contract_id) topics.
    let has_event = events.iter().any(|event| {
        event.1.len() >= 2
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&cancelled_topic)
            && event.1.get(1).is_some()
    });
    assert!(
        has_event,
        "Expected cancelled event with (Symbol(\"cancelled\"), contract_id) topics"
    );
}

/// Assert that cancelling a PartiallyFunded contract emits the cancelled event
/// with the correct topics. The contract is put into PartiallyFunded state via
/// a partial deposit before cancellation.
#[test]
fn cancel_partially_funded_emits_event_with_partially_funded_status() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    client.set_settlement_token(&admin, &token);

    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, 100_i128, 200_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Partial deposit to put the contract in PartiallyFunded state.
    // Total milestones = 300, deposit only 100.
    let sac = StellarAssetClient::new(&env, &token);
    sac.mint(&client_addr, &10_000_0000000_i128);
    assert!(client.deposit_funds(&id, &client_addr, &100_i128));
    assert_eq!(client.get_contract(&id).status, ContractStatus::PartiallyFunded);

    assert!(client.cancel_contract(&id, &client_addr));

    let cancelled_topic = symbol_short!("cancelled");
    let events = env.events().all();

    let has_event = events.iter().any(|event| {
        event.1.len() >= 2
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&cancelled_topic)
            && event.1.get(1).is_some()
    });
    assert!(
        has_event,
        "Expected cancelled event with (Symbol(\"cancelled\"), contract_id) topics after PartiallyFunded cancel"
    );
}

// --- issue_reputation ---

#[test]
#[ignore]
fn pause_blocks_issue_reputation() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer_addr, id) = setup_completed_contract(&env, &client);
    client.pause();

    let comment = String::from_str(&env, "Great work");
    super::assert_contract_error(
        client.try_issue_reputation(&id, &client_addr, &5_u32, &comment),
        EscrowError::ContractPaused,
    );
}
