use super::{assert_contract_state, create_client, create_default_contract, setup};
use crate::ContractStatus;

#[test]
fn accumulates_deposits_without_exceeding_total() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &600_0000000_i128));
    let contract = client.get_contract(&contract_id);
    assert_contract_state(contract, ContractStatus::Created, 600_0000000_i128, 0, 0);

    assert!(client.deposit_funds(&contract_id, &600_0000000_i128));
    let contract = client.get_contract(&contract_id);
    assert_contract_state(contract, ContractStatus::Funded, 1_200_0000000_i128, 0, 0);
}

#[test]
#[should_panic]
fn rejects_zero_deposit() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    client.deposit_funds(&contract_id, &0_i128);
}

#[test]
#[should_panic]
fn rejects_overfunding() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    client.deposit_funds(&contract_id, &1_300_0000000_i128);
}

#[test]
#[should_panic]
fn rejects_deposit_after_full_refund_resolution() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));
    let refund_ids = soroban_sdk::vec![&env, 0_u32, 1_u32, 2_u32];
    let refunded = client.refund_unreleased_milestones(&contract_id, &refund_ids);
    assert_eq!(refunded, 1_200_0000000_i128);

    client.deposit_funds(&contract_id, &1_i128);
}
