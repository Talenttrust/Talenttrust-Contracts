//! Tests for read API NotFound behavior.
//!
//! `get_contract`, `get_milestones`, and `get_checklist` panic with
//! `EscrowError::ContractNotFound` (error code 9) when the requested data is
//! absent.  The Soroban SDK auto-generates `try_*` client wrappers for every
//! contract function; those wrappers return `Err(Ok(EscrowError::...))` instead
//! of propagating the panic, which is what indexers and off-chain callers should
//! use.

#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, EscrowError};

fn setup() -> (Env, EscrowClient) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &id);
    (env, client)
}

// ── get_contract ──────────────────────────────────────────────────────────────

#[test]
fn get_contract_missing_id_returns_not_found() {
    let (_env, client) = setup();
    assert_eq!(
        client.try_get_contract(&999),
        Err(Ok(EscrowError::ContractNotFound))
    );
}

#[test]
fn get_contract_existing_id_returns_ok() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let id = client.create_contract(&c, &f, &None, &milestones, &None, &None);

    let result = client.try_get_contract(&id);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().client, c);
}

// ── get_milestones ────────────────────────────────────────────────────────────

#[test]
fn get_milestones_missing_id_returns_not_found() {
    let (_env, client) = setup();
    assert_eq!(
        client.try_get_milestones(&999),
        Err(Ok(EscrowError::ContractNotFound))
    );
}

#[test]
fn get_milestones_existing_id_returns_ok() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let id = client.create_contract(&c, &f, &None, &milestones, &None, &None);

    let result = client.try_get_milestones(&id);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 2);
}

// ── get_checklist ─────────────────────────────────────────────────────────────

#[test]
fn get_checklist_absent_returns_not_found() {
    // Fresh contract: no lifecycle ops have been called, so the checklist key
    // is absent from storage.
    let (_env, client) = setup();
    assert_eq!(
        client.try_get_checklist(),
        Err(Ok(EscrowError::ContractNotFound))
    );
}
