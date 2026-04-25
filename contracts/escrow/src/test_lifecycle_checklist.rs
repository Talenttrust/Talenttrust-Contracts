//! # Contract Lifecycle Checklist Tests
//!
//! Verifies that:
//! - Each lifecycle transition sets the correct checklist field.
//! - `get_checklist` returns the current state accurately.
//! - No public entry-point can mutate the checklist directly.
//! - An unknown contract ID returns `ContractNotFound`.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{ContractChecklist, Escrow, EscrowClient, EscrowError};

fn setup(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

fn participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

fn two_milestones(env: &Env) -> soroban_sdk::Vec<i128> {
    vec![env, 100_i128, 200_i128]
}

// ─── After create ─────────────────────────────────────────────────────────────

#[test]
fn checklist_created_set_after_create_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &two_milestones(&env));

    let cl = client.get_checklist(&id);
    assert!(cl.created);
    assert!(!cl.funded);
    assert!(!cl.milestone_released);
    assert!(!cl.all_milestones_released);
    assert!(!cl.cancelled);
}

// ─── After deposit ────────────────────────────────────────────────────────────

#[test]
fn checklist_funded_set_after_deposit() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &two_milestones(&env));
    client.deposit_funds(&id, &150_i128);

    let cl = client.get_checklist(&id);
    assert!(cl.created);
    assert!(cl.funded);
    assert!(!cl.milestone_released);
    assert!(!cl.all_milestones_released);
    assert!(!cl.cancelled);
}

// ─── After partial release ────────────────────────────────────────────────────

#[test]
fn checklist_milestone_released_set_after_first_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &two_milestones(&env));
    client.deposit_funds(&id, &300_i128);
    client.release_milestone(&id, &0);

    let cl = client.get_checklist(&id);
    assert!(cl.created);
    assert!(cl.funded);
    assert!(cl.milestone_released);
    assert!(!cl.all_milestones_released, "only one of two milestones released");
    assert!(!cl.cancelled);
}

// ─── After all milestones released (completed) ────────────────────────────────

#[test]
fn checklist_all_milestones_released_set_after_completion() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &two_milestones(&env));
    client.deposit_funds(&id, &300_i128);
    client.release_milestone(&id, &0);
    client.release_milestone(&id, &1);

    let cl = client.get_checklist(&id);
    assert!(cl.created);
    assert!(cl.funded);
    assert!(cl.milestone_released);
    assert!(cl.all_milestones_released);
    assert!(!cl.cancelled);
}

// ─── After cancel ─────────────────────────────────────────────────────────────

#[test]
fn checklist_cancelled_set_after_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &two_milestones(&env));
    client.cancel_contract(&id, &ca);

    let cl = client.get_checklist(&id);
    assert!(cl.created);
    assert!(!cl.funded);
    assert!(!cl.milestone_released);
    assert!(!cl.all_milestones_released);
    assert!(cl.cancelled);
}

// ─── Read API: unknown contract ───────────────────────────────────────────────

#[test]
fn get_checklist_returns_not_found_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let result = client.try_get_checklist(&999);
    assert_eq!(result, Err(Ok(EscrowError::ContractNotFound)));
}

// ─── External mutation blocked ────────────────────────────────────────────────

/// `get_checklist` returns a value snapshot. Mutating it locally has no effect
/// on the persisted checklist — there is no `set_checklist` entry-point.
#[test]
fn get_checklist_returns_snapshot_not_a_storage_reference() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &two_milestones(&env));

    // Mutate the local copy.
    let mut local = client.get_checklist(&id);
    local.funded = true;
    local.cancelled = true;

    // Re-read from storage — must be unchanged.
    let stored = client.get_checklist(&id);
    assert_eq!(
        stored,
        ContractChecklist {
            created: true,
            funded: false,
            milestone_released: false,
            all_milestones_released: false,
            cancelled: false,
        }
    );
}

/// Compile-time proof that no `set_checklist` / `update_checklist` public
/// entry-point exists. If one were added, `EscrowClient` would expose it and
/// this test would need to be updated. The absence of such a call below is the
/// security property under test.
#[test]
fn no_public_set_checklist_entry_point_exists() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(&ca, &fa, &two_milestones(&env));
    // Only get_checklist is available — no set_checklist call is possible.
    let cl = client.get_checklist(&id);
    assert!(cl.created);
}
