use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env,
};

use crate::{ContractStatus, Escrow, EscrowClient, ReleaseAuthorization};

fn setup_funded_contract(env: &Env) -> (EscrowClient<'_>, Address, Address) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);

    (client, client_addr, freelancer_addr)
}

#[test]
fn test_timeout_boundary_allows_approval_at_exact_deadline() {
    let env = Env::default();
    let (client, client_addr, _) = setup_funded_contract(&env);

    let contract = client.get_contract(&1);
    let deadline = contract.milestones.get(0).unwrap().deadline_at;

    env.ledger().with_mut(|li| {
        li.timestamp = deadline;
    });

    let result = client.approve_milestone_release(&1, &client_addr, &0);
    assert!(result);
}

#[test]
#[should_panic(expected = "Milestone deadline has expired; contract moved to Disputed")]
fn test_timeout_expiry_rejects_approval_past_deadline() {
    let env = Env::default();
    let (client, client_addr, _) = setup_funded_contract(&env);

    let contract = client.get_contract(&1);
    let deadline = contract.milestones.get(0).unwrap().deadline_at;

    env.ledger().with_mut(|li| {
        li.timestamp = deadline + 1;
    });

    client.approve_milestone_release(&1, &client_addr, &0);
}

#[test]
fn test_timeout_expiry_transitions_contract_to_disputed() {
    let env = Env::default();
    let (client, _, _) = setup_funded_contract(&env);

    let contract = client.get_contract(&1);
    let deadline = contract.milestones.get(0).unwrap().deadline_at;

    env.ledger().with_mut(|li| {
        li.timestamp = deadline + 1;
    });

    let expired = client.evaluate_milestone_timeout(&1, &0);
    assert!(expired);

    let updated_contract = client.get_contract(&1);
    assert_eq!(updated_contract.status, ContractStatus::Disputed);
}

#[test]
#[should_panic(expected = "Milestone deadline has expired; contract moved to Disputed")]
fn test_timeout_expiry_rejects_release_past_deadline() {
    let env = Env::default();
    let (client, client_addr, _) = setup_funded_contract(&env);

    let contract = client.get_contract(&1);
    let deadline = contract.milestones.get(0).unwrap().deadline_at;

    env.ledger().with_mut(|li| {
        li.timestamp = deadline;
    });
    client.approve_milestone_release(&1, &client_addr, &0);

    env.ledger().with_mut(|li| {
        li.timestamp = deadline + 1;
    });
    client.release_milestone(&1, &client_addr, &0);
}
