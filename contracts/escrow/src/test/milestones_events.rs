#![cfg(test)]

use super::EscrowFixture;
use crate::ContractStatus;
use soroban_sdk::{symbol_short, testutils::Events, Address, String, Symbol, TryFromVal};

fn latest_event(
    fixture: &EscrowFixture,
) -> (
    Address,
    soroban_sdk::Vec<soroban_sdk::Val>,
    soroban_sdk::Val,
) {
    fixture
        .env
        .events()
        .all()
        .iter()
        .last()
        .expect("the emitting call must publish an event")
}

fn assert_topic(
    fixture: &EscrowFixture,
    event: &(
        Address,
        soroban_sdk::Vec<soroban_sdk::Val>,
        soroban_sdk::Val,
    ),
    expected: Symbol,
) {
    assert_eq!(event.0, fixture.escrow_address);
    assert_eq!(event.1.len(), 2, "milestone events have two topics");
    assert_eq!(
        Symbol::try_from_val(&fixture.env, &event.1.get(0).unwrap()).unwrap(),
        expected
    );
    assert_eq!(
        u32::try_from_val(&fixture.env, &event.1.get(1).unwrap()).unwrap(),
        fixture.escrow_id
    );
}

#[test]
fn release_event_has_expected_topic_and_payload() {
    let fixture = EscrowFixture::builder().funded().build();
    let client = fixture.escrow();
    let milestone_index = 1_u32;
    let timestamp = fixture.env.ledger().timestamp();

    client.approve_milestone_release(&fixture.escrow_id, &fixture.client, &milestone_index);
    client.release_milestone(&fixture.escrow_id, &fixture.client, &milestone_index);

    let event = latest_event(&fixture);
    assert_topic(&fixture, &event, symbol_short!("mlstn_rls"));
    let payload: (u32, i128, i128, i128, Address, u64) =
        TryFromVal::try_from_val(&fixture.env, &event.2).unwrap();
    assert_eq!(
        payload,
        (
            milestone_index,
            400_0000000_i128,
            0_i128,
            400_0000000_i128,
            fixture.client.clone(),
            timestamp,
        )
    );
}

#[test]
fn refund_event_has_expected_topic_and_payload() {
    let fixture = EscrowFixture::builder().funded().build();
    let client = fixture.escrow();
    let indices = soroban_sdk::vec![&fixture.env, 0_u32, 2_u32];
    let timestamp = fixture.env.ledger().timestamp();

    client.refund_unreleased_milestones(&fixture.escrow_id, &indices);

    let event = latest_event(&fixture);
    assert_topic(&fixture, &event, symbol_short!("refunded"));
    let payload: (i128, ContractStatus, u64) =
        TryFromVal::try_from_val(&fixture.env, &event.2).unwrap();
    assert_eq!(
        payload,
        (800_0000000_i128, ContractStatus::Funded, timestamp)
    );
}

#[test]
fn evidence_event_has_expected_topic_and_payload() {
    let fixture = EscrowFixture::builder().funded().build();
    let client = fixture.escrow();
    let milestone_index = 2_u32;
    let evidence = String::from_str(&fixture.env, "ipfs://milestone-2");
    let timestamp = fixture.env.ledger().timestamp();

    client.submit_work_evidence(
        &fixture.escrow_id,
        &fixture.freelancer,
        &milestone_index,
        &evidence,
    );

    let event = latest_event(&fixture);
    assert_topic(&fixture, &event, symbol_short!("evidence"));
    let payload: (u32, String, u64) = TryFromVal::try_from_val(&fixture.env, &event.2).unwrap();
    assert_eq!(payload, (milestone_index, evidence, timestamp));
}

#[test]
fn milestone_event_topics_do_not_collide_with_each_other_or_existing_topics() {
    let milestone_topics = [
        symbol_short!("mlstn_rls"),
        symbol_short!("refunded"),
        symbol_short!("evidence"),
    ];
    for (index, topic) in milestone_topics.iter().enumerate() {
        assert!(
            milestone_topics[index + 1..]
                .iter()
                .all(|other| topic != other),
            "milestone event topics must be unique"
        );
    }

    let existing_topics = [
        symbol_short!("init"),
        symbol_short!("admin"),
        symbol_short!("created"),
        symbol_short!("contract"),
        symbol_short!("deposit"),
        symbol_short!("ctrct_st"),
        symbol_short!("ctrct_cmp"),
        symbol_short!("pause"),
        symbol_short!("unpaused"),
        symbol_short!("cancelled"),
        symbol_short!("fee"),
        symbol_short!("withdraw"),
        symbol_short!("dispute"),
        symbol_short!("opened"),
        symbol_short!("resolved"),
        symbol_short!("finalized"),
        symbol_short!("mlstn_idx"),
        symbol_short!("proto_fee"),
        symbol_short!("sttl_bind"),
        symbol_short!("repr_put"),
    ];
    for milestone_topic in milestone_topics {
        assert!(
            existing_topics
                .iter()
                .all(|topic| topic != &milestone_topic),
            "milestone topic {milestone_topic:?} collides with an existing topic"
        );
    }
}
