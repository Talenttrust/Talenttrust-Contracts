#![cfg(test)]

use crate::types::{DataKey, Error, ReleaseAuthorization};
use crate::{Escrow, EscrowClient};
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

#[test]
fn test_milestone_settlement_succeeds_first_time() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, client_addr, _freelancer_addr, c_id) =
        setup_and_create_escrow(&env, &[1_000, 2_000]);

    // First release of milestone 0
    let res = client.release_milestone(&c_id, &client_addr, &0);
    assert!(res);

    let summary = client.get_contract(&c_id);
    assert_eq!(summary.released_amount, 1_000);
}

#[test]
fn test_milestone_settlement_rejects_second_settlement_double_spend() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, client_addr, _freelancer_addr, c_id) =
        setup_and_create_escrow(&env, &[1_000, 2_000]);

    // First release succeeds
    assert!(client.release_milestone(&c_id, &client_addr, &0));

    // Second release of identical milestone 0 must fail
    let res = client.try_release_milestone(&c_id, &client_addr, &0);
    assert!(res.is_err());

    // Ensure released amount is not mutated
    let summary = client.get_contract(&c_id);
    assert_eq!(summary.released_amount, 1_000);
}

#[test]
fn test_milestone_settlement_unrelated_milestones_unaffected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, client_addr, _freelancer_addr, c_id) =
        setup_and_create_escrow(&env, &[1_000, 2_000]);

    // Release milestone 0
    assert!(client.release_milestone(&c_id, &client_addr, &0));

    // Milestone 1 can still be released independently
    assert!(client.release_milestone(&c_id, &client_addr, &1));

    let summary = client.get_contract(&c_id);
    assert_eq!(summary.released_amount, 3_000);
}

#[test]
fn test_milestone_settlement_different_contracts_isolated() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, client_addr1, freelancer_addr1, c_id1) =
        setup_and_create_escrow(&env, &[5_000]);

    let mut milestones2 = Vec::new(&env);
    milestones2.push_back(5_000i128);
    let client_addr2 = Address::generate(&env);
    let freelancer_addr2 = Address::generate(&env);

    let c_id2 = client.create_contract(
        &client_addr2,
        &freelancer_addr2,
        &None,
        &milestones2,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&c_id2, &client_addr2, &5_000);

    // Release milestone on contract 1
    assert!(client.release_milestone(&c_id1, &client_addr1, &0));

    // Release milestone on contract 2 is completely unaffected and succeeds
    assert!(client.release_milestone(&c_id2, &client_addr2, &0));

    assert_eq!(client.get_contract(&c_id1).released_amount, 5_000);
    assert_eq!(client.get_contract(&c_id2).released_amount, 5_000);
}
