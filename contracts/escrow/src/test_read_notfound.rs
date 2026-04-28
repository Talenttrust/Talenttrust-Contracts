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

fn setup() -> (Env, soroban_sdk::Address) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    (env, id)
}

fn assert_not_found<T: core::fmt::Debug>(result: Result<Result<T, soroban_sdk::ConversionError>, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>) {
    match result {
        Err(Ok(e)) => {
            let expected: soroban_sdk::Error = EscrowError::ContractNotFound.into();
            assert_eq!(e, expected);
        }
        other => panic!("expected ContractNotFound error, got {:?}", other),
    }
}

// ── get_contract ──────────────────────────────────────────────────────────────

#[test]
fn get_contract_missing_id_returns_not_found() {
    let (env, id) = setup();
    let client = EscrowClient::new(&env, &id);
    assert_not_found(client.try_get_contract(&999));
}

#[test]
fn get_contract_existing_id_returns_ok() {
    let (env, id) = setup();
    let client = EscrowClient::new(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let cid = client.create_contract(&c, &f, &milestones);

    let result = client.try_get_contract(&cid);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap().client, c);
}

// ── get_milestones ────────────────────────────────────────────────────────────

#[test]
fn get_milestones_missing_id_returns_not_found() {
    let (env, id) = setup();
    let client = EscrowClient::new(&env, &id);
    assert_not_found(client.try_get_milestones(&999));
}

#[test]
fn get_milestones_existing_id_returns_ok() {
    let (env, id) = setup();
    let client = EscrowClient::new(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let cid = client.create_contract(&c, &f, &milestones);

    let result = client.try_get_milestones(&cid);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap().len(), 2);
}

// ── get_checklist ─────────────────────────────────────────────────────────────

#[test]
fn get_checklist_absent_returns_not_found() {
    // Fresh contract: no lifecycle ops have been called, so the checklist key
    // is absent from storage.
    let (env, id) = setup();
    let client = EscrowClient::new(&env, &id);
    assert_not_found(client.try_get_checklist());
}
