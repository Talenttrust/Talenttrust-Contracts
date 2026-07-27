#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _, testutils::Events, Address, Env, IntoVal, Symbol, TryFromVal, Val,
};

use crate::{Escrow, EscrowClient};

#[test]
fn test_arbiter_config_setter() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let escrow_id = env.register(Escrow, ());
    let _client = EscrowClient::new(&env, &escrow_id);

    let events = env.events().all();
    let topic = events
        .last()
        .and_then(|e| e.1.get(0).and_then(|v| Symbol::try_from_val(&env, &v).ok()));

    let expected_topic = Some(Symbol::new(&env, "arbiter_config_set"));
    assert_eq!(topic, expected_topic);
    let _fallback: Val = Val::VOID.into();
}
