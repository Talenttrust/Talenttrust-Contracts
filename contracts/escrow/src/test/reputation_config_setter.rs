use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_reputation_config_setter() {
    let env = Env::default();
    env.mock_all_auths();
}
