//! Tests for issue #1712: versioned structured events.
//!
//! Each test asserts the exact topic tuple `(EVENT_SCHEMA_V, symbol, id)` and the
//! full payload struct via `env.events().all()`.

use crate::{
    CancelEvent, CompletedEvent, ContractStatus, CreatedEvent, DepositEvent, DisputeEvent,
    EmergencyActivatedEvent, EmergencyResolvedEvent, EvidenceEvent, InitEvent, PausedEvent,
    RefundedEvent, ReleaseEvent, ReputationEvent, ResolvedEvent, SettlementTokenBoundEvent,
    UnpausedEvent, EVENT_SCHEMA_V,
};
use crate::{Escrow, EscrowClient, ReleaseAuthorization};
use soroban_sdk::{
    testutils::{Address as _, Events},
    token::StellarAssetClient,
    vec, Address, Env, Symbol, TryFromVal, Val,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup() -> (Env, EscrowClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &addr);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

fn bind_token(env: &Env, client: &EscrowClient, admin: &Address) -> Address {
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(admin, &token);
    token
}

fn make_contract(env: &Env, client: &EscrowClient) -> (u32, Address, Address) {
    let c = Address::generate(env);
    let f = Address::generate(env);
    let id = client.create_contract(
        &c,
        &f,
        &None,
        &vec![env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    (id, c, f)
}

fn fund(env: &Env, client: &EscrowClient, admin: &Address, id: u32, c: &Address, total: i128) -> Address {
    let token = bind_token(env, client, admin);
    StellarAssetClient::new(env, &token).mint(c, &total);
    client.deposit_funds(&id, c, &total);
    token
}

/// Get the last event from env.events().all() as (topics, data).
fn last_event(env: &Env) -> (soroban_sdk::Vec<Val>, Val) {
    let all = env.events().all();
    let n = all.len();
    assert!(n > 0, "no events emitted");
    let (_, topics, data) = all.get(n - 1).unwrap();
    (topics, data)
}

/// Get the nth-from-last event (0 = last).
fn nth_last_event(env: &Env, n_from_last: u32) -> (soroban_sdk::Vec<Val>, Val) {
    let all = env.events().all();
    let len = all.len();
    let idx = len - 1 - n_from_last;
    let (_, topics, data) = all.get(idx).unwrap();
    (topics, data)
}

fn assert_topics_escrow(env: &Env, topics: &soroban_sdk::Vec<Val>, symbol_name: &str, contract_id: u32) {
    let v: u32 = u32::try_from_val(env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(v, EVENT_SCHEMA_V, "schema version mismatch for {}", symbol_name);
    let sym = Symbol::try_from_val(env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(sym, Symbol::new(env, symbol_name), "symbol mismatch");
    let id: u32 = u32::try_from_val(env, &topics.get(2).unwrap()).unwrap();
    assert_eq!(id, contract_id, "contract_id mismatch for {}", symbol_name);
}

fn assert_topics_global(env: &Env, topics: &soroban_sdk::Vec<Val>, symbol_name: &str) {
    let v: u32 = u32::try_from_val(env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(v, EVENT_SCHEMA_V, "schema version mismatch for {}", symbol_name);
    let sym = Symbol::try_from_val(env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(sym, Symbol::new(env, symbol_name), "symbol mismatch");
}

// ── init ─────────────────────────────────────────────────────────────────────

#[test]
fn init_event_topics_and_payload() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &addr);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (topics, data) = last_event(&env);
    assert_topics_global(&env, &topics, "init");
    let payload = InitEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.admin, admin);
}

// ── settlement_token_bound ────────────────────────────────────────────────────

#[test]
fn settlement_token_bound_event_topics_and_payload() {
    let (env, client, admin) = setup();
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);

    let (topics, data) = last_event(&env);
    assert_topics_global(&env, &topics, "token_bound");
    let payload = SettlementTokenBoundEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.admin, admin);
    assert_eq!(payload.token, token);
}

// ── created ──────────────────────────────────────────────────────────────────

#[test]
fn created_event_topics_and_payload() {
    let (env, client, _admin) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let id = client.create_contract(
        &c,
        &f,
        &None,
        &vec![&env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    let (topics, data) = last_event(&env);
    assert_topics_escrow(&env, &topics, "created", id);
    let payload = CreatedEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.client, c);
    assert_eq!(payload.freelancer, f);
    assert_eq!(payload.total, 300_i128);
    assert_eq!(payload.milestone_count, 2);
}

// ── deposit ──────────────────────────────────────────────────────────────────

#[test]
fn deposit_event_topics_and_payload() {
    let (env, client, admin) = setup();
    let (id, c, _f) = make_contract(&env, &client);
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);
    StellarAssetClient::new(&env, &token).mint(&c, &300_i128);

    client.deposit_funds(&id, &c, &300_i128);

    let (topics, data) = last_event(&env);
    assert_topics_escrow(&env, &topics, "deposit", id);
    let payload = DepositEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.from, c);
    assert_eq!(payload.amount, 300_i128);
    assert_eq!(payload.funded_amount, 300_i128);
    assert_eq!(payload.status, ContractStatus::Funded);
}

// ── release ───────────────────────────────────────────────────────────────────

#[test]
fn release_event_topics_and_payload() {
    let (env, client, admin) = setup();
    let (id, c, f) = make_contract(&env, &client);
    fund(&env, &client, &admin, id, &c, 300_i128);

    client.approve_milestone_release(&id, &c, &0);
    client.release_milestone(&id, &c, &0);

    let (topics, data) = last_event(&env);
    assert_topics_escrow(&env, &topics, "mlstn_rls", id);
    let payload = ReleaseEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.index, 0);
    assert_eq!(payload.amount, 100_i128);
    assert_eq!(payload.to, f);
    assert_eq!(payload.status, ContractStatus::Funded);
}

