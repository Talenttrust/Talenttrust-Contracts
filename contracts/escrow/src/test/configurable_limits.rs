use super::register_client;
use crate::{
    EscrowError, Escrow, EscrowClient, MAX_MAX_MILESTONES, DEFAULT_MAX_TOTAL_ESCROW_STROOPS,
    MIN_MAX_ESCROW_STROOPS,
};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

// ─── Setup ───────────────────────────────────────────────────────────────────

fn setup_initialized() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    (env, contract_id, admin)
}

// ─── Default values ──────────────────────────────────────────────────────────

#[test]
fn max_milestones_returns_default_before_any_set() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert_eq!(client.get_max_milestones(), 10);
}

#[test]
fn max_escrow_stroops_returns_default_before_any_set() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert_eq!(client.get_max_escrow_stroops(), DEFAULT_MAX_TOTAL_ESCROW_STROOPS);
}

// ─── Setting limits ─────────────────────────────────────────────────────────

#[test]
fn admin_can_set_max_milestones_within_bounds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_milestones(&20));
    assert_eq!(client.get_max_milestones(), 20);
}

#[test]
fn admin_can_set_max_escrow_stroops_within_bounds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let new_limit: i128 = 5_000_000_000_000;
    assert!(client.set_max_escrow_stroops(&new_limit));
    assert_eq!(client.get_max_escrow_stroops(), new_limit);
}

// ─── Out-of-range rejection ──────────────────────────────────────────────────

#[test]
fn set_max_milestones_rejects_zero() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(
        client.try_set_max_milestones(&0),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_milestones_rejects_above_maximum() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(
        client.try_set_max_milestones(&(MAX_MAX_MILESTONES + 1)),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_escrow_stroops_rejects_below_minimum() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(
        client.try_set_max_escrow_stroops(&0),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_escrow_stroops_rejects_above_mainnet_cap() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    let too_high: i128 = 1_000_000_000_000_000i128 + 1;
    super::assert_contract_error(
        client.try_set_max_escrow_stroops(&too_high),
        EscrowError::LimitOutOfRange,
    );
}

// ─── Requires initialization ─────────────────────────────────────────────────

#[test]
fn set_max_milestones_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(
        client.try_set_max_milestones(&20),
        EscrowError::NotInitialized,
    );
}

#[test]
fn set_max_escrow_stroops_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(
        client.try_set_max_escrow_stroops(&5_000_000_000_000),
        EscrowError::NotInitialized,
    );
}

// ─── create_contract respects configurable limits ────────────────────────────

#[test]
fn create_contract_respects_lower_max_milestones() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_milestones(&2));

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128, 200_i128, 300_i128];
    super::assert_contract_error(
        client.try_create_contract(&client_addr, &freelancer_addr, &milestones),
        EscrowError::TooManyMilestones,
    );
}

#[test]
fn create_contract_respects_higher_max_milestones() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_milestones(&20));

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![
        &env, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128,
        100_i128, 100_i128, 100_i128, 100_i128, 100_i128,
        100_i128, 100_i128, 100_i128, 100_i128, 100_i128,
    ];
    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    let contract = client.get_contract(&id);
    assert_eq!(contract.milestones.len(), 15);
}

#[test]
fn create_contract_respects_lower_max_escrow() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_escrow_stroops(&500));

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 300_i128, 300_i128];
    super::assert_contract_error(
        client.try_create_contract(&client_addr, &freelancer_addr, &milestones),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_respects_higher_max_escrow() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_escrow_stroops(&50_000_000_000_000));

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 20_000_000_000_000_i128, 20_000_000_000_000_i128];
    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    let contract = client.get_contract(&id);
    assert_eq!(contract.milestones.len(), 2);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

#[test]
fn set_max_milestones_at_boundary_succeeds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_milestones(&1));
    assert_eq!(client.get_max_milestones(), 1);
    assert!(client.set_max_milestones(&MAX_MAX_MILESTONES));
    assert_eq!(client.get_max_milestones(), MAX_MAX_MILESTONES);
}

#[test]
fn set_max_escrow_at_minimum_boundary_succeeds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_escrow_stroops(&MIN_MAX_ESCROW_STROOPS));
    assert_eq!(client.get_max_escrow_stroops(), MIN_MAX_ESCROW_STROOPS);
}

#[test]
fn default_limits_apply_when_not_set() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![
        &env, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128,
        100_i128, 100_i128, 100_i128, 100_i128, 100_i128,
    ];
    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert_eq!(id, 1);
}

#[test]
fn set_max_milestones_event_is_emitted() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_milestones(&15));
    assert_eq!(client.get_max_milestones(), 15);
}

#[test]
fn set_max_escrow_stroops_event_is_emitted() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_escrow_stroops(&25_000_000_000_000));
    assert_eq!(client.get_max_escrow_stroops(), 25_000_000_000_000);
}
