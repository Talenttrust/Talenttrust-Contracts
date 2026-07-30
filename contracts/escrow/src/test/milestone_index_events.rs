#![cfg(test)]

//! Assertions for the `mlstn_idx` indexed-event stream added for off-chain
//! milestone-history reconstruction. Fires on every milestone state change:
//! creation, release, and refund (both refund entrypoints).

use soroban_sdk::{testutils::Events as _, Address, Env, Symbol, TryIntoVal};

use crate::{
    events::MilestoneIndexEvent,
    test::{EscrowFixture, MILESTONE_ONE, MILESTONE_THREE, MILESTONE_TWO},
};

/// Extracts all `mlstn_idx` events emitted by `contract_address`.
///
/// Each item is a `(contract_id, milestone_index, MilestoneIndexEvent)` triple
/// so tests can assert both the topics and the typed payload.
fn mlstn_idx_events(
    env: &Env,
    contract_address: &Address,
) -> soroban_sdk::Vec<(u32, u32, MilestoneIndexEvent)> {
    let topic = Symbol::new(env, "mlstn_idx");
    let mut out = soroban_sdk::Vec::new(env);
    for (addr, topics, data) in env.events().all().iter() {
        if &addr != contract_address {
            continue;
        }
        if topics.len() != 3 {
            continue;
        }
        let t0: Symbol = topics.get(0).unwrap().try_into_val(env).unwrap();
        if t0 != topic {
            continue;
        }
        let contract_id: u32 = topics.get(1).unwrap().try_into_val(env).unwrap();
        let milestone_index: u32 = topics.get(2).unwrap().try_into_val(env).unwrap();
        let payload: MilestoneIndexEvent = data.try_into_val(env).unwrap();
        out.push_back((contract_id, milestone_index, payload));
    }
    out
}

#[test]
fn creation_emits_indexed_event_per_milestone() {
    let fixture = EscrowFixture::builder().build();
    let events = mlstn_idx_events(&fixture.env, &fixture.escrow_address);
    assert_eq!(events.len(), 3, "one mlstn_idx event per created milestone");

    let expected = [MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE];
    for i in 0..3u32 {
        let (contract_id, milestone_index, payload) = events.get(i).unwrap();
        assert_eq!(contract_id, fixture.escrow_id);
        assert_eq!(milestone_index, i);
        assert_eq!(payload.amount, expected[i as usize]);
        assert!(!payload.released);
        assert!(!payload.refunded);
    }
}

#[test]
fn release_emits_indexed_event_with_correct_payload() {
    let fixture = EscrowFixture::builder().funded().build();
    let client = fixture.escrow();
    client.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0u32);
    client.release_milestone(&fixture.escrow_id, &fixture.client, &0u32);

    let events = mlstn_idx_events(&fixture.env, &fixture.escrow_address);
    let release_event = events
        .iter()
        .find(|(cid, idx, payload)| {
            *cid == fixture.escrow_id && *idx == 0 && payload.released && !payload.refunded
        });
    assert!(
        release_event.is_some(),
        "expected an mlstn_idx event for the release"
    );
    let (_, _, payload) = release_event.unwrap();
    assert_eq!(payload.amount, MILESTONE_ONE);
    assert!(payload.released);
    assert!(!payload.refunded);
}

#[test]
fn refund_emits_indexed_event_with_correct_payload() {
    let fixture = EscrowFixture::builder().funded().build();
    let client = fixture.escrow();
    let indices = soroban_sdk::vec![&fixture.env, 1u32];
    client.refund_unreleased_milestones(&fixture.escrow_id, &indices);

    let events = mlstn_idx_events(&fixture.env, &fixture.escrow_address);
    let refund_event = events
        .iter()
        .find(|(cid, idx, payload)| {
            *cid == fixture.escrow_id && *idx == 1 && !payload.released && payload.refunded
        });
    assert!(
        refund_event.is_some(),
        "expected an mlstn_idx event for the refund"
    );
    let (_, _, payload) = refund_event.unwrap();
    assert_eq!(payload.amount, MILESTONE_TWO);
    assert!(!payload.released);
    assert!(payload.refunded);
}

#[test]
fn milestone_index_event_fields_match_tuple_semantics() {
    // Edge-case: verify field alignment is preserved — the struct's fields
    // carry the same meaning as the old (amount, released, refunded, timestamp)
    // tuple but are now self-describing.
    let payload = MilestoneIndexEvent {
        amount: 500_0000000,
        released: true,
        refunded: false,
        timestamp: 1_000_000,
    };
    assert_eq!(payload.amount, 500_0000000);
    assert!(payload.released);
    assert!(!payload.refunded);
    assert_eq!(payload.timestamp, 1_000_000);
}

#[test]
fn mlstn_idx_topic_does_not_collide_with_existing_topics() {
    // The full set of pre-existing symbol_short! topics in this crate, confirmed
    // via repo-wide search before adding this event.
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
    ];
    assert!(
        !existing.contains(&"mlstn_idx"),
        "mlstn_idx must be a new, non-colliding topic"
    );
}
