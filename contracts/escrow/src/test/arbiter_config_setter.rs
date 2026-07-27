#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env, Symbol, TryFromVal, Val,
};

use crate::{DisputeConfig, Escrow, EscrowClient, EscrowError};

fn setup(env: &Env) -> (EscrowClient<'_>, Address) {
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(env, &escrow_address);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn valid_set_stores_and_readable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);

    assert!(client.set_arbiter_config(&4000u32, &6000u32));

    let config = client.get_arbiter_config();
    assert_eq!(config.partial_refund_freelancer_bps, 4000);
    assert_eq!(config.partial_refund_client_bps, 6000);
    let _ = admin;
}

#[test]
fn sum_not_equal_to_10000_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let result = client.try_set_arbiter_config(&3000u32, &6000u32);
    assert!(result.is_err());
}

#[test]
fn individual_value_over_10000_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let result = client.try_set_arbiter_config(&11000u32, &0u32);
    assert!(result.is_err());
}

#[test]
fn non_admin_rejected() {
    let env = Env::default();
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_address);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);

    // Override mock to only allow the attacker's auth, not admin's
    let attacker = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &escrow_address,
            fn_name: "set_arbiter_config",
            args: soroban_sdk::vec![&env, 5000u32.into(), 5000u32.into()],
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_arbiter_config(&5000u32, &5000u32);
    assert!(result.is_err());
}

#[test]
fn event_emitted_on_valid_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    client.set_arbiter_config(&3000u32, &7000u32);

    let events = env.events().all();
    let has_arbiter_cfg = events.iter().any(|e| {
        Symbol::try_from_val(&env, &e.1.get(0).unwrap_or(Val::VOID.into()))
            .ok()
            .as_ref()
            == Some(&Symbol::new(&env, "arbiter_cfg"))
    });
    assert!(has_arbiter_cfg, "expected arbiter_cfg event to be emitted");
}

#[test]
fn default_unchanged_if_set_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let _ = client.try_set_arbiter_config(&3000u32, &6000u32); // sum != 10000

    let config = client.get_arbiter_config();
    assert_eq!(config, DisputeConfig::default());
}