// ── complete event emitted exactly once on last release ───────────────────────

#[test]
fn complete_event_emitted_on_last_release() {
    let (env, client, admin) = setup();
    let (id, c, _f) = make_contract(&env, &client);
    fund(&env, &client, &admin, id, &c, 300_i128);

    // Release first milestone — complete must NOT appear.
    client.approve_milestone_release(&id, &c, &0);
    client.release_milestone(&id, &c, &0);

    // Verify no complete event yet.
    let all_after_first = env.events().all();
    let complete_sym = Symbol::new(&env, "complete");
    let has_complete_early = all_after_first.iter().any(|(_, topics, _)| {
        Symbol::try_from_val(&env, &topics.get(1).unwrap_or_default())
            .ok()
            .as_ref()
            == Some(&complete_sym)
    });
    assert!(!has_complete_early, "complete must not fire before last milestone");

    // Approve and release the last (second) milestone.
    client.approve_milestone_release(&id, &c, &1);
    client.release_milestone(&id, &c, &1);

    // Now complete MUST appear.
    let all = env.events().all();
    let complete_sym = Symbol::new(&env, "complete");
    let mut complete_count = 0u32;
    let mut complete_topics: Option<soroban_sdk::Vec<Val>> = None;
    let mut complete_data: Option<Val> = None;
    for (_, topics, data) in all.iter() {
        if Symbol::try_from_val(&env, &topics.get(1).unwrap_or_default())
            .ok()
            .as_ref()
            == Some(&complete_sym)
        {
            complete_count += 1;
            complete_topics = Some(topics);
            complete_data = Some(data);
        }
    }
    assert_eq!(complete_count, 1, "complete must fire exactly once");

    // The complete event must have correct schema version and contract_id.
    let topics_c = complete_topics.unwrap();
    let data_c = complete_data.unwrap();
    let v: u32 = u32::try_from_val(&env, &topics_c.get(0).unwrap()).unwrap();
    assert_eq!(v, EVENT_SCHEMA_V);
    let id_c: u32 = u32::try_from_val(&env, &topics_c.get(2).unwrap()).unwrap();
    assert_eq!(id_c, id);
    let payload = CompletedEvent::try_from_val(&env, &data_c).unwrap();
    assert_eq!(payload.released_amount, 300_i128);
}

#[test]
fn complete_event_not_emitted_on_non_last_release() {
    let (env, client, admin) = setup();
    let (id, c, _f) = make_contract(&env, &client);
    fund(&env, &client, &admin, id, &c, 300_i128);

    client.approve_milestone_release(&id, &c, &0);
    client.release_milestone(&id, &c, &0);

    let all = env.events().all();
    let complete_sym = Symbol::new(&env, "complete");
    let has_complete = all.iter().any(|(_, topics, _)| {
        Symbol::try_from_val(&env, &topics.get(1).unwrap_or_default())
            .ok()
            .as_ref()
            == Some(&complete_sym)
    });
    assert!(!has_complete, "complete must not fire before last milestone");
}

// ── cancel ───────────────────────────────────────────────────────────────────

#[test]
fn cancel_event_topics_and_payload() {
    let (env, client, _admin) = setup();
    let (id, c, _f) = make_contract(&env, &client);

    client.cancel_contract(&id, &c);

    let (topics, data) = last_event(&env);
    assert_topics_escrow(&env, &topics, "cancelled", id);
    let payload = CancelEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.to, c);
    assert_eq!(payload.refund, 0_i128);
    assert_eq!(payload.status, ContractStatus::Cancelled);
}

// ── refunded ─────────────────────────────────────────────────────────────────

