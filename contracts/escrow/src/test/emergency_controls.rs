use crate::{Escrow, EscrowClient, EscrowError};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

fn setup_initialized() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    (env, contract_id, admin)
}

#[test]
fn activate_emergency_sets_flags() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(!client.is_emergency());
    assert!(!client.is_paused());
    assert!(client.activate_emergency_pause());
    assert!(client.is_emergency());
    assert!(client.is_paused());
}

#[test]
fn unpause_fails_while_emergency_is_active() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.activate_emergency_pause());
    super::assert_contract_error(client.try_unpause(), EscrowError::EmergencyActive);
}

#[test]
fn resolve_emergency_restores_operations() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.activate_emergency_pause());
    assert!(client.resolve_emergency());
    assert!(!client.is_emergency());
    assert!(!client.is_paused());

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 10_i128, 20_i128];

    let created = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert_eq!(created, 1);
}
