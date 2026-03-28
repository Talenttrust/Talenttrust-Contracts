use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient};

#[test]
#[should_panic]
fn test_create_contract_panics_when_client_equals_freelancer() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let same_party = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];

    client.create_contract(&same_party, &same_party, &milestones);
}

#[test]
fn test_create_contract_accepts_distinct_client_and_freelancer() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let hiring_party = Address::generate(&env);
    let service_provider = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 250_0000000_i128];

    let id = client.create_contract(&hiring_party, &service_provider, &milestones);
    assert!(id > 0);
}
