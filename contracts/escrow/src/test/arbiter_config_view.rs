#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{Escrow, EscrowClient};

#[test]
fn test_arbiter_config_view() {
    let env = Env::default();
    env.mock_all_auths();

    let _admin = Address::generate(&env);
    let escrow_id = env.register(Escrow, ());
    let _client = EscrowClient::new(&env, &escrow_id);
}
