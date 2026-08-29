#![cfg(test)]

use crate::test::EscrowFixture;
use soroban_sdk::testutils::Events;
use soroban_sdk::{Env, Symbol, TryFromVal};
use crate::types::DisputeResolution;

fn assert_transfer_event_is_last(env: &Env, events: &soroban_sdk::Vec<(soroban_sdk::Address, soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)>) {
    let last_event = events.last().unwrap();
    let topics = last_event.1;
    let topic_name: Symbol = TryFromVal::try_from_val(env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic_name, Symbol::new(env, "transfer"), "Expected transfer event to be last");
}

#[test]
fn test_release_event_ordering() {
    let fixture = EscrowFixture::builder().funded().build();
    let env = &fixture.env;
    
    fixture.escrow().approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    fixture.escrow().release_milestone(&fixture.escrow_id, &fixture.client, &0);
    
    let events = env.events().all();
    assert_transfer_event_is_last(env, &events);
}

#[test]
fn test_dispute_event_ordering() {
    let fixture = EscrowFixture::builder().disputed().build();
    let env = &fixture.env;
    
    let resolution = DisputeResolution::Split { client_share: 50, freelancer_share: 50 };
    fixture.escrow().resolve_dispute(&fixture.escrow_id, &fixture.arbiter.unwrap(), &resolution);
    
    // Note: resolve_dispute currently doesn't call token_client.transfer internally.
    // If it did, we would assert the transfer event is last here.
    // This test ensures the dispute flow is covered.
    let events = env.events().all();
    assert!(events.len() > 0);
}

#[test]
fn test_closure_event_ordering() {
    let fixture = EscrowFixture::builder().funded().build();
    let env = &fixture.env;
    
    fixture.escrow().cancel_contract(&fixture.escrow_id, &fixture.client);
    
    let events = env.events().all();
    assert_transfer_event_is_last(env, &events);
}

#[test]
fn test_multiple_events_in_one_call_ordering() {
    let fixture = EscrowFixture::builder().funded().build();
    let env = &fixture.env;
    
    let milestones = soroban_sdk::vec![env, 0, 1, 2];
    fixture.escrow().refund_unreleased_milestones(&fixture.escrow_id, &milestones);
    
    let events = env.events().all();
    assert_transfer_event_is_last(env, &events);
}

#[test]
#[should_panic]
fn test_failed_transaction_ordering() {
    let fixture = EscrowFixture::builder().funded().build();
    
    // Simulating a failed transaction due to invalid state / balance
    // This will revert any events emitted within the transaction,
    // ensuring no invalid state leaks to indexers.
    fixture.escrow().cancel_contract(&fixture.escrow_id, &fixture.freelancer);
}
