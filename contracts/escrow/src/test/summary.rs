//! # Contract Summary Tests
//!
//! Verifies `get_contract_summary` across the full escrow lifecycle:
//! - Created, Funded, partial release, full release (Completed), Cancelled
//! - Field correctness: roles, status, financial totals, milestone flags
//! - Schema version stability
//! - Error on unknown contract

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{ContractStatus, Escrow, EscrowClient, EscrowError, CONTRACT_SUMMARY_SCHEMA_VERSION};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

fn generate_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

fn default_milestones(env: &Env) -> soroban_sdk::Vec<i128> {
    vec![env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128]
}

fn total_amount() -> i128 {
    200_0000000 + 400_0000000 + 600_0000000
}

// ---------------------------------------------------------------------------
// Schema / versioning
// ---------------------------------------------------------------------------

/// The schema_version field must always equal CONTRACT_SUMMARY_SCHEMA_VERSION.
#[test]
fn summary_schema_version_is_stable() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.schema_version, CONTRACT_SUMMARY_SCHEMA_VERSION);
    assert_eq!(summary.schema_version, 1u32);
}

// ---------------------------------------------------------------------------
// Roles
// ---------------------------------------------------------------------------

/// Roles are surfaced correctly without an arbiter.
#[test]
fn summary_roles_without_arbiter() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.client, client_addr);
    assert_eq!(summary.freelancer, freelancer_addr);
    assert!(summary.arbiter.is_none());
}

/// Roles are surfaced correctly when an arbiter is present.
#[test]
fn summary_roles_with_arbiter() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);
    let arbiter_addr = Address::generate(&env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &default_milestones(&env),
    );

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.client, client_addr);
    assert_eq!(summary.freelancer, freelancer_addr);
    assert_eq!(summary.arbiter, Some(arbiter_addr));
}

// ---------------------------------------------------------------------------
// Created state
// ---------------------------------------------------------------------------

/// Summary immediately after creation reflects Created status and zero balances.
#[test]
fn summary_at_created_state() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.status, ContractStatus::Created);
    assert_eq!(summary.total_amount, total_amount());
    assert_eq!(summary.funded_amount, 0);
    assert_eq!(summary.released_amount, 0);
    assert_eq!(summary.refundable_balance, 0);
    assert_eq!(summary.released_milestone_count, 0);
    assert!(!summary.reputation_issued);
}

/// All milestone flags are false immediately after creation.
#[test]
fn summary_milestones_all_pending_at_creation() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.milestones.len(), 3);
    for m in summary.milestones.iter() {
        assert!(!m.released, "milestone {} should not be released", m.index);
        assert!(!m.refunded, "milestone {} should not be refunded", m.index);
    }
}

/// Milestone indices are zero-based and sequential.
#[test]
fn summary_milestone_indices_are_sequential() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));

    let summary = client.get_contract_summary(&contract_id);
    for (pos, m) in summary.milestones.iter().enumerate() {
        assert_eq!(m.index, pos as u32);
    }
}

/// Milestone amounts in the summary match the amounts passed to create_contract.
#[test]
fn summary_milestone_amounts_match_creation() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));

    let summary = client.get_contract_summary(&contract_id);
    let amounts: soroban_sdk::Vec<i128> = summary.milestones.iter().map(|m| m.amount).collect();
    assert_eq!(amounts.get(0).unwrap(), 200_0000000_i128);
    assert_eq!(amounts.get(1).unwrap(), 400_0000000_i128);
    assert_eq!(amounts.get(2).unwrap(), 600_0000000_i128);
}

// ---------------------------------------------------------------------------
// Funded state
// ---------------------------------------------------------------------------

/// After a full deposit the summary reflects Funded status and the deposited amount.
#[test]
fn summary_at_funded_state() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_amount());

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.status, ContractStatus::Funded);
    assert_eq!(summary.funded_amount, total_amount());
    assert_eq!(summary.released_amount, 0);
    assert_eq!(summary.released_milestone_count, 0);
}

// ---------------------------------------------------------------------------
// Partial release
// ---------------------------------------------------------------------------

/// After releasing one milestone the summary correctly marks it released.
#[test]
fn summary_after_first_milestone_release() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_amount());
    client.release_milestone(&contract_id, &0);

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.released_milestone_count, 1);
    assert_eq!(summary.released_amount, 200_0000000_i128);
    assert!(summary.milestones.get(0).unwrap().released);
    assert!(!summary.milestones.get(1).unwrap().released);
    assert!(!summary.milestones.get(2).unwrap().released);
}

