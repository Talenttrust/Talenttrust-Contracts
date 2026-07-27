#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env, String, Symbol, TryFromVal, Val,
};

use crate::{Error, Escrow, EscrowClient, ReputationConfig};

use super::complete_contract;

fn setup(env: &Env) -> (EscrowClient<'_>, Address) {
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(env, &escrow_address);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    (client, admin)
}

// ── get_reputation_config defaults ──────────────────────────────────────────

#[test]
fn returns_default_before_init() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_address);

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
    super::assert_contract_error(result, Error::InvalidReputationParameters);
}

#[test]
fn max_rating_below_min_rating_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let result = client.try_set_reputation_config(&5u32, &4u32, &200u32);
    super::assert_contract_error(result, Error::InvalidReputationParameters);
}

#[test]
fn max_rating_over_ceiling_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let result = client.try_set_reputation_config(&1u32, &11u32, &200u32);
    super::assert_contract_error(result, Error::InvalidReputationParameters);
}

#[test]
fn max_comment_bytes_zero_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let result = client.try_set_reputation_config(&1u32, &5u32, &0u32);
    super::assert_contract_error(result, Error::InvalidReputationParameters);
}

#[test]
fn max_comment_bytes_over_ceiling_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let result = client.try_set_reputation_config(&1u32, &5u32, &1_001u32);
    super::assert_contract_error(result, Error::InvalidReputationParameters);
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
    let has_rep_cfg = events.iter().any(|e| {
        Symbol::try_from_val(&env, &e.1.get(0).unwrap_or(Val::VOID.into()))
            .ok()
            .as_ref()
            == Some(&Symbol::new(&env, "rep_cfg"))
    });
    assert!(has_rep_cfg, "expected rep_cfg event to be emitted");
}

#[test]
fn no_event_emitted_when_set_fails() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let _ = client.try_set_reputation_config(&0u32, &5u32, &200u32);

    let events = env.events().all();
    let has_rep_cfg = events.iter().any(|e| {
        Symbol::try_from_val(&env, &e.1.get(0).unwrap_or(Val::VOID.into()))
            .ok()
            .as_ref()
            == Some(&Symbol::new(&env, "rep_cfg"))
    });
    assert!(
        !has_rep_cfg,
        "rep_cfg event must not be emitted on a rejected set"
    );
}

// ── issue_reputation actually enforces the configured bounds ────────────────

#[test]
fn issue_reputation_uses_updated_rating_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_address);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Narrow the rating scale to [3, 4]; the old default of 1 must now be rejected.
    assert!(client.set_reputation_config(&3u32, &4u32, &200u32));

    let (client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    let comment = String::from_str(&env, "great work");
    let result = client.try_issue_reputation(&contract_id, &client_addr, &1u32, &comment);
    super::assert_contract_error(result, Error::InvalidRating);
}

#[test]
fn issue_reputation_accepts_rating_within_updated_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_address);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    assert!(client.set_reputation_config(&3u32, &4u32, &200u32));

    let (client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    let comment = String::from_str(&env, "great work");
    assert!(client.issue_reputation(&contract_id, &client_addr, &4u32, &comment));
}

#[test]
fn issue_reputation_uses_updated_comment_byte_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_address);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Shrink the comment cap to 5 bytes; a 10-byte comment must now be rejected
    // even though it was well within the original 200-byte default.
    assert!(client.set_reputation_config(&1u32, &5u32, &5u32));

    let (client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    let comment = String::from_str(&env, "0123456789");
    let result = client.try_issue_reputation(&contract_id, &client_addr, &5u32, &comment);
    super::assert_contract_error(result, Error::CommentTooLong);
}
