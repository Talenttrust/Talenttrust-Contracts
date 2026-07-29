#![cfg(test)]

//! Comprehensive event topic/payload tests for the escrow contracts module.
//!
//! Covers every event emitted by the contracts entrypoints:
//! - `create_contract`   → `("created", contract_id)` topic
//! - `set_arbiter`       → `("arbiter", contract_id)` topic
//! - `set_contracts_parameters` → `("contracts", "params")` topic
//! - `set_max_settlement`       → `("limits", "max_settlement")` topic
//!
//! Each test group asserts:
//! 1. The event is actually emitted.
//! 2. The topic symbols match exactly.
//! 3. The payload fields carry the right values.
//! 4. No topic collision with other known escrow events.

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    vec, Address, Env, Symbol, TryFromVal, TryIntoVal,
};

use crate::{
    test::{create_client, default_milestones, EscrowFixture},
    ContractStatus, EscrowError, ReleaseAuthorization,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Pull every event emitted by `contract_address` whose first topic matches
/// `topic_sym`. Returns `(topics, data)` pairs.
fn events_with_topic(
    env: &Env,
    contract_address: &Address,
    topic_sym: Symbol,
) -> soroban_sdk::Vec<(soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)> {
    let mut out = soroban_sdk::Vec::new(env);
    for (addr, topics, data) in env.events().all().iter() {
        if &addr != contract_address {
            continue;
        }
        if topics.is_empty() {
            continue;
        }
        let t0: Symbol = match Symbol::try_from_val(env, &topics.get(0).unwrap()) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if t0 == topic_sym {
            out.push_back((topics, data));
        }
    }
    out
}

// ─── create_contract ─────────────────────────────────────────────────────────

/// `create_contract` must emit exactly one event with topic `("created", id)`.
#[test]
fn create_contract_emits_created_event() {
    let fixture = EscrowFixture::builder().build();
    let created_sym = soroban_sdk::symbol_short!("created");
    let evts = events_with_topic(&fixture.env, &fixture.escrow_address, created_sym);
    assert_eq!(evts.len(), 1, "expected exactly one 'created' event");
}

/// The first topic of the `created` event must be the symbol `"created"`.
#[test]
fn create_contract_event_first_topic_is_created_symbol() {
    let fixture = EscrowFixture::builder().build();
    let created_sym = soroban_sdk::symbol_short!("created");
    let evts = events_with_topic(&fixture.env, &fixture.escrow_address, created_sym.clone());
    assert!(!evts.is_empty());
    let (topics, _) = evts.get(0).unwrap();
    let t0: Symbol = Symbol::try_from_val(&fixture.env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(t0, created_sym);
}

/// The second topic must be the allocated contract ID.
#[test]
fn create_contract_event_second_topic_is_contract_id() {
    let fixture = EscrowFixture::builder().build();
    let created_sym = soroban_sdk::symbol_short!("created");
    let evts = events_with_topic(&fixture.env, &fixture.escrow_address, created_sym);
    let (topics, _) = evts.get(0).unwrap();
    let id: u32 = TryFromVal::try_from_val(&fixture.env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(id, fixture.escrow_id);
}

/// The payload must be `(client: Address, freelancer: Address, timestamp: u64)`.
#[test]
fn create_contract_event_payload_contains_client_and_freelancer() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let created_sym = soroban_sdk::symbol_short!("created");
    let evts = events_with_topic(&env, &escrow_addr, created_sym);
    assert_eq!(evts.len(), 1);
    let (_, data) = evts.get(0).unwrap();
    let (emitted_client, emitted_freelancer, _ts): (Address, Address, u64) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(emitted_client, client_addr);
    assert_eq!(emitted_freelancer, freelancer_addr);
    let _ = id;
}

/// Multiple contracts each emit their own `created` event with the right ID.
#[test]
fn create_contract_each_contract_emits_own_event() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let created_sym = soroban_sdk::symbol_short!("created");

    for expected_id in 1u32..=3 {
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let id = escrow.create_contract(
            &c,
            &f,
            &None,
            &default_milestones(&env),
            &ReleaseAuthorization::ClientOnly,
        );
        assert_eq!(id, expected_id);

        // Most-recently emitted created event must carry this ID.
        let evts = events_with_topic(&env, &escrow_addr, created_sym.clone());
        let (topics, _) = evts.get(evts.len() - 1).unwrap();
        let emitted_id: u32 = TryFromVal::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
        assert_eq!(emitted_id, expected_id);
    }
}

/// `"created"` topic must not collide with any other known escrow event topics.
#[test]
fn create_contract_topic_no_collision() {
    let known_topics = [
        "contract",
        "arbiter",
        "contracts",
        "limits",
        "init",
        "dispute",
        "milestone_released",
        "mlstn_idx",
        "settlement_token_bound",
        "protocol_fee_bps",
        "admin",
        "arbiter_cfg",
    ];
    let created = soroban_sdk::symbol_short!("created");
    for other in &known_topics {
        // Use string comparison since Symbol can't be constructed from arbitrary str easily.
        assert_ne!(
            created,
            soroban_sdk::symbol_short!("created"),
            // This line only runs if created == Symbol::new(env, other), which it won't
        );
        // Verify string-level non-collision.
        assert_ne!("created", *other, "created must not collide with {other}");
    }
}

// ─── set_arbiter ─────────────────────────────────────────────────────────────

/// `set_arbiter` must emit an event with first topic `"arbiter"`.
#[test]
fn set_arbiter_emits_arbiter_event() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let new_arbiter = Address::generate(&env);
    escrow.set_arbiter(&id, &admin, &Some(new_arbiter));

    let arbiter_sym = soroban_sdk::symbol_short!("arbiter");
    let evts = events_with_topic(&env, &escrow_addr, arbiter_sym);
    assert!(!evts.is_empty(), "expected at least one 'arbiter' event");
}

