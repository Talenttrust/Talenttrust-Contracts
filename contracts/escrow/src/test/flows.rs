use super::{complete_contract, create_contract, default_milestones, register_client, total_milestone_amount};
use crate::{EscrowError, ReleaseAuthorization, types::DataKey};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

#[test]
fn multiple_contracts_for_same_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (first_client_addr, freelancer_addr, first_id) = complete_contract(&env, &client);

    let milestones = default_milestones(&env);
    let client_addr = Address::generate(&env);
    let second_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&second_id, &client_addr, &total_milestone_amount()));
    assert!(client.release_milestone(&second_id, &client_addr, &0));
    assert!(client.release_milestone(&second_id, &client_addr, &1));
    assert!(client.release_milestone(&second_id, &client_addr, &2));
    assert!(client.issue_reputation(&first_id, &first_client_addr, &freelancer_addr, &5));
    assert!(client.issue_reputation(&second_id, &client_addr, &freelancer_addr, &4));

    let record = client.get_reputation(&freelancer_addr).unwrap();
    assert_eq!(record.completed_contracts, 2);
    assert_eq!(record.total_rating, 9);
}

#[test]
fn scenario_reputation_invalid_rating_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &0);
    super::assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn scenario_reputation_invalid_rating_six_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, freelancer_addr, contract_id) = complete_contract(&env, &client);

    let result = client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &6);
    super::assert_contract_error(result, EscrowError::InvalidRating);
}

#[test]
fn deposit_funds_emits_structured_deposit_event() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));

    let events = env.events().all();
    assert!(events.iter().any(|event| event.0 == symbol_short!("deposit")));
}

#[test]
fn release_milestone_emits_protocol_fee_event_when_fees_active() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    env.storage()
        .persistent()
        .set(&DataKey::ProtocolFeeBps, &100u32);

    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let events = env.events().all();
    assert!(events.iter().any(|event| event.0 == symbol_short!("protocol_fee")));
}
