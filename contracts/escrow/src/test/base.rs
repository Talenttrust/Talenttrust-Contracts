use soroban_sdk::{testutils::Address as _, vec, Env};

use super::{default_milestones, generated_participants, register_client, world_symbol};

#[test]
fn test_hello() {
    let env = Env::default();
    let client = register_client(&env);
    assert_eq!(client.hello(&world_symbol()), world_symbol());
}

#[test]
fn test_create_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert_eq!(id, 1);
}

#[test]
fn test_deposit_funds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    let result = client.deposit_funds(&id, &super::total_milestone_amount());
    assert!(result);
}

#[test]
fn test_release_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    client.deposit_funds(&id, &super::total_milestone_amount());
    let result = client.release_milestone(&id, &0);
    assert!(result);
}
