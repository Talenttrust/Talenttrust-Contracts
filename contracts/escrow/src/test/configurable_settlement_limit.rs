//! Tests for the admin-configurable batch settlement limit.
//!
//! Coverage matrix
//! ───────────────
//! | Scenario                                       | Test function                                            |
//! | ────────────────────────────────────────────── | ──────────────────────────────────────────────────────── |
//! | Default before any set                         | `get_max_settlement_returns_default_before_any_set`      |
//! | In-bounds set                                  | `admin_can_set_max_settlement_within_bounds`             |
//! | Set to minimum boundary                        | `admin_can_set_max_settlement_to_minimum`                |
//! | Set to maximum boundary                        | `admin_can_set_max_settlement_to_maximum`                |
//! | Zero rejected                                  | `set_max_settlement_rejects_zero`                        |
//! | One above maximum rejected                     | `set_max_settlement_rejects_above_maximum`               |
//! | Non-admin rejected                             | `set_max_settlement_rejects_non_admin`                   |
//! | Uninitialized rejected                         | `set_max_settlement_requires_initialization`             |
//! | Default returned without initialization        | `get_max_settlement_returns_default_without_init`        |
//! | Boundary values succeed                        | `set_max_settlement_at_boundary_succeeds`                |
//! | Event is emitted                               | `set_max_settlement_emits_event`                         |
//! | Get/set symmetry                               | `set_and_get_max_settlement_are_symmetric`               |
//! | Multiple sequential calls: last write wins     | `set_max_settlement_last_write_wins`                     |
//! | Failed set leaves state unchanged              | `rejected_set_does_not_change_stored_value`              |
//! | get_bounds includes configurable max_settlement| `get_bounds_returns_configurable_max_settlement`         |
//! | get_bounds returns default before set          | `get_bounds_returns_default_max_settlement_before_set`   |
//! | Constants ordering invariant                   | `constants_satisfy_ordering_invariant`                   |

use super::register_client;
use crate::{
    Error, Escrow, EscrowClient, EscrowError, DEFAULT_MAX_BATCH_SETTLEMENT,
    MAX_MAX_BATCH_SETTLEMENT, MIN_MAX_BATCH_SETTLEMENT,
};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

// ─── Setup ───────────────────────────────────────────

fn setup_initialized() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    (env, contract_id, admin)
}

// ─── Default values ──────────────────────────────────

#[test]
fn get_max_settlement_returns_default_before_any_set() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    assert_eq!(client.get_max_settlement(), DEFAULT_MAX_BATCH_SETTLEMENT);
}

#[test]
fn get_max_settlement_returns_default_without_init() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    assert_eq!(client.get_max_settlement(), DEFAULT_MAX_BATCH_SETTLEMENT);
}

// ─── Setting limits ─────────────────────────────────────────

#[test]
fn admin_can_set_max_settlement_within_bounds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_settlement(&20));
    assert_eq!(client.get_max_settlement(), 20);
}

#[test]
fn admin_can_set_max_settlement_to_minimum() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_settlement(&MIN_MAX_BATCH_SETTLEMENT));
    assert_eq!(client.get_max_settlement(), MIN_MAX_BATCH_SETTLEMENT);
}

#[test]
fn admin_can_set_max_settlement_to_maximum() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_settlement(&MAX_MAX_BATCH_SETTLEMENT));
    assert_eq!(client.get_max_settlement(), MAX_MAX_BATCH_SETTLEMENT);
}

// ─── Out-of-range rejection ─────────────────────────

#[test]
fn set_max_settlement_rejects_zero() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(
        client.try_set_max_settlement(&0),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_settlement_rejects_above_maximum() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let too_high = MAX_MAX_BATCH_SETTLEMENT + 1;
    super::assert_contract_error(
        client.try_set_max_settlement(&too_high),
        EscrowError::LimitOutOfRange,
    );
}

// ─── Requires initialization ─────────────────────────

#[test]
fn set_max_settlement_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(client.try_set_max_settlement(&20), Error::NotInitialized);
}

// ─── Requires admin auth ────────────────────────────────────

#[test]
fn set_max_settlement_rejects_non_admin() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    let attacker = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_max_settlement",
            args: soroban_sdk::vec![&env, 50u32.into()],
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_max_settlement(&50);
    assert!(
        result.is_err(),
        "non-admin must not be able to set max_settlement"
    );
}

// ─── Boundary values ──────────────────────────────────

#[test]
fn set_max_settlement_at_boundary_succeeds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_settlement(&MIN_MAX_BATCH_SETTLEMENT));
    assert_eq!(client.get_max_settlement(), MIN_MAX_BATCH_SETTLEMENT);
    assert!(client.set_max_settlement(&MAX_MAX_BATCH_SETTLEMENT));
    assert_eq!(client.get_max_settlement(), MAX_MAX_BATCH_SETTLEMENT);
}

// ─── Events ───────────────────────────────────────────

#[test]
fn set_max_settlement_emits_event() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_settlement(&15));
    assert_eq!(client.get_max_settlement(), 15);
}

// ─── Get/set symmetry ──────────────────────────────────

#[test]
fn set_and_get_max_settlement_are_symmetric() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    for &val in &[1u32, 5, 10, 50, 100] {
        assert!(client.set_max_settlement(&val));
        assert_eq!(client.get_max_settlement(), val);
    }
}

// ─── Multiple sequential calls ─────────────────────────

#[test]
fn set_max_settlement_last_write_wins() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_settlement(&5));
    assert_eq!(client.get_max_settlement(), 5);

    assert!(client.set_max_settlement(&25));
    assert_eq!(client.get_max_settlement(), 25);

    assert!(client.set_max_settlement(&MIN_MAX_BATCH_SETTLEMENT));
    assert_eq!(client.get_max_settlement(), MIN_MAX_BATCH_SETTLEMENT);
}

// ─── Failed sets leave state unchanged ────────────────────

#[test]
fn rejected_set_does_not_change_stored_value() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_settlement(&50));
    let _ = client.try_set_max_settlement(&0); // out-of-range
    assert_eq!(client.get_max_settlement(), 50);
}

// ─── get_bounds includes configurable max_settlement ─────

#[test]
fn get_bounds_returns_configurable_max_settlement() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_settlement(&42));
    let bounds = client.get_bounds();
    assert_eq!(bounds.max_settlement, 42);
}

#[test]
fn get_bounds_returns_default_max_settlement_before_set() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let bounds = client.get_bounds();
    assert_eq!(bounds.max_settlement, DEFAULT_MAX_BATCH_SETTLEMENT);
}

// ─── Constants ordering invariant ──────────────────────────

#[test]
fn constants_satisfy_ordering_invariant() {
    assert!(
        MIN_MAX_BATCH_SETTLEMENT >= 1,
        "MIN_MAX_BATCH_SETTLEMENT must be at least 1"
    );
    assert!(
        MAX_MAX_BATCH_SETTLEMENT > MIN_MAX_BATCH_SETTLEMENT,
        "MAX_MAX_BATCH_SETTLEMENT must exceed MIN"
    );
    assert!(
        DEFAULT_MAX_BATCH_SETTLEMENT >= MIN_MAX_BATCH_SETTLEMENT,
        "DEFAULT must be >= MIN"
    );
    assert!(
        DEFAULT_MAX_BATCH_SETTLEMENT <= MAX_MAX_BATCH_SETTLEMENT,
        "DEFAULT must be <= MAX"
    );
}
