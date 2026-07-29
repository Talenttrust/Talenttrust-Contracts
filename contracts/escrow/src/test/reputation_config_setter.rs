#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _, testutils::Events, Address, Env, IntoVal, Symbol, TryFromVal, Val,
};

use crate::{Escrow, EscrowClient};

#[test]
fn test_reputation_config_setter() {
    let env = Env::default();
    env.mock_all_auths();

    let _admin = Address::generate(&env);
    let escrow_id = env.register(Escrow, ());
    let _client = EscrowClient::new(&env, &escrow_id);

    let events = env.events().all();

    let _fallback1: Val = Val::VOID.into();
    let topic1 = events
        .last()
        .and_then(|e| e.1.get(0).and_then(|v| Symbol::try_from_val(&env, &v).ok()));
    let expected1 = Some(Symbol::new(&env, "reputation_config_set"));
    assert_eq!(topic1, expected1);

    let _fallback2: Val = Val::VOID.into();
    let topic2 = events
        .last()
        .and_then(|e| e.1.get(0).and_then(|v| Symbol::try_from_val(&env, &v).ok()));
    let expected2 = Some(Symbol::new(&env, "reputation_config_updated"));
    assert_eq!(topic2, expected2);
}
