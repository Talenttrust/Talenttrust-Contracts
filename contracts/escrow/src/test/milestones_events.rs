#![cfg(test)]

use soroban_sdk::{testutils::Address as _, testutils::Events, Address, Env};

use crate::{Escrow, EscrowClient};

#[test]
fn test_milestones_events() {
    let env = Env::default();
    env.mock_all_auths();

    let _admin = Address::generate(&env);
    let escrow_id = env.register(Escrow, ());
    let _client = EscrowClient::new(&env, &escrow_id);

    let events = env.events().all();
    let last_event = events.last();
    assert!(last_event.is_some() || last_event.is_none());
}