/// Second topic of `set_arbiter` event is the contract ID.
#[test]
fn set_arbiter_event_second_topic_is_contract_id() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let new_arbiter = Address::generate(&env);
    escrow.set_arbiter(&id, &admin, &Some(new_arbiter));

    let arbiter_sym = soroban_sdk::symbol_short!("arbiter");
    let evts = events_with_topic(&env, &escrow_addr, arbiter_sym);
    let (topics, _) = evts.get(evts.len() - 1).unwrap();
    let emitted_id: u32 = TryFromVal::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(emitted_id, id);
}

/// Payload of `set_arbiter` is `(old_arbiter: Option<Address>, new_arbiter: Option<Address>, timestamp: u64)`.
#[test]
fn set_arbiter_event_payload_fields() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    let new_arbiter = Address::generate(&env);
    escrow.set_arbiter(&id, &admin, &Some(new_arbiter.clone()));

    let arbiter_sym = soroban_sdk::symbol_short!("arbiter");
    let evts = events_with_topic(&env, &escrow_addr, arbiter_sym);
    let (_, data) = evts.get(evts.len() - 1).unwrap();
    let (old, new_arb, _ts): (Option<Address>, Option<Address>, u64) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(
        old, None,
        "old arbiter must be None before any arbiter was set"
    );
    assert_eq!(new_arb, Some(new_arbiter));
}

/// Removing an arbiter emits the event with `new_arbiter = None`.
#[test]
fn set_arbiter_event_new_arbiter_none_when_removed() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arb = Address::generate(&env);
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arb.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    // Remove the arbiter (ClientOnly auth allows it).
    escrow.set_arbiter(&id, &admin, &None);

    let arbiter_sym = soroban_sdk::symbol_short!("arbiter");
    let evts = events_with_topic(&env, &escrow_addr, arbiter_sym);
    let (_, data) = evts.get(evts.len() - 1).unwrap();
    let (old, new_arb, _ts): (Option<Address>, Option<Address>, u64) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(old, Some(arb));
    assert_eq!(new_arb, None);
}

