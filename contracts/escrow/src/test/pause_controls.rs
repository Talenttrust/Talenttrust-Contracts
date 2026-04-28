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

    super::assert_contract_error(
        client.try_create_contract(&client_addr, &freelancer, &milestones),
        EscrowError::ContractPaused,
    );
}

#[test]
fn pause_blocks_deposit_funds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (_, _, cid) = super::create_contract(&env, &client);
    assert!(client.pause());

    super::assert_contract_error(
        client.try_deposit_funds(&cid, &100_i128),
        EscrowError::ContractPaused,
    );
}

#[test]
fn pause_blocks_release_milestone() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (_, _, cid) = super::create_contract(&env, &client);
    client.deposit_funds(&cid, &super::total_milestone_amount());
    assert!(client.pause());

    super::assert_contract_error(
        client.try_release_milestone(&cid, &0),
        EscrowError::ContractPaused,
    );
}

#[test]
fn pause_blocks_refund_milestone() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (_, _, cid) = super::create_contract(&env, &client);
    client.deposit_funds(&cid, &super::total_milestone_amount());
    assert!(client.pause());

    super::assert_contract_error(
        client.try_refund_milestone(&cid, &vec![&env, 0_u32]),
        EscrowError::ContractPaused,
    );
}

#[test]
fn pause_blocks_cancel_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (client_addr, _, cid) = super::create_contract(&env, &client);
    assert!(client.pause());

    super::assert_contract_error(
        client.try_cancel_contract(&cid, &client_addr),
        EscrowError::ContractPaused,
    );
}

#[test]
fn pause_blocks_issue_reputation() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (client_addr, freelancer_addr, cid) = super::complete_contract(&env, &client);
    assert!(client.pause());

    super::assert_contract_error(
        client.try_issue_reputation(&cid, &client_addr, &freelancer_addr, &5),
        EscrowError::ContractPaused,
    );
}

#[test]
fn pause_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(client.try_pause(), EscrowError::NotInitialized);
}

#[test]
fn unpause_restores_all_operations() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.pause());
    assert!(client.unpause());

    // All mutating ops should work again
    let (_, _, cid) = super::create_contract(&env, &client);
    assert!(client.deposit_funds(&cid, &super::total_milestone_amount()));
    assert!(client.release_milestone(&cid, &0));
}
