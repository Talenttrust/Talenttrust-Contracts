use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient};

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_is_zero() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);
    let milestones = vec![&env, 0_i128];

    client.create_contract(&hiring_party, &service_provider, &milestones);
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_single_milestone_is_negative() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);
    let milestones = vec![&env, -1_i128];

    client.create_contract(&hiring_party, &service_provider, &milestones);
}

#[test]
#[should_panic]
fn test_create_contract_panics_when_any_milestone_is_non_positive() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 0_i128, 200_0000000_i128];

    client.create_contract(&hiring_party, &service_provider, &milestones);
}

#[test]
fn test_create_contract_accepts_all_positive_milestones() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 1_i128, 999_0000000_i128];

    let id = client.create_contract(&hiring_party, &service_provider, &milestones);
    assert!(id > 0);
}