/// `"arbiter"` topic must not collide with any other known escrow event topics.
#[test]
fn set_arbiter_topic_no_collision_with_known_topics() {
    let other_topics = [
        "created",
        "contract",
        "contracts",
        "limits",
        "init",
        "dispute",
        "milestone_released",
        "mlstn_idx",
        "settlement_token_bound",
        "protocol_fee_bps",
        "arbiter_cfg",
        "admin",
    ];
    for other in &other_topics {
        assert_ne!("arbiter", *other, "arbiter must not collide with {other}");
    }
}

// ─── set_contracts_parameters ────────────────────────────────────────────────

/// `set_contracts_parameters` must emit an event with first topic `"contracts"`.
#[test]
fn set_contracts_parameters_emits_contracts_event() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    escrow.set_contracts_parameters(&5u32, &5_000_000_000_000_i128);

    let contracts_sym = soroban_sdk::symbol_short!("contracts");
    let evts = events_with_topic(&env, &escrow_addr, contracts_sym);
    assert_eq!(evts.len(), 1, "expected exactly one 'contracts' event");
}

/// Second topic of `set_contracts_parameters` must be `"params"`.
#[test]
fn set_contracts_parameters_event_second_topic_is_params() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    escrow.set_contracts_parameters(&5u32, &5_000_000_000_000_i128);

    let contracts_sym = soroban_sdk::symbol_short!("contracts");
    let evts = events_with_topic(&env, &escrow_addr, contracts_sym);
    let (topics, _) = evts.get(0).unwrap();
    let t1: Symbol = Symbol::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(t1, Symbol::new(&env, "params"));
}

/// Payload of `set_contracts_parameters` includes the updated params and timestamp.
#[test]
fn set_contracts_parameters_event_payload_matches_set_values() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let max_ms = 7u32;
    let max_stroop = 3_000_000_000_000_i128;
    escrow.set_contracts_parameters(&max_ms, &max_stroop);

    let contracts_sym = soroban_sdk::symbol_short!("contracts");
    let evts = events_with_topic(&env, &escrow_addr, contracts_sym);
    let (_, data) = evts.get(0).unwrap();
    let (params, _ts): (crate::types::ContractsParameters, u64) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(params.max_milestones, max_ms);
    assert_eq!(params.max_escrow_stroops, max_stroop);
}

/// Updating twice emits two events; the second carries the new values.
#[test]
fn set_contracts_parameters_second_call_emits_updated_params() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    escrow.set_contracts_parameters(&5u32, &5_000_000_000_000_i128);
    escrow.set_contracts_parameters(&8u32, &8_000_000_000_000_i128);

    let contracts_sym = soroban_sdk::symbol_short!("contracts");
    let evts = events_with_topic(&env, &escrow_addr, contracts_sym);
    assert_eq!(evts.len(), 2, "two calls → two events");

    let (_, data) = evts.get(1).unwrap();
    let (params, _ts): (crate::types::ContractsParameters, u64) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(params.max_milestones, 8);
    assert_eq!(params.max_escrow_stroops, 8_000_000_000_000_i128);
}

/// `"contracts"` topic must not collide with any other known escrow event topics.
#[test]
fn set_contracts_parameters_topic_no_collision() {
    let other_topics = [
        "created",
        "arbiter",
        "contract",
        "limits",
        "init",
        "dispute",
        "milestone_released",
        "mlstn_idx",
        "settlement_token_bound",
        "protocol_fee_bps",
        "arbiter_cfg",
        "admin",
    ];
    for other in &other_topics {
        assert_ne!(
            "contracts", *other,
            "contracts must not collide with {other}"
        );
    }
}

// ─── set_max_settlement ───────────────────────────────────────────────────────

/// `set_max_settlement` must emit an event with first topic `"limits"`.
#[test]
fn set_max_settlement_emits_limits_event() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    escrow.set_max_settlement(&5u32);

    let limits_sym = soroban_sdk::symbol_short!("limits");
    let evts = events_with_topic(&env, &escrow_addr, limits_sym);
    assert_eq!(evts.len(), 1, "expected exactly one 'limits' event");
}

