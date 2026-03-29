use soroban_sdk::symbol_short;

use super::{register_escrow, setup_env};

#[test]
fn test_hello() {
    let env = setup_env();
    let client = register_escrow(&env);

    let result = client.hello(&symbol_short!("World"));

    assert_eq!(result, symbol_short!("World"));
}
