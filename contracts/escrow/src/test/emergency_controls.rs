use crate::{Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};
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

fn setup_funded_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 100_i128, 200_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &client_addr, &300_i128);
    (client_addr, freelancer_addr, id)
}

fn setup_completed_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let (client_addr, freelancer_addr, id) = setup_funded_contract(env, client);
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    client.approve_milestone_release(&id, &client_addr, &1);
    client.release_milestone(&id, &client_addr, &1);
    (client_addr, freelancer_addr, id)
}

// ─── flag state ──────────────────────────────────────────────────────────────

#[test]
fn activate_emergency_sets_both_flags() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(!client.is_emergency());
    assert!(!client.is_paused());
    assert!(client.activate_emergency_pause());
    assert!(client.is_emergency());
    assert!(client.is_paused());
}

#[test]
fn unpause_fails_while_emergency_active() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    client.activate_emergency_pause();
    super::assert_contract_error(client.try_unpause(), Error::EmergencyActive);
}

#[test]
fn resolve_emergency_clears_both_flags() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    client.activate_emergency_pause();
    assert!(client.resolve_emergency());
    assert!(!client.is_emergency());
    assert!(!client.is_paused());
}

// ─── create_contract blocked ─────────────────────────────────────────────────

#[test]
fn emergency_blocks_create_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    client.activate_emergency_pause();

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    super::assert_contract_error(
        client.try_create_contract(
            &a,
            &b,
            &None,
            &vec![&env, 50_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        Error::ContractPaused,
    );
}

// ─── deposit_funds blocked ───────────────────────────────────────────────────

#[test]
fn emergency_blocks_deposit_funds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _, id) = setup_funded_contract(&env, &client);
    client.activate_emergency_pause();

    super::assert_contract_error(
        client.try_deposit_funds(&id, &client_addr, &50_i128),
        Error::ContractPaused,
    );
}

// ─── release_milestone blocked ───────────────────────────────────────────────

#[test]
fn emergency_blocks_release_milestone() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _, id) = setup_funded_contract(&env, &client);
    client.activate_emergency_pause();

    super::assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        Error::ContractPaused,
    );
}

// ─── issue_reputation blocked ────────────────────────────────────────────────

#[test]
#[ignore]
fn emergency_blocks_issue_reputation() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer_addr, id) = setup_completed_contract(&env, &client);
    client.activate_emergency_pause();

    let comment = soroban_sdk::String::from_str(&env, "Good job");
    super::assert_contract_error(
        client.try_issue_reputation(&id, &client_addr, &5_u32, &comment),
        Error::ContractPaused,
    );
}

// ─── cancel_contract blocked ─────────────────────────────────────────────────

#[test]
fn emergency_blocks_cancel_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _, id) = setup_funded_contract(&env, &client);
    client.activate_emergency_pause();

    super::assert_contract_error(
        client.try_cancel_contract(&id, &client_addr),
        Error::ContractPaused,
    );
}

// ─── resolve restores operations ─────────────────────────────────────────────

#[test]
fn resolve_emergency_restores_all_operations() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    client.activate_emergency_pause();
    client.resolve_emergency();

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let id = client.create_contract(
        &a,
        &b,
        &None,
        &vec![&env, 50_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);

    assert!(client.deposit_funds(&id, &a, &50_i128));
    assert!(client.cancel_contract(&id, &a));
}
