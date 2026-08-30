#![cfg(test)]

use crate::types::{ContractStatus, DataKey, Error, ReleaseAuthorization};
use crate::{Escrow, EscrowClient, MAX_BATCH_MILESTONES};
use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal, Symbol, Vec,
};

fn setup_and_create_escrow<'a>(
    env: &'a Env,
    milestone_amounts: &[i128],
) -> (EscrowClient<'a>, Address, Address, Address, u32) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);

    let admin = Address::generate(env);
    client.initialize(&admin);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);

    let mut milestones = Vec::new(env);
    let mut total_amount = 0i128;
    for &amount in milestone_amounts {
        milestones.push_back(amount);
        total_amount += amount;
    }

    let c_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Deposit full amount
    client.deposit_funds(&c_id, &client_addr, &total_amount);

    (client, admin, client_addr, freelancer_addr, c_id)
}

fn setup_with_protocol_fee<'a>(
    env: &'a Env,
    milestone_amounts: &[i128],
    fee_bps: u32,
) -> (EscrowClient<'a>, Address, Address, Address, u32) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);

    let admin = Address::generate(env);
    client.initialize(&admin);

    // Set protocol fee with admin nonce
    client.set_protocol_fee_bps(&fee_bps, &0);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);

    let mut milestones = Vec::new(env);
    let mut total_amount = 0i128;
    for &amount in milestone_amounts {
        milestones.push_back(amount);
        total_amount += amount;
    }

    let c_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Deposit full amount
    client.deposit_funds(&c_id, &client_addr, &total_amount);

    (client, admin, client_addr, freelancer_addr, c_id)
}

#[test]
fn test_batch_release_empty_batch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    let indices: Vec<u32> = Vec::new(&env);
    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(result.is_err(), "Empty batch must be rejected");
}

#[test]
fn test_batch_release_limit_exceeded_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    let mut indices: Vec<u32> = Vec::new(&env);
    for i in 0..11 {
        indices.push_back(i);
    }

    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(result.is_err(), "Over-limit batch must be rejected");
}

#[test]
fn test_batch_release_maximum_batch_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    // Create contract with exactly MAX_BATCH_MILESTONES
    let mut amounts = Vec::new(&env);
    for _ in 0..MAX_BATCH_MILESTONES {
        amounts.push_back(100i128);
    }
    let amounts_slice: &[i128] = &[100i128; MAX_BATCH_MILESTONES as usize];
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, amounts_slice);

    let mut indices: Vec<u32> = Vec::new(&env);
    for i in 0..MAX_BATCH_MILESTONES {
        indices.push_back(i);
    }

    let success = client.release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(success, "Maximum batch size should succeed");

    // Verify all milestones are released
    let contract_milestones = client.get_milestones(&c_id);
    for i in 0..MAX_BATCH_MILESTONES {
        assert!(contract_milestones.get(i).unwrap().released);
    }
}

#[test]
fn test_batch_release_duplicate_index_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(0);

    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(
        result.is_err(),
        "Duplicate indices in batch must be rejected"
    );

}

#[test]
fn test_batch_release_all_or_nothing_atomicity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    // Release milestone 0 individually first
    assert!(client.release_milestone(&c_id, &client_addr, &0));

    // Try batch with [0, 1] -> index 0 is already released -> entire batch must fail
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(1);

    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(result.is_err(), "Batch containing released item must fail");

    // Verify milestone 1 remains unreleased (atomic rollback / all-or-nothing)
    let contract_milestones = client.get_milestones(&c_id);
    assert!(!contract_milestones.get(1).unwrap().released);

    let contract = client.get_contract(&c_id);
    assert_eq!(contract.released_amount, 100);
}

#[test]
fn test_batch_release_one_invalid_item_refunded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    // Refund milestone 1 individually first
    client.refund_unreleased_milestones(&c_id, &vec![&env, 1]);

    // Try batch with [0, 1] -> index 1 is refunded -> entire batch must fail
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(1);

    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(result.is_err(), "Batch containing refunded item must fail");

    // Verify milestone 0 remains unreleased (atomic rollback)
    let contract_milestones = client.get_milestones(&c_id);
    assert!(!contract_milestones.get(0).unwrap().released);
}

#[test]
fn test_batch_release_one_invalid_item_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    // Try batch with [0, 5] -> index 5 is out of bounds -> entire batch must fail
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(5);

    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(
        result.is_err(),
        "Batch containing out-of-bounds index must fail"
    );

    // Verify milestone 0 remains unreleased (atomic rollback)
    let contract_milestones = client.get_milestones(&c_id);
    assert!(!contract_milestones.get(0).unwrap().released);
}

