#![cfg(test)]

use super::{default_milestones, EscrowClient};
use soroban_sdk::testutils::{Address as _, Events, Ledger, LedgerInfo};
use soroban_sdk::{symbol_short, Address, Env, Symbol, TryFromVal, Val};

use crate::{Escrow, EscrowError, ReleaseAuthorization};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn setup_escrow_with_admin(env: &Env) -> (EscrowClient<'_>, Address) {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    (client, admin)
}

fn has_arbiter_topic(
    events: &soroban_sdk::Vec<(Address, soroban_sdk::Vec<Val>, Val)>,
    env: &Env,
) -> bool {
    let arbiter = symbol_short!("arbiter");
    events.iter().any(|event| {
        event.1.len() > 0
            && Symbol::try_from_val(env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&arbiter)
    })
}

fn decode_last_arbiter_event(
    events: &soroban_sdk::Vec<(Address, soroban_sdk::Vec<Val>, Val)>,
    env: &Env,
) -> (Option<Address>, Option<Address>, u64) {
    let arbiter = symbol_short!("arbiter");
    let event = events
        .iter()
        .rev()
        .find(|e| {
            e.1.len() > 0
                && Symbol::try_from_val(env, &e.1.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&arbiter)
        })
        .expect("no arbiter event found");

    let data: soroban_sdk::Vec<Val> = soroban_sdk::TryFromVal::try_from_val(env, &event.2).unwrap();
    assert_eq!(data.len(), 3, "arbiter event data should have 3 fields");

    let old: Option<Address> =
        soroban_sdk::TryFromVal::try_from_val(env, &data.get(0).unwrap()).unwrap();
    let new: Option<Address> =
        soroban_sdk::TryFromVal::try_from_val(env, &data.get(1).unwrap()).unwrap();
    let ts: u64 = soroban_sdk::TryFromVal::try_from_val(env, &data.get(2).unwrap()).unwrap();

    (old, new, ts)
}

fn last_arbiter_event_contract_id(
    events: &soroban_sdk::Vec<(Address, soroban_sdk::Vec<Val>, Val)>,
    env: &Env,
) -> u32 {
    let arbiter = symbol_short!("arbiter");
    let event = events
        .iter()
        .rev()
        .find(|e| {
            e.1.len() > 0
                && Symbol::try_from_val(env, &e.1.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&arbiter)
        })
        .expect("no arbiter event found");

    soroban_sdk::TryFromVal::try_from_val(env, &event.1.get(1).unwrap()).unwrap()
}

// ── Creation-time arbiter event ──────────────────────────────────────────────

#[test]
fn creation_with_arbiter_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_escrow_with_admin(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let all_events = env.events().all();

    assert!(
        has_arbiter_topic(&all_events, &env),
        "expected an arbiter event after creation with arbiter"
    );

    assert_eq!(
        last_arbiter_event_contract_id(&all_events, &env),
        contract_id
    );

    let (old, new, _ts) = decode_last_arbiter_event(&all_events, &env);
    assert!(old.is_none(), "old_arbiter should be None at creation");
    assert_eq!(new, Some(arbiter_addr));
}

#[test]
fn creation_without_arbiter_no_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_escrow_with_admin(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let all_events = env.events().all();
    assert!(
        !has_arbiter_topic(&all_events, &env),
        "no arbiter event should be emitted when arbiter is None"
    );
}

// ── set_arbiter entrypoint ───────────────────────────────────────────────────