#[test]
fn refunded_event_topics_and_payload() {
    let (env, client, admin) = setup();
    let (id, c, _f) = make_contract(&env, &client);
    fund(&env, &client, &admin, id, &c, 300_i128);

    client.refund_unreleased_milestones(&id, &vec![&env, 0u32, 1u32]);

    let (topics, data) = last_event(&env);
    assert_topics_escrow(&env, &topics, "refunded", id);
    let payload = RefundedEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.amount, 300_i128);
    assert_eq!(payload.status, ContractStatus::Refunded);
}

// ── dispute opened ────────────────────────────────────────────────────────────

#[test]
fn dispute_opened_event_topics_and_payload() {
    let (env, client, admin) = setup();
    let arbiter = Address::generate(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let id = client.create_contract(
        &c,
        &f,
        &Some(arbiter.clone()),
        &vec![&env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);
    StellarAssetClient::new(&env, &token).mint(&c, &300_i128);
    client.deposit_funds(&id, &c, &300_i128);

    client.raise_dispute(&id, &c);

    let (topics, data) = last_event(&env);
    assert_topics_escrow(&env, &topics, "dispute_opened", id);
    let payload = DisputeEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.by, c);
    assert_eq!(payload.status, ContractStatus::Disputed);
}

// ── dispute resolved ──────────────────────────────────────────────────────────

#[test]
fn dispute_resolved_event_topics_and_payload() {
    use crate::DisputeResolution;

    let (env, client, admin) = setup();
    let arbiter = Address::generate(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let id = client.create_contract(
        &c,
        &f,
        &Some(arbiter.clone()),
        &vec![&env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);
    StellarAssetClient::new(&env, &token).mint(&c, &300_i128);
    client.deposit_funds(&id, &c, &300_i128);
    client.raise_dispute(&id, &c);

    client.resolve_dispute(&id, &arbiter, &DisputeResolution::FullRefund);

    let (topics, data) = last_event(&env);
    assert_topics_escrow(&env, &topics, "dispute_resolved", id);
    let payload = ResolvedEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.to_client, 300_i128);
    assert_eq!(payload.to_freelancer, 0_i128);
}

// ── reputation ────────────────────────────────────────────────────────────────

#[test]
fn reputation_event_topics_and_payload() {
    let (env, client, admin) = setup();
    let (id, c, f) = make_contract(&env, &client);
    fund(&env, &client, &admin, id, &c, 300_i128);
    client.approve_milestone_release(&id, &c, &0);
    client.release_milestone(&id, &c, &0);
    client.approve_milestone_release(&id, &c, &1);
    client.release_milestone(&id, &c, &1);

    client.issue_reputation(
        &id,
        &c,
        &5u32,
        &soroban_sdk::String::from_str(&env, "great"),
    );

    let (topics, data) = last_event(&env);
    assert_topics_escrow(&env, &topics, "reputation", id);
    let payload = ReputationEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.freelancer, f);
    assert_eq!(payload.rating, 5);
    assert_eq!(payload.completed_contracts, 1);
    assert_eq!(payload.total_rating, 5);
}

// ── evidence ─────────────────────────────────────────────────────────────────

#[test]
fn evidence_event_topics_and_payload() {
    let (env, client, admin) = setup();
    let (id, c, f) = make_contract(&env, &client);
    fund(&env, &client, &admin, id, &c, 300_i128);

    client.submit_work_evidence(
        &id,
        &f,
        &0u32,
        &soroban_sdk::String::from_str(&env, "ipfs://abc"),
    );

    let (topics, data) = last_event(&env);
    assert_topics_escrow(&env, &topics, "evidence", id);
    let payload = EvidenceEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.milestone_index, 0);
    assert_eq!(payload.freelancer, f);
}

// ── pause / unpause ───────────────────────────────────────────────────────────

#[test]
fn paused_event_topics_and_payload() {
    let (env, client, admin) = setup();
    client.pause();

    let (topics, data) = last_event(&env);
    assert_topics_global(&env, &topics, "paused");
    let payload = PausedEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.admin, admin);
}

#[test]
fn unpaused_event_topics_and_payload() {
    let (env, client, admin) = setup();
    client.pause();
    client.unpause();

    let (topics, data) = last_event(&env);
    assert_topics_global(&env, &topics, "unpaused");
    let payload = UnpausedEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.admin, admin);
}

// ── emergency ────────────────────────────────────────────────────────────────

#[test]
fn emergency_activated_event_topics_and_payload() {
    let (env, client, admin) = setup();
    client.activate_emergency_pause();

    let (topics, data) = last_event(&env);
    assert_topics_global(&env, &topics, "emergency_on");
    let payload = EmergencyActivatedEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.admin, admin);
}

#[test]
fn emergency_resolved_event_topics_and_payload() {
    let (env, client, admin) = setup();
    client.activate_emergency_pause();
    client.resolve_emergency();

    let (topics, data) = last_event(&env);
    assert_topics_global(&env, &topics, "emergency_off");
    let payload = EmergencyResolvedEvent::try_from_val(&env, &data).unwrap();
    assert_eq!(payload.admin, admin);
}
