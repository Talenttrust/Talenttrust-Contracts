#![cfg(test)]

use crate::test::{register_client, setup_test_env};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_escrow_initialization_sanity_check() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let client = register_client(&env, &admin);

    assert!(!client.is_paused());
}
