use soroban_sdk::{testutils::Address as _, Address, Env};

use super::register_client;
use crate::{DataKey, Escrow, EscrowClient, SettlementState};

#[test]
fn settlement_state_defaults_when_unset() {
    let env = Env::default();
    let client = register_client(&env);

    assert_eq!(client.get_settlement_state(), SettlementState::default());
    assert!(client.get_settlement_token().is_none());
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

#[test]
fn settlement_state_returns_stored_binding_and_fee_boundary() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let token = Address::generate(&env);
    let fees = i128::MAX;
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::SettlementToken, &token);
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &fees);
    });

    let state = client.get_settlement_state();

    assert_eq!(state.token, Some(token));
    assert_eq!(state.accumulated_protocol_fees, fees);
    assert_eq!(
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<_, i128>(&DataKey::AccumulatedProtocolFees)
        }),
        Some(fees),
        "read-only settlement view must not mutate persisted fees"
    );
}
