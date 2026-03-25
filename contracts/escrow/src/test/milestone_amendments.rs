#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    vec, Address, Env,
};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

#[test]
fn test_amendment_full_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000, 2000];
    
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &ReleaseAuthorization::ClientOnly);
    client.deposit_funds(&id);

    // Freelancer proposes amendment to milestone 0 (1000 -> 1500)
    client.propose_milestone_amendment(&id, &0, &freelancer_addr, &1500);
    
    // Check amendment exists
    let amendment = client.get_amendment(&id, &0).unwrap();
    assert_eq!(amendment.new_amount, 1500);
    assert_eq!(amendment.proposer, freelancer_addr);

    // Client approves
    client.approve_milestone_amendment(&id, &0, &client_addr);

    // Verify milestone updated
    let contract = client.get_contract(&id);
    assert_eq!(contract.milestones.get(0).unwrap().amount, 1500);
    
    // Verify amendment cleared
    assert!(client.get_amendment(&id, &0).is_none());
}

#[test]
fn test_amendment_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000];
    
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &ReleaseAuthorization::ClientOnly);
    client.deposit_funds(&id);

    // Client proposes amendment
    client.propose_milestone_amendment(&id, &0, &client_addr, &800);
    
    // Freelancer rejects
    client.reject_milestone_amendment(&id, &0, &freelancer_addr);

    // Verify amendment cleared and amount unchanged
    assert!(client.get_amendment(&id, &0).is_none());
    let contract = client.get_contract(&id);
    assert_eq!(contract.milestones.get(0).unwrap().amount, 1000);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_amendment_propose_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let unauthorized_addr = Address::generate(&env);
    let milestones = vec![&env, 1000, 2000];
    
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &ReleaseAuthorization::ClientOnly);
    client.deposit_funds(&id);

    // Stranger tries to propose amendment
    client.propose_milestone_amendment(&id, &0, &unauthorized_addr, &500);
}

#[test]
#[should_panic(expected = "Proposer cannot approve their own amendment")]
fn test_amendment_self_approval_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000];
    
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &ReleaseAuthorization::ClientOnly);
    client.deposit_funds(&id);

    // Client proposes
    client.propose_milestone_amendment(&id, &0, &client_addr, &800);
    
    // Client tries to approve their own
    client.approve_milestone_amendment(&id, &0, &client_addr);
}

#[test]
#[should_panic(expected = "Cannot amend a released milestone")]
fn test_amendment_already_released_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000, 2000];
    
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &ReleaseAuthorization::ClientOnly);
    client.deposit_funds(&id);

    // Release first milestone. Contract remains Funded because of the second milestone.
    client.approve_milestone(&id, &0, &client_addr);
    client.release_milestone(&id, &0);

    // Try to propose amendment to the released milestone
    client.propose_milestone_amendment(&id, &0, &client_addr, &1200);
}