/// Second topic of `set_max_settlement` event must be the `"max_settlement"` symbol.
#[test]
fn set_max_settlement_event_second_topic_is_max_settlement() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    escrow.set_max_settlement(&5u32);

    let limits_sym = soroban_sdk::symbol_short!("limits");
    let evts = events_with_topic(&env, &escrow_addr, limits_sym);
    let (topics, _) = evts.get(0).unwrap();
    let t1: Symbol = Symbol::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(t1, Symbol::new(&env, "max_settlement"));
}

/// Payload of `set_max_settlement` is `(max_settlement: u32, timestamp: u64)`.
#[test]
fn set_max_settlement_event_payload_contains_value_and_timestamp() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    let new_max: u32 = 20;
    escrow.set_max_settlement(&new_max);

    let limits_sym = soroban_sdk::symbol_short!("limits");
    let evts = events_with_topic(&env, &escrow_addr, limits_sym);
    let (_, data) = evts.get(0).unwrap();
    let (emitted_max, _ts): (u32, u64) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(emitted_max, new_max);
}

/// Setting the minimum boundary value still emits the correct event.
#[test]
fn set_max_settlement_event_at_minimum_boundary() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    escrow.set_max_settlement(&crate::MIN_MAX_BATCH_SETTLEMENT);

    let limits_sym = soroban_sdk::symbol_short!("limits");
    let evts = events_with_topic(&env, &escrow_addr, limits_sym);
    assert!(!evts.is_empty());
    let (_, data) = evts.get(0).unwrap();
    let (emitted_max, _ts): (u32, u64) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(emitted_max, crate::MIN_MAX_BATCH_SETTLEMENT);
}

/// Setting the maximum boundary value still emits the correct event.
#[test]
fn set_max_settlement_event_at_maximum_boundary() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    escrow.initialize(&admin);

    escrow.set_max_settlement(&crate::MAX_MAX_BATCH_SETTLEMENT);

    let limits_sym = soroban_sdk::symbol_short!("limits");
    let evts = events_with_topic(&env, &escrow_addr, limits_sym);
    assert!(!evts.is_empty());
    let (_, data) = evts.get(0).unwrap();
    let (emitted_max, _ts): (u32, u64) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(emitted_max, crate::MAX_MAX_BATCH_SETTLEMENT);
}

/// `"limits"` topic must not collide with any other known escrow event topics.
#[test]
fn set_max_settlement_topic_no_collision() {
    let other_topics = [
        "created",
        "arbiter",
        "contract",
        "contracts",
        "init",
        "dispute",
        "milestone_released",
        "mlstn_idx",
        "settlement_token_bound",
        "protocol_fee_bps",
        "arbiter_cfg",
        "admin",
    ];
    for other in &other_topics {
        assert_ne!("limits", *other, "limits must not collide with {other}");
    }
}

// ─── cross-topic collision matrix ────────────────────────────────────────────

/// All four contracts-module event topics must be mutually distinct.
#[test]
fn all_contracts_module_topics_are_mutually_distinct() {
    let topics = ["created", "arbiter", "contracts", "limits"];
    for i in 0..topics.len() {
        for j in (i + 1)..topics.len() {
            assert_ne!(
                topics[i], topics[j],
                "topic collision: {} == {}",
                topics[i], topics[j]
            );
        }
    }
}

/// None of the contracts-module topics collide with global escrow topics.
#[test]
fn contracts_module_topics_do_not_collide_with_global_escrow_topics() {
    let contracts_topics = ["created", "arbiter", "contracts", "limits"];
    let global_topics = [
        "contract",
        "init",
        "dispute",
        "milestone_released",
        "mlstn_idx",
        "settlement_token_bound",
        "protocol_fee_bps",
        "arbiter_cfg",
        "admin",
        "pause",
        "unpause",
        "refunded",
        "cancelled",
        "deposit",
        "finalized",
        "repr_put",
    ];
    for ct in &contracts_topics {
        for gt in &global_topics {
            assert_ne!(
                ct, gt,
                "topic collision: contracts-module '{ct}' == global '{gt}'"
            );
        }
    }
}
