#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{symbol_short, vec, Address, Env, Symbol, Vec};

use super::{assert_contract_error, register_client};
use crate::{Error, EscrowError, EventInput, MAX_EVENT_BATCH_SIZE};

#[test]
fn empty_batch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let empty_events: Vec<EventInput> = vec![&env];
    let res = client.try_batch_events(&caller, &empty_events);
    assert_contract_error(res, Error::EmptyRefundRequest);
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
    assert_eq!(count, MAX_EVENT_BATCH_SIZE);

    let emitted = env.events().all();
    assert!(emitted.len() >= MAX_EVENT_BATCH_SIZE as usize);
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
    assert_contract_error(res, Error::BatchCapExceeded);
}

#[test]
fn per_item_events_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let events = vec![
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
    assert_eq!(count, 2);

    let all_events = env.events().all();
    let found_1 = all_events
        .iter()
        .any(|e| e.1.len() > 0 && e.1.get(0).unwrap() == Symbol::new(&env, "event_1").into());
    let found_2 = all_events
        .iter()
        .any(|e| e.1.len() > 0 && e.1.get(0).unwrap() == Symbol::new(&env, "event_2").into());
    assert!(found_1);
    assert!(found_2);
}

#[test]
fn emit_events_batch_alias_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let events = vec![
        &env,
        EventInput {
            topic: symbol_short!("alias_evt"),
            contract_id: 42,
            data: symbol_short!("alias_dat"),
        },
    ];

    let count = client.emit_events_batch(&caller, &events);
    assert_eq!(count, 1);
}

#[test]
fn events_batch_alias_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let events = vec![
        &env,
        EventInput {
            topic: symbol_short!("alias_evt"),
            contract_id: 43,
            data: symbol_short!("alias_dat"),
        },
    ];

    let count = client.events_batch(&caller, &events);
    assert_eq!(count, 1);
}

#[test]
fn emit_single_event_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let topic = symbol_short!("single_t");
    let data = symbol_short!("single_d");

    let ok = client.emit_event(&caller, &topic, &1, &data);
    assert!(ok);
}

#[test]
fn batch_events_fails_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    client.pause();

    let events = vec![
        &env,
        EventInput {
            topic: symbol_short!("paused_e"),
            contract_id: 1,
            data: symbol_short!("paused_d"),
        },
    ];

    let res = client.try_batch_events(&caller, &events);
    assert_contract_error(res, EscrowError::ContractPaused);
}
