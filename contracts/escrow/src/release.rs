use soroban_sdk::vec;

use super::{
    assert_contract_state, assert_milestone_flags, create_client, create_default_contract, setup,
};
use crate::ContractStatus;

#[test]
fn releases_funded_milestones_and_completes_when_all_are_released() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));

    assert!(client.release_milestone(&contract_id, &0));
    let contract = client.get_contract(&contract_id);
    assert_contract_state(
        contract,
        ContractStatus::Funded,
        1_200_0000000_i128,
        200_0000000_i128,
        0,
    );
    assert_milestone_flags(client.get_milestones(&contract_id), 0, true, false);
    assert_eq!(
        client.get_refundable_balance(&contract_id),
        1_000_0000000_i128
    );

    assert!(client.release_milestone(&contract_id, &1));
    assert!(client.release_milestone(&contract_id, &2));

    let contract = client.get_contract(&contract_id);
    assert_contract_state(
        contract,
        ContractStatus::Completed,
        1_200_0000000_i128,
        1_200_0000000_i128,
        0,
    );
    assert_eq!(client.get_refundable_balance(&contract_id), 0);
}

#[test]
#[should_panic]
fn rejects_release_without_sufficient_balance() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &100_0000000_i128));
    client.release_milestone(&contract_id, &0);
}

#[test]
#[should_panic]
fn rejects_release_of_invalid_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));
    client.release_milestone(&contract_id, &3);
}

#[test]
#[should_panic]
fn rejects_releasing_refunded_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));
    let refund_ids = vec![&env, 1_u32];
    client.refund_unreleased_milestones(&contract_id, &refund_ids);

    client.release_milestone(&contract_id, &1);
}

#[test]
#[should_panic]
fn rejects_releasing_same_milestone_twice() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));
    assert!(client.release_milestone(&contract_id, &0));

    client.release_milestone(&contract_id, &0);
}
