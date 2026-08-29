#![cfg(test)]

//! Tests for the newly added events: `mlstn_app` (milestone approval) and
//! `rep_issd` (reputation issuance).

use soroban_sdk::String;
use soroban_sdk::{testutils::Events as _, Address, Env, Symbol, TryIntoVal, Val, Vec};

use crate::test::EscrowFixture;
use crate::ReleaseAuthorization;

/// Helper to collect all events with a given primary topic and contract ID.
/// Returns a vector of `(milestone_index, raw_payload)`.
fn events_with_topic_and_contract(
    env: &Env,
    contract_address: &Address,
    topic: Symbol,
    contract_id: u32,
) -> Vec<(u32, Val)> {
    let mut out = Vec::new(env);
    for (addr, topics, data) in env.events().all().iter() {
        if &addr != contract_address {
            continue;
        }
        if topics.len() < 2 {
            continue;
        }
        let t0: Symbol = topics.get(0).unwrap().try_into_val(env).unwrap();
        if t0 != topic {
            continue;
        }
        let cid: u32 = topics.get(1).unwrap().try_into_val(env).unwrap();
        if cid != contract_id {
            continue;
        }
        let milestone_index = if topics.len() >= 3 {
            topics.get(2).unwrap().try_into_val(env).unwrap()
        } else {
            0u32
        };
        out.push_back((milestone_index, data.clone()));
    }
    out
}

#[test]
fn approve_milestone_release_emits_mlstn_app_exactly_once() {
    let fixture = EscrowFixture::builder().funded().build();
    let contract_id = fixture.escrow_id;
    let milestone_index = 0u32;

    fixture
        .escrow()
        .approve_milestone_release(&contract_id, &fixture.client, &milestone_index);

    let topic = Symbol::new(&fixture.env, "mlstn_app");
    let events = events_with_topic_and_contract(
        &fixture.env,
        &fixture.escrow_address,
        topic.clone(),
        contract_id,
    );
    assert_eq!(events.len(), 1, "Expected exactly one mlstn_app event");

    let (idx, payload) = events.get(0).unwrap();
    assert_eq!(idx, milestone_index);
    let decoded: (u32, Address, u64) = payload.try_into_val(&fixture.env).unwrap();
    assert_eq!(decoded.0, milestone_index);
    assert_eq!(decoded.1, fixture.client);
    // Check that timestamp equals the ledger timestamp (which may be 0 in tests)
    assert_eq!(decoded.2, fixture.env.ledger().timestamp());
}

#[test]
fn approve_milestone_release_failure_does_not_emit() {
    let fixture = EscrowFixture::builder()
        .release_authorization(ReleaseAuthorization::ClientOnly)
        .funded()
        .build();
    let contract_id = fixture.escrow_id;
    let milestone_index = 0u32;

    let res = fixture.escrow().try_approve_milestone_release(
        &contract_id,
        &fixture.freelancer,
        &milestone_index,
    );
    assert!(res.is_err(), "Expected error for unauthorized approval");

    let topic = Symbol::new(&fixture.env, "mlstn_app");
    let events =
        events_with_topic_and_contract(&fixture.env, &fixture.escrow_address, topic, contract_id);
    assert_eq!(
        events.len(),
        0,
        "No approval event should be emitted on failure"
    );
}

#[test]
fn issue_reputation_emits_rep_issd_exactly_once() {
    let fixture = EscrowFixture::builder().completed().build();
    let contract_id = fixture.escrow_id;
    let rating = 5u32;
    let comment = String::from_str(&fixture.env, "Great work!");

    fixture
        .escrow()
        .issue_reputation(&contract_id, &fixture.client, &rating, &comment);

    let topic = Symbol::new(&fixture.env, "rep_issd");
    let events = events_with_topic_and_contract(
        &fixture.env,
        &fixture.escrow_address,
        topic.clone(),
        contract_id,
    );
    assert_eq!(events.len(), 1, "Expected exactly one rep_issd event");

    let (idx, payload) = events.get(0).unwrap();
    assert_eq!(idx, 0);
    let decoded: (Address, u32, u64) = payload.try_into_val(&fixture.env).unwrap();
    assert_eq!(decoded.0, fixture.freelancer);
    assert_eq!(decoded.1, rating);
    assert_eq!(decoded.2, fixture.env.ledger().timestamp());
}

#[test]
fn issue_reputation_failure_does_not_emit() {
    let fixture = EscrowFixture::builder().funded().build(); // not Completed
    let contract_id = fixture.escrow_id;
    let rating = 5u32;
    let comment = String::from_str(&fixture.env, "Good");

    let res =
        fixture
            .escrow()
            .try_issue_reputation(&contract_id, &fixture.client, &rating, &comment);
    assert!(
        res.is_err(),
        "Expected error because contract is not Completed"
    );

    let topic = Symbol::new(&fixture.env, "rep_issd");
    let events =
        events_with_topic_and_contract(&fixture.env, &fixture.escrow_address, topic, contract_id);
    assert_eq!(
        events.len(),
        0,
        "No reputation event should be emitted on failure"
    );
}

#[test]
fn read_only_calls_emit_no_events() {
    let fixture = EscrowFixture::builder().funded().build();
    let env = &fixture.env;
    let contract_id = fixture.escrow_id;

    fixture.escrow().get_contract(&contract_id);
    fixture.escrow().get_milestones(&contract_id);
    fixture.escrow().get_contract_summary(&contract_id);

    let app_topic = Symbol::new(env, "mlstn_app");
    let issd_topic = Symbol::new(env, "rep_issd");
    let app_events =
        events_with_topic_and_contract(env, &fixture.escrow_address, app_topic, contract_id);
    let issd_events =
        events_with_topic_and_contract(env, &fixture.escrow_address, issd_topic, contract_id);
    assert_eq!(
        app_events.len(),
        0,
        "mlstn_app should not be emitted on read-only calls"
    );
    assert_eq!(
        issd_events.len(),
        0,
        "rep_issd should not be emitted on read-only calls"
    );
}

#[test]
fn new_event_topics_do_not_collide() {
    let existing = [
        "admin",
        "cancelled",
        "created",
        "ctrct_cmp",
        "dispute",
        "evidence",
        "fee",
        "finalized",
        "init",
        "mlstn_rls",
        "opened",
        "refunded",
        "resolved",
        "unpaused",
        "withdraw",
        "pause",
        "mlstn_idx",
        "settlement_token_bound",
        "arbiter_cfg",
        "limits",
        "rep_cfg",
    ];
    let new_topics = ["mlstn_app", "rep_issd"];
    for nt in new_topics {
        assert!(
            !existing.contains(&nt),
            "New topic '{}' collides with an existing one",
            nt
        );
    }
}
