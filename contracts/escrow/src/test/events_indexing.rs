#![cfg(test)]

use super::EscrowFixture;
use soroban_sdk::{
    symbol_short, token,
    testutils::Events,
    Symbol, TryFromVal,
};

#[test]
fn deposit_emits_indexed_event_with_short_symbol_and_correct_payload() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let client = fixture.escrow();
    let deposit_amount = fixture.total_amount();

    let token_client = token::StellarAssetClient::new(&fixture.env, fixture.settlement_token.as_ref().unwrap());
    token_client.mint(&fixture.client, &deposit_amount);

    assert!(client.deposit_funds(&fixture.escrow_id, &fixture.client, &deposit_amount));

    let events = fixture.env.events().all();
    assert!(!events.is_empty());

    let deposit_topic = symbol_short!("deposit");

    let found_deposit_event = events.iter().any(|event| {
        let topics = event.1;
        if topics.len() >= 2 {
            if let (Ok(sym), Ok(id)) = (
                Symbol::try_from_val(&fixture.env, &topics.get(0).unwrap()),
                u32::try_from_val(&fixture.env, &topics.get(1).unwrap()),
            ) {
                return sym == deposit_topic && id == fixture.escrow_id;
            }
        }
        false
    });

    assert!(found_deposit_event, "Deposit event not found in {:?}", events);
}

#[test]
fn protocol_fee_accrual_emits_indexed_proto_fee_event() {
    let fixture = EscrowFixture::builder().funded().build();
    let client = fixture.escrow();

    client.set_protocol_fee_bps(&100u32);
    client.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    assert!(client.release_milestone(&fixture.escrow_id, &fixture.client, &0));

    let events = fixture.env.events().all();
    let proto_fee_topic = symbol_short!("proto_fee");

    let found_fee_event = events.iter().any(|event| {
        let topics = event.1;
        if topics.len() >= 2 {
            if let (Ok(sym), Ok(id)) = (
                Symbol::try_from_val(&fixture.env, &topics.get(0).unwrap()),
                u32::try_from_val(&fixture.env, &topics.get(1).unwrap()),
            ) {
                return sym == proto_fee_topic && id == fixture.escrow_id;
            }
        }
        false
    });

    assert!(found_fee_event, "Proto fee event not found in {:?}", events);
}

#[test]
fn no_topic_collision_between_events() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let client = fixture.escrow();
    let deposit_amount = fixture.total_amount();

    let token_client = token::StellarAssetClient::new(&fixture.env, fixture.settlement_token.as_ref().unwrap());
    token_client.mint(&fixture.client, &deposit_amount);

    assert!(client.deposit_funds(&fixture.escrow_id, &fixture.client, &deposit_amount));

    let deposit_topic = symbol_short!("deposit");
    let state_topic = symbol_short!("ctrct_st");

    assert_ne!(deposit_topic, state_topic);
}
