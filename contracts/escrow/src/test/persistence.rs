use super::{create_contract, register_client, total_milestone_amount};
use crate::{ContractStatus, EscrowError, ProtocolParameters};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

#[test]
fn contract_state_round_trips_across_lifecycle_mutations() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);
    let created = client.get_contract(&contract_id);
    assert_eq!(created.client, client_addr);
    assert_eq!(created.freelancer, freelancer_addr);
    assert_eq!(created.status, ContractStatus::Created);

    assert!(client.deposit_funds(&contract_id, &1_000_0000000_i128));
    let funded = client.get_contract(&contract_id);
    assert_eq!(funded.status, ContractStatus::Funded);
    assert_eq!(funded.funded_amount, 1_000_0000000_i128);
    assert!(funded.updated_at >= created.updated_at);

    assert!(client.deposit_funds(
        &contract_id,
        &(total_milestone_amount() - 1_000_0000000_i128),
    ));
    assert!(client.release_milestone(&contract_id, &0));

    let after_release = client.get_contract(&contract_id);
    assert_eq!(after_release.released_milestones, 1);
    assert_eq!(after_release.released_amount, super::MILESTONE_ONE);
    assert_eq!(after_release.status, ContractStatus::Funded);
}

#[test]
fn participant_metadata_and_pending_credits_persist_until_reputation_is_issued() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let completed = client.get_contract(&contract_id);
    assert_eq!(completed.client, client_addr);
    assert_eq!(completed.freelancer, freelancer_addr);
    assert_eq!(completed.status, ContractStatus::Completed);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 1);

    assert!(client.issue_reputation(&contract_id, &5));
    let after_rating = client.get_contract(&contract_id);
    assert!(after_rating.reputation_issued);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 0);
}

#[test]
fn governance_and_pause_configuration_persist_in_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let pause_admin = Address::generate(&env);
    assert!(client.initialize(&pause_admin));
    assert_eq!(client.get_admin(), Some(pause_admin));

    let governance_admin = Address::generate(&env);
    assert!(client.initialize_protocol_governance(
        &governance_admin,
        &10_i128,
        &4_u32,
        &2_i128,
        &5_i128,
    ));
    assert_eq!(client.get_governance_admin(), Some(governance_admin));
    assert_eq!(
        client.get_protocol_parameters(),
        ProtocolParameters {
            min_milestone_amount: 10,
            max_milestones: 4,
            min_reputation_rating: 2,
            max_reputation_rating: 5,
        }
    );

    assert!(client.pause());
    assert!(client.is_paused());
}

#[test]
fn try_get_contract_reports_missing_state_without_mutating_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    super::assert_contract_error(client.try_get_contract(&777), EscrowError::ContractNotFound);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 10_i128];
    let created = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert_eq!(created, 1);
}
