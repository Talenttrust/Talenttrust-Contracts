#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _, testutils::Events, Address, Env, IntoVal, Symbol, TryFromVal, Val,
};

use crate::{Escrow, EscrowClient};

#[test]
fn test_reputation_config_setter() {
    let env = Env::default();
    env.mock_all_auths();

    let config = client.get_reputation_config();
    assert_eq!(config, ReputationConfig::default());
}

#[test]
fn returns_default_after_init_before_set() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let config = client.get_reputation_config();
    assert_eq!(config, ReputationConfig::default());
    assert_eq!(config.min_rating, 1);
    assert_eq!(config.max_rating, 5);
    assert_eq!(config.max_comment_bytes, 200);
}

// ── valid set ────────────────────────────────────────────────────────────────

#[test]
fn valid_set_stores_and_readable() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    assert!(client.set_reputation_config(&2u32, &8u32, &300u32));

    let config = client.get_reputation_config();
    assert_eq!(config.min_rating, 2);
    assert_eq!(config.max_rating, 8);
    assert_eq!(config.max_comment_bytes, 300);
}

#[test]
fn valid_set_at_exact_ceilings_accepted() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // min_rating floor (1), max_rating ceiling (10), max_comment_bytes ceiling (1_000).
    assert!(client.set_reputation_config(&1u32, &10u32, &1_000u32));

    let config = client.get_reputation_config();
    assert_eq!(config.min_rating, 1);
    assert_eq!(config.max_rating, 10);
    assert_eq!(config.max_comment_bytes, 1_000);
}

#[test]
fn valid_set_allows_equal_min_and_max_rating() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // A single-point scale (min == max) is a degenerate but internally
    // consistent range and must not be rejected.
    assert!(client.set_reputation_config(&3u32, &3u32, &50u32));

    let config = client.get_reputation_config();
    assert_eq!(config.min_rating, 3);
    assert_eq!(config.max_rating, 3);
}

// ── bounds rejections ───────────────────────────────────────────────────────

#[test]
fn min_rating_zero_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let result = client.try_set_reputation_config(&0u32, &5u32, &200u32);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);
}

#[test]
fn max_rating_below_min_rating_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let result = client.try_set_reputation_config(&5u32, &4u32, &200u32);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);
}

#[test]
fn max_rating_over_ceiling_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let result = client.try_set_reputation_config(&1u32, &11u32, &200u32);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);
}

#[test]
fn max_comment_bytes_zero_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let result = client.try_set_reputation_config(&1u32, &5u32, &0u32);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);
}

#[test]
fn max_comment_bytes_over_ceiling_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let result = client.try_set_reputation_config(&1u32, &5u32, &1_001u32);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);
}

#[test]
fn default_unchanged_if_set_fails() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let _ = client.try_set_reputation_config(&0u32, &5u32, &200u32);

    let config = client.get_reputation_config();
    assert_eq!(config, ReputationConfig::default());
}

// ── non-admin rejection ──────────────────────────────────────────────────────

#[test]
fn non_admin_rejected() {
    let env = Env::default();
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_address);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    // Override mock to only allow the attacker's auth, not admin's.
    let attacker = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &escrow_address,
            fn_name: "set_reputation_config",
            args: soroban_sdk::vec![&env, 2u32.into(), 8u32.into(), 300u32.into()],
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_reputation_config(&2u32, &8u32, &300u32);
    assert!(result.is_err());

    // Storage must remain untouched by the rejected call.
    let config = client.get_reputation_config();
    assert_eq!(config, ReputationConfig::default());
}

// ── event emission ───────────────────────────────────────────────────────────

#[test]
fn event_emitted_on_valid_set() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.set_reputation_config(&2u32, &8u32, &300u32);

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
