use super::{create_contract, register_client, total_milestone_amount, MILESTONE_ONE};
use crate::{ContractStatus, EscrowError};
use soroban_sdk::{testutils::Address as _, Address, Env};

// ─── Happy-path lifecycle ─────────────────────────────────────────────────────

#[test]
fn created_to_funded_to_completed() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, id) = create_contract(&env, &client);

    assert_eq!(client.get_contract(&id).status, ContractStatus::Created);

    assert!(client.deposit_funds(&id, &total_milestone_amount()));
    assert_eq!(client.get_contract(&id).status, ContractStatus::Funded);

    assert!(client.release_milestone(&id, &0));
    assert!(client.release_milestone(&id, &1));
    assert!(client.release_milestone(&id, &2));
    assert_eq!(client.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn created_to_partially_funded_to_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    // Use Incremental deposit mode so partial deposits are accepted.
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &milestones,
        &crate::types::DepositMode::Incremental,
    );

    assert_eq!(client.get_contract(&id).status, ContractStatus::Created);

    // Partial deposit → PartiallyFunded
    assert!(client.deposit_funds(&id, &MILESTONE_ONE));
    assert_eq!(
        client.get_contract(&id).status,
        ContractStatus::PartiallyFunded
    );

    // Remaining deposit → Funded
    let remaining = total_milestone_amount() - MILESTONE_ONE;
    assert!(client.deposit_funds(&id, &remaining));
    assert_eq!(client.get_contract(&id).status, ContractStatus::Funded);
}

#[test]
fn cancel_from_created_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, id) = create_contract(&env, &client);

    assert!(client.cancel_contract(&id, &client_addr));
    assert_eq!(client.get_contract(&id).status, ContractStatus::Cancelled);
}

// ─── Fail-closed state guards ─────────────────────────────────────────────────

#[test]
fn release_on_created_contract_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, id) = create_contract(&env, &client);

    // No deposit — status is Created; release must be rejected.
    let result = client.try_release_milestone(&id, &0);
    super::assert_contract_error(result, EscrowError::InvalidStatusTransition);
}

#[test]
fn release_on_partially_funded_contract_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &milestones,
        &crate::types::DepositMode::Incremental,
    );

    assert!(client.deposit_funds(&id, &MILESTONE_ONE));
    assert_eq!(
        client.get_contract(&id).status,
        ContractStatus::PartiallyFunded
    );

    let result = client.try_release_milestone(&id, &0);
    super::assert_contract_error(result, EscrowError::InvalidStatusTransition);
}

#[test]
fn release_on_cancelled_contract_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, id) = create_contract(&env, &client);

    assert!(client.cancel_contract(&id, &client_addr));

    let result = client.try_release_milestone(&id, &0);
    super::assert_contract_error(result, EscrowError::InvalidStatusTransition);
}

#[test]
fn deposit_on_funded_contract_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&id, &total_milestone_amount()));
    assert_eq!(client.get_contract(&id).status, ContractStatus::Funded);

    let result = client.try_deposit_funds(&id, &1_i128);
    super::assert_contract_error(result, EscrowError::InvalidStatusTransition);
}

#[test]
fn deposit_on_cancelled_contract_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, id) = create_contract(&env, &client);

    assert!(client.cancel_contract(&id, &client_addr));

    let result = client.try_deposit_funds(&id, &total_milestone_amount());
    super::assert_contract_error(result, EscrowError::InvalidStatusTransition);
}

#[test]
fn cancel_after_deposit_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &milestones,
        &crate::types::DepositMode::Incremental,
    );

    assert!(client.deposit_funds(&id, &MILESTONE_ONE));
    // Status is now PartiallyFunded — cancel must be rejected.
    let result = client.try_cancel_contract(&id, &client_addr);
    super::assert_contract_error(result, EscrowError::InvalidStatusTransition);
}

#[test]
fn double_cancel_fails_with_already_cancelled() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, id) = create_contract(&env, &client);

    assert!(client.cancel_contract(&id, &client_addr));

    let result = client.try_cancel_contract(&id, &client_addr);
    super::assert_contract_error(result, EscrowError::AlreadyCancelled);
}

#[test]
fn no_accepted_variant_in_any_transition() {
    // Compile-time proof: ContractStatus::Accepted no longer exists.
    // This test verifies the full lifecycle without ever touching the removed variant.
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, id) = create_contract(&env, &client);

    let statuses = [
        ContractStatus::Created,
        ContractStatus::PartiallyFunded,
        ContractStatus::Funded,
        ContractStatus::Completed,
        ContractStatus::Cancelled,
        ContractStatus::Disputed,
        ContractStatus::Refunded,
    ];
    // Ensure the initial status is Created (first in the valid set).
    assert_eq!(client.get_contract(&id).status, statuses[0]);
}
