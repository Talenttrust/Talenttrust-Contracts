#![cfg(test)]

use crate::{ContractStatus, DataKey, Escrow, EscrowClient, StateV1, StateV2};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

#[test]
fn test_get_state_forward_compatible() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_i128, 2000_i128];

    // Inject legacy StateV1 directly into the persistent storage representing pre-migration ledger data
    let legacy_state = StateV1 {
        client: client_addr.clone(),
        freelancer: freelancer_addr.clone(),
        milestones: milestones.clone(),
    };
    // The environment directly simulates pre-migration environments here safely over contract scopes
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::State, &legacy_state);
    });

    // Execute standard forward-compatible read entrypoint handling standard upgrades natively
    let active_state: StateV2 = client.get_state();

    assert_eq!(active_state.client, client_addr);
    assert_eq!(active_state.freelancer, freelancer_addr);
    assert_eq!(active_state.status, ContractStatus::Created);
}

#[test]
fn test_migrate_state_persistence() {
    let env = Env::default();
    env.mock_all_auths(); // Bypass strict Auth limits during environment test bounds explicitly
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin_caller = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 5000_i128];

    let legacy_state = StateV1 {
        client: client_addr.clone(),
        freelancer: freelancer_addr.clone(),
        milestones: milestones.clone(),
    };

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::State, &legacy_state);
    });

    // Execute migration handling logic validating Auth checks bounds and rewrite loops
    let success = client.migrate_state(&admin_caller);
    assert!(success);

    // Evaluate direct storage retrieval to guarantee memory parsed V2 explicitly onto datakey
    env.as_contract(&contract_id, || {
        let saved_state: StateV2 = env.storage().persistent().get(&DataKey::State).unwrap();
        assert_eq!(saved_state.status, ContractStatus::Created);
    });
}
