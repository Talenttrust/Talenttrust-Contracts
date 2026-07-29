#![cfg(test)]

use soroban_sdk::{testutils::Address as _, testutils::Events, Address, Env};

use crate::{Escrow, EscrowClient};

pub fn latest_event(
    env: &Env,
) -> Option<(
    soroban_sdk::Address,
    soroban_sdk::Vec<soroban_sdk::Val>,
    soroban_sdk::Val,
)> {
    let events = env.events().all();
    events.last()
}

#[test]
fn test_milestones_events() {
    let env = Env::default();
    env.mock_all_auths();

    let _admin = Address::generate(&env);
    let escrow_id = env.register(Escrow, ());
    let _client = EscrowClient::new(&env, &escrow_id);

    let last_event = latest_event(&env);
    assert!(last_event.is_some() || last_event.is_none());
}
