use super::{assert_contract_error, EscrowFixture};
use crate::{ContractStatus, Error};
use soroban_sdk::{
    symbol_short,
    testutils::Events as _,
    token::StellarAssetClient,
    Address, Env, Symbol, TryFromVal, Val, Vec,
};

fn has_deposit_event(env: &Env) -> bool {
    let topic = symbol_short!("deposit");
    env.events().all().iter().any(|event| {
        let topics = &event.1;
        !topics.is_empty()
            && Symbol::try_from_val(env, &topics.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&topic)
    })
}

fn count_deposit_events(env: &Env) -> usize {
    let topic = symbol_short!("deposit");
    env.events()
        .all()
        .iter()
        .filter(|event| {
            let topics = &event.1;
        !topics.is_empty()
            && Symbol::try_from_val(env, &topics.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&topic)
        })
        .count()
}

#[test]
fn full_deposit_emits_deposit_event() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &total));

    assert!(
        has_deposit_event(&fixture.env),
        "full deposit must emit a deposit event"
    );
    assert_eq!(
        count_deposit_events(&fixture.env),
        1,
        "exactly one deposit event expected"
    );
}

#[test]
fn full_deposit_event_payload_correctness() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &total));

    let deposit_topic = symbol_short!("deposit");
    let event = fixture
        .env
        .events()
        .all()
        .iter()
        .find(|event| {
            let topics = &event.1;
            !topics.is_empty()
                && Symbol::try_from_val(&fixture.env, &topics.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&deposit_topic)
        })
        .expect("deposit event must exist");

    let topics = &event.1;
    assert_eq!(topics.len(), 2, "expected 2 topics: deposit + contract_id");
    assert_eq!(
        Symbol::try_from_val(&fixture.env, &topics.get(0).unwrap()).unwrap(),
        deposit_topic
    );
    assert_eq!(
        u32::try_from_val(&fixture.env, &topics.get(1).unwrap()).unwrap(),
        fixture.escrow_id
    );

    let data: Vec<Val> = Vec::try_from_val(&fixture.env, &event.2).unwrap();
    assert_eq!(
        data.len(),
        4,
        "expected 4 data fields: caller, amount, status, timestamp"
    );

    assert_eq!(
        Address::try_from_val(&fixture.env, &data.get(0).unwrap()).unwrap(),
        fixture.client
    );
    assert_eq!(
        i128::try_from_val(&fixture.env, &data.get(1).unwrap()).unwrap(),
        total
    );
    assert_eq!(
        u32::try_from_val(&fixture.env, &data.get(2).unwrap()).unwrap(),
        ContractStatus::Funded as u32
    );
}

#[test]
fn partial_deposit_emits_event_with_partially_funded_status() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let half = total / 2;
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &half));

    let deposit_topic = symbol_short!("deposit");
    let event = fixture
        .env
        .events()
        .all()
        .iter()
        .find(|event| {
            let topics = &event.1;
            !topics.is_empty()
                && Symbol::try_from_val(&fixture.env, &topics.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&deposit_topic)
        })
        .expect("deposit event must exist for partial deposit");

    let data: Vec<Val> = Vec::try_from_val(&fixture.env, &event.2).unwrap();
    assert_eq!(
        u32::try_from_val(&fixture.env, &data.get(2).unwrap()).unwrap(),
        ContractStatus::PartiallyFunded as u32,
        "partial deposit must report PartiallyFunded status"
    );
}

#[test]
fn multiple_incremental_deposits_transition_status_correctly() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let half = total / 2;
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &half));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::PartiallyFunded
    );

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &(total - half)));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
}

#[test]
fn rejected_deposit_does_not_emit_event() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert_contract_error(
        escrow.try_deposit_funds(&fixture.escrow_id, &fixture.freelancer, &total),
        Error::UnauthorizedRole,
    );
    assert!(
        !has_deposit_event(&fixture.env),
        "rejected deposit (unauthorized caller) must not emit a deposit event"
    );
}

#[test]
fn zero_amount_deposit_rejected_no_event() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();

    assert_contract_error(
        escrow.try_deposit_funds(&fixture.escrow_id, &fixture.client, &0),
        Error::AmountMustBePositive,
    );
    assert!(
        !has_deposit_event(&fixture.env),
        "rejected deposit (zero amount) must not emit a deposit event"
    );
}

#[test]
fn no_topic_collision_deposit_is_unique() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    let events_before = fixture.env.events().all();
    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &total));

    let deposit_topic = symbol_short!("deposit");
    let events_after = fixture.env.events().all();
    let new_events = events_after.slice(events_before.len()..);
    let has_other_topic = new_events.iter().any(|event| {
        let topics = &event.1;
        !topics.is_empty()
            && Symbol::try_from_val(&fixture.env, &topics.get(0).unwrap())
                .ok()
                .as_ref()
                != Some(&deposit_topic)
    });
    assert!(
        !has_other_topic,
        "no event other than deposit should be emitted by deposit_funds"
    );
}