/// released_milestone_count increments correctly with each successive release.
#[test]
fn summary_released_milestone_count_increments() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_amount());

    client.release_milestone(&contract_id, &0);
    assert_eq!(
        client.get_contract_summary(&contract_id).released_milestone_count,
        1
    );

    client.release_milestone(&contract_id, &1);
    assert_eq!(
        client.get_contract_summary(&contract_id).released_milestone_count,
        2
    );
}

// ---------------------------------------------------------------------------
// Completed state
// ---------------------------------------------------------------------------

/// After all milestones are released the summary reflects Completed status.
#[test]
fn summary_at_completed_state() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_amount());
    client.release_milestone(&contract_id, &0);
    client.release_milestone(&contract_id, &1);
    client.release_milestone(&contract_id, &2);

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.status, ContractStatus::Completed);
    assert_eq!(summary.released_amount, total_amount());
    assert_eq!(summary.released_milestone_count, 3);

    for m in summary.milestones.iter() {
        assert!(m.released, "milestone {} should be released", m.index);
    }
}

/// reputation_issued flips to true after issue_reputation is called.
#[test]
fn summary_reputation_issued_flag() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_amount());
    client.release_milestone(&contract_id, &0);
    client.release_milestone(&contract_id, &1);
    client.release_milestone(&contract_id, &2);

    let before = client.get_contract_summary(&contract_id);
    assert!(!before.reputation_issued);

    client.issue_reputation(&contract_id, &5);

    let after = client.get_contract_summary(&contract_id);
    assert!(after.reputation_issued);
}

// ---------------------------------------------------------------------------
// Cancelled state
// ---------------------------------------------------------------------------

/// After cancellation the summary reflects Cancelled status.
#[test]
fn summary_at_cancelled_state() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));
    client.cancel_contract(&contract_id, &client_addr);

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.status, ContractStatus::Cancelled);
    assert_eq!(summary.released_milestone_count, 0);
    assert_eq!(summary.released_amount, 0);
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

/// Calling get_contract_summary for a non-existent contract must fail.
#[test]
fn summary_fails_for_unknown_contract_id() {
    let env = Env::default();
    let client = register_client(&env);

    let result = client.try_get_contract_summary(&999);
    assert!(
        result.is_err(),
        "expected an error for a non-existent contract"
    );
    assert_eq!(result, Err(Ok(EscrowError::ContractNotFound)));
}

// ---------------------------------------------------------------------------
// Lifecycle consistency
// ---------------------------------------------------------------------------

/// Summary totals must be internally consistent throughout the lifecycle:
/// released_amount <= funded_amount <= total_amount.
#[test]
fn summary_totals_are_consistent_across_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generate_participants(&env);

    let contract_id =
        client.create_contract(&client_addr, &freelancer_addr, &None, &default_milestones(&env));

    // Created
    let s = client.get_contract_summary(&contract_id);
    assert!(s.released_amount <= s.funded_amount);
    assert!(s.funded_amount <= s.total_amount);

    // Funded
    client.deposit_funds(&contract_id, &total_amount());
    let s = client.get_contract_summary(&contract_id);
    assert!(s.released_amount <= s.funded_amount);
    assert!(s.funded_amount <= s.total_amount);

    // First milestone released
    client.release_milestone(&contract_id, &0);
    let s = client.get_contract_summary(&contract_id);
    assert!(s.released_amount <= s.funded_amount);
    assert!(s.funded_amount <= s.total_amount);

    // Fully released
    client.release_milestone(&contract_id, &1);
    client.release_milestone(&contract_id, &2);
    let s = client.get_contract_summary(&contract_id);
    assert_eq!(s.released_amount, s.funded_amount);
    assert_eq!(s.funded_amount, s.total_amount);
}

/// Multiple independent contracts each have their own isolated summary.
#[test]
fn summary_is_isolated_per_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr1, freelancer_addr1) = generate_participants(&env);
    let (client_addr2, freelancer_addr2) = generate_participants(&env);

    let id1 = client.create_contract(
        &client_addr1,
        &freelancer_addr1,
        &None,
        &default_milestones(&env),
    );
    let id2 = client.create_contract(
        &client_addr2,
        &freelancer_addr2,
        &None,
        &vec![&env, 100_i128, 200_i128],
    );

    client.deposit_funds(&id1, &total_amount());
    client.release_milestone(&id1, &0);

    let s1 = client.get_contract_summary(&id1);
    let s2 = client.get_contract_summary(&id2);

    assert_eq!(s1.status, ContractStatus::Funded);
    assert_eq!(s1.released_milestone_count, 1);
    assert_eq!(s2.status, ContractStatus::Created);
    assert_eq!(s2.released_milestone_count, 0);
}
