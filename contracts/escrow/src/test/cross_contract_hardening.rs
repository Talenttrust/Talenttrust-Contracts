use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{Escrow, EscrowClient, EscrowError};

#[test]
fn test_set_and_get_token_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    assert!(client.set_token_contract(&admin, &token));
    assert_eq!(client.get_token_contract(), Some(token));
}

#[test]
fn test_guarded_transfer_requires_configured_token_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let from = Address::generate(&env);
    let to = Address::generate(&env);

    let result = client.try_guarded_external_transfer(&from, &to, &100_i128);
    assert_eq!(result, Err(Ok(EscrowError::TokenContractNotSet)));
}

#[test]
fn test_guarded_transfer_rejects_non_positive_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    assert!(client.set_token_contract(&admin, &token));

    let result = client.try_guarded_external_transfer(&from, &to, &0_i128);
    assert_eq!(result, Err(Ok(EscrowError::AmountMustBePositive)));
}
