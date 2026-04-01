use super::{create_contract, register_client, total_milestone_amount};
use crate::ContractStatus;
use soroban_sdk::{vec, Env};

#[test]
fn deposit_partial_funds_transitions_to_funded_and_persists_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &1_000_0000000_i128));

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.funded_amount, 1_000_0000000_i128);
    assert_eq!(contract.released_amount, 0);
    assert_eq!(contract.status, ContractStatus::Funded);
}

#[test]
fn releasing_all_milestones_completes_contract_and_unlocks_reputation_credit() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = super::generated_participants(&env);
    let milestones = vec![&env, 100_i128, 200_i128];

    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert!(client.deposit_funds(&contract_id, &300_i128));

    assert!(client.release_milestone(&contract_id, &0));
    let funded_contract = client.get_contract(&contract_id);
    assert_eq!(funded_contract.status, ContractStatus::Funded);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 0);

    assert!(client.release_milestone(&contract_id, &1));
    let completed_contract = client.get_contract(&contract_id);
    assert_eq!(completed_contract.released_amount, 300_i128);
    assert_eq!(completed_contract.status, ContractStatus::Completed);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 1);
}

#[test]
fn issue_reputation_updates_record_and_consumes_credit() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (_client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 1);
    assert!(client.issue_reputation(&contract_id, &5));

    let reputation = client
        .get_reputation(&freelancer_addr)
        .expect("reputation should exist after issuance");
    assert_eq!(reputation.completed_contracts, 1);
    assert_eq!(reputation.total_rating, 5);
    assert_eq!(reputation.last_rating, 5);
    assert_eq!(reputation.ratings_count, 1);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 0);
}

#[test]
fn full_funding_matches_total_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &total_milestone_amount()));

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.funded_amount, total_milestone_amount());
    assert_eq!(contract.status, ContractStatus::Funded);
}
