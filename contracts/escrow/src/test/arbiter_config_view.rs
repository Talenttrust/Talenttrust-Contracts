#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{DataKey, DisputeConfig, Escrow, EscrowClient};

#[test]
fn returns_default_before_init() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_address);

    let config = client.get_arbiter_config();
    assert_eq!(config, DisputeConfig::default());
}

#[test]
fn returns_default_after_init_before_set() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_address);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let config = client.get_arbiter_config();
    assert_eq!(config, DisputeConfig::default());
}

#[test]
fn returns_configured_values_after_set() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow_address = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_address);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let config = DisputeConfig {
        partial_refund_freelancer_bps: 4000,
        partial_refund_client_bps: 6000,
    };
    env.as_contract(&escrow_address, || {
        env.storage()
            .persistent()
            .set(&DataKey::DisputeConfigKey, &config);
    });

    let result = client.get_arbiter_config();
    assert_eq!(result.partial_refund_freelancer_bps, 4000);
    assert_eq!(result.partial_refund_client_bps, 6000);
}