#[test]
fn test_batch_release_valid_batch_succeeds_and_completes_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    // Release all 3 milestones in a single batch
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(1);
    indices.push_back(2);

    let success = client.release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(success);

    // Verify all milestones are marked released
    let contract_milestones = client.get_milestones(&c_id);
    assert!(contract_milestones.get(0).unwrap().released);
    assert!(contract_milestones.get(1).unwrap().released);
    assert!(contract_milestones.get(2).unwrap().released);

    // Verify contract transitioned to Completed
    let contract = client.get_contract(&c_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, 600);
}

#[test]
fn test_batch_release_with_protocol_fee_accounting() {
    let env = Env::default();
    env.mock_all_auths();

    // Set up with 5% protocol fee (500 bps)
    let fee_bps = 500;
    let (client, admin, client_addr, _, c_id) =
        setup_with_protocol_fee(&env, &[100, 200, 300], fee_bps);

    // Release all milestones in batch
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(1);
    indices.push_back(2);

    let success = client.release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(success);

    // Verify accounting:
    // Total gross = 600
    // Total fee = 600 * 500 / 10000 = 30
    // Total net = 570
    let contract = client.get_contract(&c_id);
    assert_eq!(contract.released_amount, 570);

    // Verify accumulated fees
    let accumulated_fees = client.get_accumulated_protocol_fees();
    assert_eq!(accumulated_fees, 30);

    // Verify accounting invariant: released + refunded + fees <= funded
    let invariant_sum = contract.released_amount + contract.refunded_amount + accumulated_fees;
    assert_eq!(invariant_sum, 600);
    assert_eq!(contract.funded_amount, 600);
}

#[test]
fn test_batch_release_insufficient_funds_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    // Release milestone 0 first (100 released)
    assert!(client.release_milestone(&c_id, &client_addr, &0));

    // Try to release remaining milestones [1, 2] -> requires 500 but only 400 available
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(1);
    indices.push_back(2);

    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(result.is_err(), "Batch with insufficient funds must fail");


    // Verify no state changes occurred
    let contract_milestones = client.get_milestones(&c_id);
    assert!(!contract_milestones.get(1).unwrap().released);
    assert!(!contract_milestones.get(2).unwrap().released);
}

#[test]
fn test_batch_release_authorization_boundaries() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);

    let mut milestones = Vec::new(&env);
    milestones.push_back(100);
    milestones.push_back(200);
    milestones.push_back(300);

    // Create contract with ClientAndArbiter authorization
    let c_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );

    client.deposit_funds(&c_id, &client_addr, &600);

    // Try batch release as freelancer (unauthorized)
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(1);

    let result = client.try_release_milestone_batch(&c_id, &freelancer_addr, &indices);
    assert!(result.is_err(), "Unauthorized role must be rejected");

}

#[test]
fn test_batch_release_accounting_invariant_preserved() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) =
        setup_and_create_escrow(&env, &[100, 200, 300, 400]);

    // Release milestones in two batches
    let mut indices1: Vec<u32> = Vec::new(&env);
    indices1.push_back(0);
    indices1.push_back(1);
    assert!(client.release_milestone_batch(&c_id, &client_addr, &indices1));

    let contract = client.get_contract(&c_id);
    assert_eq!(contract.released_amount, 300);

    let mut indices2: Vec<u32> = Vec::new(&env);
    indices2.push_back(2);
    indices2.push_back(3);
    assert!(client.release_milestone_batch(&c_id, &client_addr, &indices2));

    let contract = client.get_contract(&c_id);
    assert_eq!(contract.released_amount, 1000);
    assert_eq!(contract.funded_amount, 1000);

    // Verify accounting invariant holds
    let invariant_sum = contract.released_amount + contract.refunded_amount;
    assert_eq!(invariant_sum, contract.funded_amount);
}

#[test]
fn test_batch_release_partial_batch_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) =
        setup_and_create_escrow(&env, &[100, 200, 300, 400, 500]);

    // Release only first 2 milestones in batch
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(1);

    let success = client.release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(success);

    let contract_milestones = client.get_milestones(&c_id);
    assert!(contract_milestones.get(0).unwrap().released);
    assert!(contract_milestones.get(1).unwrap().released);
    assert!(!contract_milestones.get(2).unwrap().released);
    assert!(!contract_milestones.get(3).unwrap().released);
    assert!(!contract_milestones.get(4).unwrap().released);

    let contract = client.get_contract(&c_id);
    assert_eq!(contract.released_amount, 300);
    assert_eq!(contract.status, ContractStatus::Funded);
}
