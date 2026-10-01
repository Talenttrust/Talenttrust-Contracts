#![cfg(test)]

use crates{
    types{ContractStatus, DepositMode},
    EscrowClient, EscrowError,
};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn setup_env() -> (Env, EscrowClient<'static>, Address, Address) {
    let env = Env.default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, crate::Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    (env, client, client_addr, freelancer_addr)
}

// -----------------------------------------------------------------------------
// ExactTotal deposit mode
// -----------------------------------------------------------------------------

#[test]
fn test_exact_total_accepts_exact_amount() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None, // arbiter
        &milestones,
        &None, // terms_hash
        &None, // grace_period
        &DepositMode::ExactTotal,
    );

    let res = client.deposit_funds(&contract_id, &3000);
    assert!(res);

    let data = client.get_contract(&contract_id);
    assert_eq!(data.status, ContractStatus::Funded);
    assert_eq!(data.total_deposited, 3000);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_exact_total_rejects_partial_amount() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::ExactTotal,
    );

    // Deposit 1000, should fail because ExactDepositRequired = 11
    client.deposit_funds(&contract_id, &1000);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_exact_total_rejects_overpayment() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::ExactTotal,
    );

    // Overpayment must be rejected with the same exact-deposit error.
    client.deposit_funds(&contract_id, &3001);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_exact_total_rejects_zero_amount() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::ExactTotal,
    );

    // Zero deposit must not move the contract into a funded state.
    client.deposit_funds(&contract_id, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_exact_total_rejects_duplicate_deposit() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::ExactTotal,
    );

    client.deposit_funds(&contract_id, &3000);
    // A duplicate deposit after funding must be rejected and not double-count.
    client.deposit_funds(&contract_id, &3000);
}

// -----------------------------------------------------------------------------
// Incremental deposit mode
// -----------------------------------------------------------------------------

#[test]
fn test_incremental_accepts_multiple_deposits() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::Incremental,
    );

    // First deposit 1000
    let res = client.deposit_funds(&contract_id, &1000);
    assert!(res);

    let data_partial = client.get_contract(&contract_id);
    assert_eq!(data_partial.status, ContractStatus::PartiallyFunded);
    assert_eq!(data_partial.total_deposited, 1000);

    // Second deposit 2000
    let res2 = client.deposit_funds(&contract_id, &2000);
    assert!(res2);

    let data_funded = client.get_contract(&contract_id);
    assert_eq!(data_funded.status, ContractStatus::Funded);
    assert_eq!(data_funded.total_deposited, 3000);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_incremental_rejects_overflow() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::Incremental,
    );

    // Try to deposit 4000, should fail with DepositWouldExceedTotal = 12
    client.deposit_funds(&contract_id, &4000);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_incremental_rejects_cumulative_overflow() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 2000]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::Incremental,
    );

    // Cumulative overflow must be rejected and leave the recorded total unchanged.
    client.deposit_funds(&contract_id, &2000);
    client.deposit_funds(&contract_id, &2000);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_incremental_rejects_deposit_after_funded() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 200]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::Incremental,
    );

    client.deposit_funds(&contract_id, &3000);
    // Any further deposit after full funding must be rejected.
    client.deposit_funds(&contract_id, &1);
}

// -----------------------------------------------------------------------------
// Cross-mode compatibility contracts
// -----------------------------------------------------------------------------

#[test]
fn test_incremental_exact_total_leaves_status_untouched_on_rejection() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 200]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::Incremental,
    );

    // A valid partial deposit must persist even when a later deposit is rejected.
    assert!(client.deposit_funds(&contract_id, &1000));
    let partial = client.get_contract(&contract_id);
    assert_eq!(partial.status, ContractStatus::PartiallyFunded);
    assert_eq!(partial.total_deposited, 1000);

    // This overflow is rejected and must not corrupt the persisted state.
    let result = client.try_deposit_funds(&contract_id, &3000);
    assert_eq!(result, Err(Oc.from(EscrowError::DepositWouldExceedTotal)));

    let after = client.get_contract(&contract_id);
    assert_eq!(after.status, ContractStatus::PartiallyFunded);
    assert_eq!(after.total_deposited, 1000);
}

#[test]
fn test_exact_total_leaves_status_untouched_on_rejection() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, [1000, 200]); // Total 3000

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::ExactTotal,
    );

    // A mismatched deposit must not partially fund the contract.
    let result = client.try_deposit_funds(&contract_id, &1000);
    assert_eq!(result, Err(Oc.from(EscrowError::ExactDepositRequired)));

    let after = client.get_contract(&contract_id);
    assert_eq!(after.status, ContractStatus::Created);
    assert_eq!(after.total_deposited, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_exact_total_rejects_deposit_on_empty_milestones() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, []);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::ExactTotal,
    );

    // An empty milestone set has zero total, so any non-zero deposit is a mismatch.
    client.deposit_funds(&contract_id, &1);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_incremental_rejects_deposit_on_empty_milestones() {
    let (env, client, client_addr, freelancer_addr) = setup_env();

    let milestones = Vec::from_array(&env, []);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
        &DepositMode::Incremental,
    );

    // Incremental deposits are capped by the total, which is zero for empty milestones.
    client.deposit_funds(&contract_id, &1);
}
