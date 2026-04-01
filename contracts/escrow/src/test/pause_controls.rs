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
fn initialize_only_once_fails() {
    let (env, contract_id, admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    super::assert_contract_error(
        client.try_initialize(&admin),
        EscrowError::AlreadyInitialized,
    );
}

#[test]
fn pause_then_unpause_toggles_state() {
    let (env, contract_id, admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert_eq!(client.get_admin(), Some(admin.clone()));
    assert!(!client.is_paused());
    assert!(client.pause());
    assert!(client.is_paused());

    assert!(client.unpause());
    assert!(!client.is_paused());
}

#[test]
fn pause_blocks_contract_creation() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.pause());
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, 50_i128, 75_i128];

    let result = client.try_create_contract(&client_addr, &freelancer, &milestones);
    super::assert_contract_error(result, EscrowError::ContractPaused);
}

#[test]
fn pause_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(client.try_pause(), EscrowError::NotInitialized);
}
