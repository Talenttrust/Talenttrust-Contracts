#![cfg(test)]

use crate::events::emit_contract_indexed_event;
use crate::test::EscrowFixture;
use crate::Contract;
use soroban_sdk::testutils::Events;
use soroban_sdk::{symbol_short, Env, Symbol, TryFromVal};

#[test]
#[should_panic(expected = "InvalidContractId")]
fn emit_contract_indexed_event_validates_contract_id_nonzero() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = Contract::default();
    emit_contract_indexed_event(&env, 0, &contract);
}

#[test]
fn emit_contract_indexed_event_accepts_valid_contract_id() {
    let fixture = EscrowFixture::builder().build();
    let events_before = fixture.env.events().all().len();
    let contract = Contract::default();
    emit_contract_indexed_event(&fixture.env, fixture.escrow_id, &contract);
    let events_after = fixture.env.events().all().len();
    assert!(
        events_after > events_before,
        "must emit an event for valid contract_id"
    );
}

#[test]
fn emit_contract_indexed_event_publishes_correct_topic_and_payload() {
    let fixture = EscrowFixture::builder().build();
    let contract = Contract {
        status: crate::ContractStatus::Funded,
        funded_amount: 1000,
        released_amount: 500,
        refunded_amount: 200,
        total_deposited: 1000,
        ..Default::default()
    };
    emit_contract_indexed_event(&fixture.env, fixture.escrow_id, &contract);

    let events = fixture.env.events().all();
    let found = events.iter().any(|event| {
        if event.1.len() != 2 {
            return false;
        }
        let topic0: Symbol =
            Symbol::try_from_val(&fixture.env, &event.1.get(0).unwrap()).unwrap();
        if topic0 != symbol_short!("contract") {
            return false;
        }
        let topic1: u32 =
            TryFromVal::try_from_val(&fixture.env, &event.1.get(1).unwrap()).unwrap();
        if topic1 != fixture.escrow_id {
            return false;
        }
        let payload: (u32, i128, i128, i128, i128) =
            TryFromVal::try_from_val(&fixture.env, &event.2).unwrap();
        payload == (crate::ContractStatus::Funded as u32, 1000, 500, 200, 1000)
    });
    assert!(found, "event with correct topic and payload must exist");
}

#[test]
#[should_panic(expected = "InvalidContractId")]
fn emit_contract_indexed_event_rejects_zero_contract_id() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = Contract::default();
    emit_contract_indexed_event(&env, 0, &contract);
}

#[test]
fn emit_contract_indexed_event_emits_for_max_contract_id() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = Contract::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        emit_contract_indexed_event(&env, u32::MAX, &contract);
    }));
    assert!(
        result.is_ok(),
        "max u32 contract_id must not panic"
    );
}
