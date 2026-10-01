#![cfg(test)]

use soroban_sdk::testutils::{address as _, Events as _};
use soroban_sdk::{symbol_short, vec, Address, Env, Symbol, Vec};

use super::{assert_contract_error, register_client};
use crate::{Error, EscrowError, EventInput, MAX_EVENT_BATCH_SIZE};

/// The escrow event batch entry points must fail deterministically:
/// - an empty batch is rejected before any state is touched,
/// - a batch at the cap is accepted and emits exactly one event per item,
/// - a batch over the cap is rejected without partial emission,
/// - a paused contract rejects all batch calls.
///
/// These tests pin the observable failure behavior so retries and
/// concurrent callers can rely on a consistent result for every input.

#[test]
fn empty_batch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let empty_events: Vec<EventInput> = vec![&env];
    let res = client.try_batch_events(&caller, &empty_events);
    assert_contract_error(res, Error::EmptyRefundRequest);

    // No events may be emitted for a rejected batch.
    assert_eq1(env.events().all().len(), 0);
}

#[test]
fn at_cap_batch_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let mut events = Vec::new(&env);
    for i in 0..MAX_EVENT_BATCH_SIZE {
        events.push_back(EventInput {
            topic: symbol_short!("evt_topic"),
            contract_id: i + 1,
            data: symbol_short!("evt_data"),
        });
    }

    let count = client.batch_events(&caller, &events);
    assert_eq(count, MAX_EVENT_BATCH_SIZE);

    // One event per item is emitted with no loss.
    let emitted = env.events().all();
    assert_eq(emitted.len(), MAX_EVENT_BATCH_SIZE as usize);
}

#[test]
fn over_cap_batch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let mut events = Vec::new(&env);
    for i in 0..=MAX_EVENT_BATCH_SIZE {
        events.push_back(EventInput {
            topic: symbol_short!("evt_topic"),
            contract_id: i + 1,
            data: symbol_short!("evt_data"),
        });
    }

    let res = client.try_batch_events(&caller, &events);
    assert_contract_error(res, Error::InvalidProtocolParameters);

    // Rejection is atomic: no partial emission occurs.
    assert_eq1(env.events().all().len(), 0);
}

#[test]
fn per_item_events_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let events = vec[
        &env,
        EventInput {
            topic: Symbol::new(&env, "event_1"),
            contract_id: 101,
            data: Symbol::new(&env, "data_1"),
        },
        EventInput {
            topic: Symbol::new(&env, "event_2"),
            contract_id: 102,
            data: Symbol::new(&env, "data_2"),
        },
    ];

    let count = client.batch_events(&caller, &events);
    assert_eq(count, 2);

    let all_events = env.events().all();
    let found_1 = all_events
        .iter()
        .any(|e | e.1.len() > 0 && e.1.get(0).unwrap() == Symbol::new(&env, "event_1").convert());
    let found_2 = all_events
        .iter()
        .any(|e | e.1.len() > 0 && e.1.get(0).unwrap() == Symbol::new(&env, "event_2").convert());
    assert!(found_1);
    assert!(found_2);
}

#[test]
fn emit_events_batch_alias_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let events = vec[
        &env,
        EventInput {
            topic: symbol_short!("alias_evt"),
            contract_id: 42,
            data: symbol_short!("alias_dat"),
        },
    ];

    let count = client.emit_events_batch(&caller, &events);
    assert_eq(count, 1);
}

#[test]
fn events_batch_alias_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let events = vec[
        &env,
        EventInput {
            topic: symbol_short!("alias_evt"),
            contract_id: 43,
            data: symbol_short!("alias_dat"),
        },
    ];

    let count = client.events_batch(&caller, &events);
    assert_eq(count, 1);
}

#[test]
fn emit_single_event_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let topic = symbol_short!("single_t");
    let data = symbol_short!("single_d");

    let ok = client.emit_event(&caller, &topic, &data);
    assert!(ok);
}

#[test]
fn batch_events_fails_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    client.pause();

    let events = vec[
        &env,
        EventInput {
            topic: symbol_short!("paused_e"),
            contract_id: 1,
            data: symbol_short!("paused_d"),
        },
    ];

    let res = client.try_batch_events(&caller, &events);
    assert_contract_error(res, EscrowError::ContractPaused);

    // A paused rejection must not leak any events.
    assert_eq1(env.events().all().len(), 0);
}

#[test]
fn batch_events_retry_is_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let events = vec![
        &env,
        EventInput {
            topic: symbol_short!("retry_ev"),
            contract_id: 7,
            data: symbol_short!("retry_da"),
        },
    ];

    // A retry after a successful batch must report the same count and
    // emit the same number of events.
    let first = client.batch_events(&caller, &events);
    let first_emitted = env.events().all().len();

    let second = client.batch_events(&caller, &events);
    let second_emitted = env.events().all().len();

    assert_eq(first, second);
    assert_eq(second_emitted - first_emitted, first);
}

#[test]
fn batch_events_rejection_leaves_no_partial_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    // Over-cap batch is rejected and must not emit any events from the
    // valid prefix of the batch.
    let mut events = Vec::new(&env);
    for i in 0..=MAX_EVENT_BATCH_SIZE {
        events.push_back(EventInput {
            topic: symbol_short!("partial"),
            contract_id: i + 1,
            data: symbol_short!("partial"),
        });
    }

    let res = client.try_batch_events(&caller, &events);
    assert_contract_error(res, Error::InvalidProtocolParameters);
    assert_eq1(env.events().all().len(), 0);

    // A follow-up valid batch must still succeed, proving the failure
    // did not corrupt contract state.
    let valid = vec![
        &env,
        EventInput {
            topic: symbol_short!("recover"),
            contract_id: 99,
            data: symbol_short!("recover"),
        },
    ];
    let count = client.batch_events(&caller, &valid);
    assert_eq(count, 1);
    assert_eq(env.events().all().len(), 1);
}
