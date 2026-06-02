use soroban_sdk::vec;

use super::{
    assert_contract_state, assert_milestone_flags, create_client, create_default_contract, setup,
};
use crate::ContractStatus;

#[test]
fn refunds_selected_unreleased_milestones_and_preserves_remaining_balance() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));
    assert!(client.release_milestone(&contract_id, &0));

    let refund_ids = vec![&env, 1_u32];
    let refunded = client.refund_unreleased_milestones(&contract_id, &refund_ids);
    assert_eq!(refunded, 400_0000000_i128);

    let contract = client.get_contract(&contract_id);
    assert_contract_state(
        contract,
        ContractStatus::Funded,
        1_200_0000000_i128,
        200_0000000_i128,
        400_0000000_i128,
    );
    assert_milestone_flags(client.get_milestones(&contract_id), 1, false, true);
    assert_eq!(
        client.get_refundable_balance(&contract_id),
        600_0000000_i128
    );
}

#[test]
fn marks_contract_refunded_when_all_unreleased_milestones_are_refunded() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));
    let refund_ids = vec![&env, 0_u32, 1_u32, 2_u32];
    let refunded = client.refund_unreleased_milestones(&contract_id, &refund_ids);
    assert_eq!(refunded, 1_200_0000000_i128);

    let contract = client.get_contract(&contract_id);
    assert_contract_state(
        contract,
        ContractStatus::Refunded,
        1_200_0000000_i128,
        0,
        1_200_0000000_i128,
    );
    assert_eq!(client.get_refundable_balance(&contract_id), 0);
}

#[test]
#[should_panic]
fn rejects_empty_refund_request() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    let refund_ids = vec![&env];
    client.refund_unreleased_milestones(&contract_id, &refund_ids);
}

#[test]
#[should_panic]
fn rejects_duplicate_milestones_in_single_refund() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));
    let refund_ids = vec![&env, 1_u32, 1_u32];
    client.refund_unreleased_milestones(&contract_id, &refund_ids);
}

#[test]
#[should_panic]
fn rejects_refunding_released_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));
    assert!(client.release_milestone(&contract_id, &0));

    let refund_ids = vec![&env, 0_u32];
    client.refund_unreleased_milestones(&contract_id, &refund_ids);
}

#[test]
#[should_panic]
fn rejects_refunding_same_milestone_twice() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &1_200_0000000_i128));
    let refund_ids = vec![&env, 2_u32];
    assert_eq!(
        client.refund_unreleased_milestones(&contract_id, &refund_ids),
        600_0000000_i128
    );

    client.refund_unreleased_milestones(&contract_id, &refund_ids);
}

#[test]
#[should_panic]
fn rejects_refund_when_balance_is_not_available() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &200_0000000_i128));
    let refund_ids = vec![&env, 1_u32];
    client.refund_unreleased_milestones(&contract_id, &refund_ids);
}