#[test]
fn set_arbiter_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_escrow_with_admin(&env);

    let initial = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: initial.sequence_number,
        timestamp: 1_700_000_000,
        protocol_version: initial.protocol_version,
        network_id: initial.network_id.clone(),
        base_reserve: initial.base_reserve,
        min_temp_entry_ttl: initial.min_temp_entry_ttl,
        min_persistent_entry_ttl: initial.min_persistent_entry_ttl,
        max_entry_ttl: initial.max_entry_ttl,
    });

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let new_arbiter = Address::generate(&env);
    assert!(client.set_arbiter(&contract_id, &admin, &Some(new_arbiter.clone()),));

    let all_events = env.events().all();

    assert!(
        has_arbiter_topic(&all_events, &env),
        "set_arbiter should emit an arbiter event; total events={}",
        all_events.len()
    );

    assert_eq!(
        last_arbiter_event_contract_id(&all_events, &env),
        contract_id
    );

    let (old, new, ts) = decode_last_arbiter_event(&all_events, &env);
    assert_eq!(old, Some(arbiter_addr));
    assert_eq!(new, Some(new_arbiter));
    assert!(ts > 0, "timestamp should be non-zero");
}

#[test]
fn set_arbiter_remove_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_escrow_with_admin(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.set_arbiter(&contract_id, &admin, &None));

    let all_events = env.events().all();
    let (old, new, _ts) = decode_last_arbiter_event(&all_events, &env);

    assert!(old.is_some(), "old_arbiter should be Some before removal");
    assert!(new.is_none(), "new_arbiter should be None after removal");
}

#[test]
fn set_arbiter_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_escrow_with_admin(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let attacker = Address::generate(&env);
    let result = client.try_set_arbiter(&contract_id, &attacker, &None);

    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn set_arbiter_not_found_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_escrow_with_admin(&env);

    let result = client.try_set_arbiter(&999u32, &admin, &None);

    super::assert_contract_error(result, EscrowError::ContractNotFound);
}

#[test]
fn set_arbiter_invalid_same_as_client_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_escrow_with_admin(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_set_arbiter(&contract_id, &admin, &Some(client_addr));

    super::assert_contract_error(result, EscrowError::InvalidArbiter);
}

#[test]
fn set_arbiter_invalid_same_as_freelancer_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_escrow_with_admin(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_set_arbiter(&contract_id, &admin, &Some(freelancer_addr));

    super::assert_contract_error(result, EscrowError::InvalidArbiter);
}

#[test]
fn set_arbiter_paused_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_escrow_with_admin(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    client.pause();

    let result = client.try_set_arbiter(&contract_id, &admin, &None);

    super::assert_contract_error(result, crate::Error::ContractPaused);
}

#[test]
fn set_arbiter_removing_from_arbiteronly_mode_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_escrow_with_admin(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    );

    let result = client.try_set_arbiter(&contract_id, &admin, &None);

    super::assert_contract_error(result, EscrowError::MissingArbiter);
}

#[test]
fn set_arbiter_removing_from_clientandarbiter_mode_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_escrow_with_admin(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );

    let result = client.try_set_arbiter(&contract_id, &admin, &None);

    super::assert_contract_error(result, EscrowError::MissingArbiter);
}

// ── Topic collision check ────────────────────────────────────────────────────

#[test]
fn arbiter_topic_does_not_collide_with_existing_topics() {
    let existing = [
        symbol_short!("init"),
        symbol_short!("admin"),
        symbol_short!("created"),
        symbol_short!("mlstn_rls"),
        symbol_short!("ctrct_cmp"),
        symbol_short!("ctrct_st"),
        symbol_short!("refunded"),
        symbol_short!("cancelled"),
        symbol_short!("pause"),
        symbol_short!("unpaused"),
        symbol_short!("limits"),
        symbol_short!("evidence"),
        symbol_short!("fee"),
        symbol_short!("withdraw"),
        symbol_short!("dispute"),
        symbol_short!("opened"),
        symbol_short!("resolved"),
        symbol_short!("finalized"),
        symbol_short!("arbiter"),
    ];

    let arbiter = symbol_short!("arbiter");
    let count = existing.iter().filter(|t| **t == arbiter).count();
    assert_eq!(
        count, 1,
        "symbol_short!(\"arbiter\") should appear exactly once in the exhaustive list"
    );

    for (i, t1) in existing.iter().enumerate() {
        for (j, t2) in existing.iter().enumerate() {
            if i != j {
                assert_ne!(
                    t1, t2,
                    "topic collision detected between index {} and index {}",
                    i, j
                );
            }
        }
    }
}
