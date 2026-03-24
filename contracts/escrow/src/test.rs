use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env, Vec};

use crate::{ContractStatus, Escrow, EscrowClient};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (EscrowClient<'_>, Address, Address) {
    env.mock_all_auths();
    let contract_addr = env.register(Escrow, ());
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    (EscrowClient::new(env, &contract_addr), client_addr, freelancer_addr)
}

fn default_milestones(env: &Env) -> Vec<i128> {
    vec![env, 200_0000000_i128, 400_0000000_i128]
}

// ── sanity / existing CI tests ────────────────────────────────────────────────

#[test]
fn test_hello() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

// ── create_contract ───────────────────────────────────────────────────────────

#[test]
fn test_create_contract_returns_sequential_ids() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let ms = default_milestones(&env);

    let id1 = client.create_contract(&client_addr, &freelancer_addr, &ms);
    let id2 = client.create_contract(&client_addr, &freelancer_addr, &ms);

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

#[test]
fn test_create_contract_initial_status_is_created() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let ms = default_milestones(&env);

    let id = client.create_contract(&client_addr, &freelancer_addr, &ms);

    assert_eq!(client.get_status(&id), ContractStatus::Created);
}

#[test]
fn test_create_contract_empty_milestones_fails() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let empty: Vec<i128> = Vec::new(&env);

    let result = client.try_create_contract(&client_addr, &freelancer_addr, &empty);
    assert!(result.is_err());
}

// ── accept_contract ───────────────────────────────────────────────────────────

#[test]
fn test_accept_contract_transitions_to_accepted() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);

    assert_eq!(client.get_status(&id), ContractStatus::Accepted);
}

#[test]
fn test_accept_nonexistent_contract_fails() {
    let env = Env::default();
    let (client, _ca, _fa) = setup(&env);

    let result = client.try_accept_contract(&99);
    assert!(result.is_err());
}

#[test]
fn test_double_acceptance_fails() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);
    let result = client.try_accept_contract(&id);
    assert!(result.is_err());
}

#[test]
fn test_cannot_accept_funded_contract() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);
    client.deposit_funds(&id, &600_0000000_i128);

    let result = client.try_accept_contract(&id);
    assert!(result.is_err());
}

// ── deposit_funds ─────────────────────────────────────────────────────────────

#[test]
fn test_deposit_after_acceptance_transitions_to_funded() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);
    let ok = client.deposit_funds(&id, &600_0000000_i128);

    assert!(ok);
    assert_eq!(client.get_status(&id), ContractStatus::Funded);
}

#[test]
fn test_deposit_without_acceptance_fails() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    // freelancer has NOT accepted yet
    let result = client.try_deposit_funds(&id, &600_0000000_i128);
    assert!(result.is_err());
}

#[test]
fn test_deposit_zero_amount_fails() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);
    let result = client.try_deposit_funds(&id, &0);
    assert!(result.is_err());
}

#[test]
fn test_deposit_negative_amount_fails() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);
    let result = client.try_deposit_funds(&id, &-1);
    assert!(result.is_err());
}

#[test]
fn test_deposit_nonexistent_contract_fails() {
    let env = Env::default();
    let (client, _ca, _fa) = setup(&env);

    let result = client.try_deposit_funds(&99, &100);
    assert!(result.is_err());
}

// ── full acceptance handshake flow ────────────────────────────────────────────

#[test]
fn test_full_handshake_created_accepted_funded() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let ms = vec![&env, 500_0000000_i128];
    let id = client.create_contract(&client_addr, &freelancer_addr, &ms);

    assert_eq!(client.get_status(&id), ContractStatus::Created);

    client.accept_contract(&id);
    assert_eq!(client.get_status(&id), ContractStatus::Accepted);

    client.deposit_funds(&id, &500_0000000_i128);
    assert_eq!(client.get_status(&id), ContractStatus::Funded);
}

#[test]
fn test_multiple_independent_contracts() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let ms = default_milestones(&env);

    let id1 = client.create_contract(&client_addr, &freelancer_addr, &ms);
    let id2 = client.create_contract(&client_addr, &freelancer_addr, &ms);

    // Accept only id1; id2 remains Created
    client.accept_contract(&id1);

    assert_eq!(client.get_status(&id1), ContractStatus::Accepted);
    assert_eq!(client.get_status(&id2), ContractStatus::Created);

    // Funding id2 (not accepted) must fail
    let result = client.try_deposit_funds(&id2, &600_0000000_i128);
    assert!(result.is_err());

    // Funding id1 (accepted) succeeds
    client.deposit_funds(&id1, &600_0000000_i128);
    assert_eq!(client.get_status(&id1), ContractStatus::Funded);
}

// ── release_milestone ─────────────────────────────────────────────────────────

#[test]
fn test_release_milestone_success() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);
    client.deposit_funds(&id, &600_0000000_i128);

    assert!(client.release_milestone(&id, &0));
    assert!(client.release_milestone(&id, &1));
}

#[test]
fn test_release_milestone_before_funding_fails() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);
    // Not funded yet
    let result = client.try_release_milestone(&id, &0);
    assert!(result.is_err());
}

#[test]
fn test_release_already_released_milestone_fails() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);
    client.deposit_funds(&id, &600_0000000_i128);
    client.release_milestone(&id, &0);

    let result = client.try_release_milestone(&id, &0);
    assert!(result.is_err());
}

#[test]
fn test_release_out_of_range_milestone_fails() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr) = setup(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    client.accept_contract(&id);
    client.deposit_funds(&id, &600_0000000_i128);

    let result = client.try_release_milestone(&id, &99);
    assert!(result.is_err());
}

// ── issue_reputation ──────────────────────────────────────────────────────────

#[test]
fn test_issue_reputation() {
    let env = Env::default();
    let (client, _ca, freelancer_addr) = setup(&env);
    assert!(client.issue_reputation(&freelancer_addr, &5));
}
