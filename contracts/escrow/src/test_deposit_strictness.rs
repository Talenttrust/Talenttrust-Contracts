#![cfg(test)]

use crate::{
    types::{ContractStatus, DepositMode},
    EscrowClient, EscrowError,
};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn setup_env() -> (Env, EscrowClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, crate::Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    (env, client, client_addr, freelancer_addr)
}

#[test]
fn test_exact_total_accepts_exact_amount() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None, // arbiter
        &milestones,
        &None, // terms_hash
        &None, // grace_period
        &DepositMode::ExactTotal,
    );

    let res = client.deposit_funds(&contract_id, &3000);
    assert!(res);

    let data = client.get_contract(&contract_id);
    assert_eq!(data.status, ContractStatus::Funded);
    assert_eq!(data.total_deposited, 3000);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_exact_total_rejects_partial_amount() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::ExactTotal,
    );

    // Deposit 1000, should fail because ExactDepositRequired = 11
    client.deposit_funds(&contract_id, &1000);
}

#[test]
fn test_incremental_accepts_multiple_deposits() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::Incremental,
    );

    // First deposit 1000
    let res = client.deposit_funds(&contract_id, &1000);
    assert!(res);

    let data_partial = client.get_contract(&contract_id);
    assert_eq!(data_partial.status, ContractStatus::PartiallyFunded);
    assert_eq!(data_partial.total_deposited, 1000);

    // Second deposit 2000
    let res2 = client.deposit_funds(&contract_id, &2000);
    assert!(res2);

    let data_funded = client.get_contract(&contract_id);
    assert_eq!(data_funded.status, ContractStatus::Funded);
    assert_eq!(data_funded.total_deposited, 3000);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_incremental_rejects_overflow() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::Incremental,
    );

    // Try to deposit 4000, should fail with DepositWouldExceedTotal = 12
    client.deposit_funds(&contract_id, &4000);
}
