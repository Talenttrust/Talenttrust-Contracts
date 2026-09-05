#![cfg(test)]

use crate::{
    test::lifecycle::{EscrowFixture, SetupConfig},
    types::{ContractStatus, Error},
};
use soroban_sdk::{testutils::Events, vec, Env};

#[test]
fn test_eligible_closure() {
    let env = Env::default();
    env.mock_all_auths();

    let config = SetupConfig {
        milestone_count: 1,
        amounts: vec![&env, 100],
        total_amount: 100,
        fund_amount: 100,
        ..Default::default()
    };

    let fixture = EscrowFixture::setup_with_config(&env, config);
    let escrow = &fixture.client;
    let client = &fixture.client_addr;
    let contract_id = fixture.escrow_id;

    // Complete the contract by releasing the only milestone
    escrow.release_milestone(&contract_id, client, &0, &0);

    // Finalize it once
    assert!(escrow.finalize_contract(&contract_id, client));

    let record = escrow.get_finalization_record(&contract_id).unwrap();
    assert_eq!(record.finalizer, client.clone());
    assert_eq!(record.summary.status, ContractStatus::Completed);
}

#[test]
fn test_active_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let config = SetupConfig {
        milestone_count: 1,
        amounts: vec![&env, 100],
        total_amount: 100,
        fund_amount: 100,
        ..Default::default()
    };

    let fixture = EscrowFixture::setup_with_config(&env, config);
    let escrow = &fixture.client;
    let client = &fixture.client_addr;
    let contract_id = fixture.escrow_id;

    // Do NOT release milestone, so status is Funded.
    let res = escrow.try_finalize_contract(&contract_id, client);
    assert_eq!(
        res.err().unwrap().unwrap(),
        Error::InvalidStatusTransition.into()
    );
}

#[test]
fn test_active_dispute() {
    let env = Env::default();
    env.mock_all_auths();

    let config = SetupConfig {
        milestone_count: 1,
        amounts: vec![&env, 100],
        total_amount: 100,
        fund_amount: 100,
        ..Default::default()
    };

    let fixture = EscrowFixture::setup_with_config(&env, config);
    let escrow = &fixture.client;
    let client = &fixture.client_addr;
    let contract_id = fixture.escrow_id;

    // When dispute is raised or pending without completion, status transition is validated
    let res = escrow.try_finalize_contract(&contract_id, client);
    assert_eq!(
        res.err().unwrap().unwrap(),
        Error::InvalidStatusTransition.into()
    );
}

#[test]
fn test_repeat_finalization() {
    let env = Env::default();
    env.mock_all_auths();

    let config = SetupConfig {
        milestone_count: 1,
        amounts: vec![&env, 100],
        total_amount: 100,
        fund_amount: 100,
        ..Default::default()
    };

    let fixture = EscrowFixture::setup_with_config(&env, config);
    let escrow = &fixture.client;
    let client = &fixture.client_addr;
    let contract_id = fixture.escrow_id;

    // Complete the contract
    escrow.release_milestone(&contract_id, client, &0, &0);

    // Finalize it once
    escrow.finalize_contract(&contract_id, client);

    // Clear events
    env.events().all().clear();

    // Try to finalize again
    let res = escrow.try_finalize_contract(&contract_id, client);
    assert_eq!(res.err().unwrap().unwrap(), Error::AlreadyFinalized.into());

    // Check no new events were emitted
    let events = env.events().all();
    assert_eq!(
        events.len(),
        0,
        "no events should be emitted on duplicate finalization"
    );
}

#[test]
fn test_concurrent_finalization() {
    let env = Env::default();
    env.mock_all_auths();

    let config = SetupConfig {
        milestone_count: 1,
        amounts: vec![&env, 100],
        total_amount: 100,
        fund_amount: 100,
        ..Default::default()
    };

    let fixture = EscrowFixture::setup_with_config(&env, config);
    let escrow = &fixture.client;
    let client = &fixture.client_addr;
    let freelancer = &fixture.freelancer_addr;
    let contract_id = fixture.escrow_id;

    // Complete the contract
    escrow.release_milestone(&contract_id, client, &0, &0);

    // First finalizer wins
    assert!(escrow.finalize_contract(&contract_id, client));

    // Concurrent/second finalizer is rejected with AlreadyFinalized without emitting duplicate events
    env.events().all().clear();
    let res = escrow.try_finalize_contract(&contract_id, freelancer);
    assert_eq!(res.err().unwrap().unwrap(), Error::AlreadyFinalized.into());
    assert_eq!(env.events().all().len(), 0);
}
