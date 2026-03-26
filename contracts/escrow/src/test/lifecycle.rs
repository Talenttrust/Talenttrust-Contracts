extern crate std;

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

use crate::{ContractStatus, Escrow, EscrowClient};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

#[test]
fn hello_round_trips_symbol() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

#[test]
fn create_contract_stores_expected_state() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    let contract = client.get_contract(&id);

    assert_eq!(id, 1);
    assert_eq!(contract.client, client_addr);
    assert_eq!(contract.freelancer, freelancer_addr);
    assert_eq!(contract.total_amount, 1_200_0000000_i128);
    assert_eq!(contract.funded_amount, 0);
    assert_eq!(contract.released_amount, 0);
    assert_eq!(contract.status, ContractStatus::Created);
    assert_eq!(contract.milestones.len(), 3);
}

#[test]
fn deposit_funds_locks_full_total() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 250_i128, 750_i128];

    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    let result = client.deposit_funds(&id, &1_000_i128);
    let contract = client.get_contract(&id);

    assert!(result);
    assert_eq!(contract.funded_amount, 1_000_i128);
    assert_eq!(contract.status, ContractStatus::Funded);
}

#[test]
fn releasing_all_milestones_completes_contract_and_unlocks_reputation() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128, 200_i128];

    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    client.deposit_funds(&id, &300_i128);

    assert!(client.release_milestone(&id, &0));
    let funded_contract = client.get_contract(&id);
    assert_eq!(funded_contract.status, ContractStatus::Funded);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 0);

    assert!(client.release_milestone(&id, &1));
    let completed_contract = client.get_contract(&id);
    assert_eq!(completed_contract.released_amount, 300_i128);
    assert_eq!(completed_contract.status, ContractStatus::Completed);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 1);
}

#[test]
fn issue_reputation_updates_record_and_consumes_credit() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 300_i128];

    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    client.deposit_funds(&id, &300_i128);
    client.release_milestone(&id, &0);

    assert!(client.issue_reputation(&freelancer_addr, &5));

    let reputation = client
        .get_reputation(&freelancer_addr)
        .expect("reputation should exist after issuance");
    assert_eq!(reputation.completed_contracts, 1);
    assert_eq!(reputation.total_rating, 5);
    assert_eq!(reputation.last_rating, 5);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 0);
}
