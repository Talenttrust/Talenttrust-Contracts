use super::{assert_contract_state, create_client, create_default_contract, setup};
use crate::ContractStatus;

/// Tests that deposits accumulate correctly and transition to Funded status when fully funded.
/// 
/// # Security
/// - Validates state transition from Created to Funded
/// - Ensures funded_amount tracking is accurate
#[test]
fn accumulates_deposits_without_exceeding_total() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &600_0000000_i128));
    let contract = client.get_contract(&contract_id);
    assert_contract_state(contract, ContractStatus::Created, 600_0000000_i128, 0, 0);

    assert!(client.deposit_funds(&contract_id, &client_addr, &600_0000000_i128));
    let contract = client.get_contract(&contract_id);
    assert_contract_state(contract, ContractStatus::Funded, 1_200_0000000_i128, 0, 0);
}

/// Tests that zero-amount deposits are rejected.
/// 
/// # Security
/// - Prevents dust attacks and invalid state transitions
#[test]
#[should_panic]
fn rejects_zero_deposit() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    client.deposit_funds(&contract_id, &client_addr, &0_i128);
}

/// Tests that deposits exceeding the total milestone amount are rejected.
/// 
/// # Security
/// - Prevents overfunding attacks
/// - Ensures contract accounting integrity
#[test]
#[should_panic]
fn rejects_overfunding() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    client.deposit_funds(&contract_id, &client_addr, &1_300_0000000_i128);
}

/// Tests that deposits are rejected after contract is fully refunded.
/// 
/// # Security
/// - Validates fail-closed state machine
/// - Prevents re-funding of resolved contracts
#[test]
#[should_panic]
fn rejects_deposit_after_full_refund_resolution() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &1_200_0000000_i128));
    let refund_ids = soroban_sdk::vec![&env, 0_u32, 1_u32, 2_u32];
    let refunded = client.refund_unreleased_milestones(&contract_id, &refund_ids);
    assert_eq!(refunded, 1_200_0000000_i128);

    client.deposit_funds(&contract_id, &client_addr, &1_i128);
}
