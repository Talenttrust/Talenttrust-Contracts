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
fn emergency_blocks_create_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.activate_emergency_pause());
    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, 10_i128, 20_i128];

    super::assert_contract_error(
        client.try_create_contract(&client_addr, &freelancer, &milestones),
        EscrowError::ContractPaused,
    );
}

#[test]
fn emergency_blocks_deposit_funds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (_, _, cid) = super::create_contract(&env, &client);
    assert!(client.activate_emergency_pause());

    super::assert_contract_error(
        client.try_deposit_funds(&cid, &100_i128),
        EscrowError::ContractPaused,
    );
}

#[test]
fn emergency_blocks_release_milestone() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (_, _, cid) = super::create_contract(&env, &client);
    client.deposit_funds(&cid, &super::total_milestone_amount());
    assert!(client.activate_emergency_pause());

    super::assert_contract_error(
        client.try_release_milestone(&cid, &0),
        EscrowError::ContractPaused,
    );
}

#[test]
fn emergency_blocks_refund_milestone() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (_, _, cid) = super::create_contract(&env, &client);
    client.deposit_funds(&cid, &super::total_milestone_amount());
    assert!(client.activate_emergency_pause());

    super::assert_contract_error(
        client.try_refund_milestone(&cid, &vec![&env, 0_u32]),
        EscrowError::ContractPaused,
    );
}

#[test]
fn emergency_blocks_cancel_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (client_addr, _, cid) = super::create_contract(&env, &client);
    assert!(client.activate_emergency_pause());

    super::assert_contract_error(
        client.try_cancel_contract(&cid, &client_addr),
        EscrowError::ContractPaused,
    );
}

#[test]
fn emergency_blocks_issue_reputation() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let (client_addr, freelancer_addr, cid) = super::complete_contract(&env, &client);
    assert!(client.activate_emergency_pause());

    super::assert_contract_error(
        client.try_issue_reputation(&cid, &client_addr, &freelancer_addr, &5),
        EscrowError::ContractPaused,
    );
}

#[test]
fn resolve_emergency_restores_operations() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.activate_emergency_pause());
    assert!(client.resolve_emergency());
    assert!(!client.is_emergency());
    assert!(!client.is_paused());

    // All mutating ops should work again
    let (_, _, cid) = super::create_contract(&env, &client);
    assert!(client.deposit_funds(&cid, &super::total_milestone_amount()));
    assert!(client.release_milestone(&cid, &0));
}

#[test]
fn emergency_marks_readiness_checklist() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.activate_emergency_pause());
    assert!(client.resolve_emergency());

    let info = client.get_mainnet_readiness_info();
    assert!(info.emergency_controls_enabled);
}
