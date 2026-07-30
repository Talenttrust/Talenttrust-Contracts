use super::register_client;
use crate::{
    Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization, DEFAULT_MAX_ARBITERS,
    DEFAULT_MAX_TOTAL_ESCROW_STROOPS, MAX_MAX_ARBITERS, MAX_MAX_MILESTONES, MIN_MAX_ARBITERS,
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

    assert_eq!(
        client.get_max_escrow_stroops(),
        DEFAULT_MAX_TOTAL_ESCROW_STROOPS
    );
}

#[test]
fn max_arbiters_returns_default_before_any_set() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert_eq!(client.get_max_arbiters(), DEFAULT_MAX_ARBITERS);
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

#[test]
fn admin_can_set_max_arbiters_within_bounds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_arbiters(&3));
    assert_eq!(client.get_max_arbiters(), 3);
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

#[test]
fn set_max_arbiters_rejects_above_maximum() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(
        client.try_set_max_arbiters(&(MAX_MAX_ARBITERS + 1)),
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

    super::assert_contract_error(client.try_set_max_milestones(&20), Error::NotInitialized);
}

#[test]
fn set_max_escrow_stroops_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(
        client.try_set_max_escrow_stroops(&5_000_000_000_000),
        Error::NotInitialized,
    );
}

#[test]
fn set_max_arbiters_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    super::assert_contract_error(client.try_set_max_arbiters(&3), Error::NotInitialized);
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
        client.try_create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        ),
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
        &env, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128,
        100_i128, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128,
    ];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let contract = client.get_contract_summary(&id);
    assert_eq!(contract.milestones.len(), 15);
}

#[test]
fn create_contract_respects_lower_max_escrow() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_escrow_stroops(&5_000_000));

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 3_000_000_i128, 3_000_000_i128];
    super::assert_contract_error(
        client.try_create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_respects_higher_max_escrow() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_escrow_stroops(&5_000_000));

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 2_000_000_i128, 2_000_000_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let contract = client.get_contract_summary(&id);
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
fn set_max_arbiters_at_boundary_succeeds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_arbiters(&MIN_MAX_ARBITERS));
    assert_eq!(client.get_max_arbiters(), MIN_MAX_ARBITERS);
    assert!(client.set_max_arbiters(&MAX_MAX_ARBITERS));
    assert_eq!(client.get_max_arbiters(), MAX_MAX_ARBITERS);
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
        &env, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128, 100_i128,
        100_i128, 100_i128,
    ];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
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

#[test]
fn set_max_arbiters_event_is_emitted() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(client.set_max_arbiters(&4));
    assert_eq!(client.get_max_arbiters(), 4);
}
