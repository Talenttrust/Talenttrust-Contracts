use super::{create_client, create_default_contract, register_client, setup, setup_env};
use crate::{
    ttl::{PERSISTENT_BUMP_THRESHOLD, PERSISTENT_MAX_TTL_LEDGERS},
    ContractStatus, ReleaseAuthorization,
};
use soroban_sdk::{testutils::Address as _, testutils::Ledger, vec, Address, Env};

// ============================================================================
// ORIGINAL PERSISTENCE TESTS (preserved from before TTL work)
// ============================================================================

#[test]
fn contract_state_round_trips_across_lifecycle_mutations() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    let created = client.get_contract(&contract_id);
    assert_eq!(created.client, client_addr);
    assert_eq!(created.freelancer, freelancer_addr);
    assert_eq!(created.status, ContractStatus::Created);

    // Partial deposit
    assert!(client.deposit_funds(&contract_id, &client_addr, &600_0000000_i128));
    let funded = client.get_contract(&contract_id);
    assert_eq!(funded.status, ContractStatus::Created);
    assert_eq!(funded.funded_amount, 600_0000000_i128);

    // Full deposit transitions to Funded
    assert!(client.deposit_funds(&contract_id, &client_addr, &600_0000000_i128));
    let fully_funded = client.get_contract(&contract_id);
    assert_eq!(fully_funded.status, ContractStatus::Funded);
    assert_eq!(fully_funded.funded_amount, 1_200_0000000_i128);

    // Approve and release first milestone
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    let after_release = client.get_contract(&contract_id);
    assert_eq!(after_release.released_amount, 200_0000000_i128);
    assert_eq!(after_release.status, ContractStatus::Funded);
}

#[test]
fn try_get_contract_reports_missing_state_without_mutating_storage() {
    let env = setup_env();
    let client = register_client(&env);

    // Non-existent contract should panic
    let result = client.try_get_contract(&777);
    assert!(result.is_err());

    // Create a real contract
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 10_i128];
    let created = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(created, 1);
}

// ============================================================================
// PERSISTENT TTL TESTS
// ============================================================================

/// Verifies that create_contract extends TTL on the new contract entry.
/// After creation, the contract should be accessible even after advancing
/// the ledger close to the bump threshold.
#[test]
fn persistent_contract_ttl_extended_on_create() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Advance ledger close to bump threshold (but not past max TTL)
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Contract should still be accessible — TTL was extended on create
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.client, client_addr);
    assert_eq!(contract.status, ContractStatus::Created);
}

/// Verifies that deposit_funds extends TTL on both contract and milestone entries.
#[test]
fn persistent_contract_ttl_extended_on_deposit() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &100_0000000_i128));

    // Advance ledger significantly
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Contract should still be accessible — TTL was extended on deposit
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.funded_amount, 100_0000000_i128);
}

/// Verifies that release_milestone extends TTL on both contract and milestone entries.
#[test]
fn persistent_contract_ttl_extended_on_release() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &1_200_0000000_i128));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    // Advance ledger significantly
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Contract should still be accessible — TTL was extended on release
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.released_amount, 200_0000000_i128);
}

/// Verifies that refund_unreleased_milestones extends TTL on both entries.
#[test]
fn persistent_contract_ttl_extended_on_refund() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &1_200_0000000_i128));

    let refund_indices = vec![&env, 0_u32];
    let refunded = client.refund_unreleased_milestones(&contract_id, &refund_indices);
    assert!(refunded > 0);

    // Advance ledger significantly
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Contract should still be accessible — TTL was extended on refund
    let contract = client.get_contract(&contract_id);
    assert!(contract.refunded_amount > 0);
}

/// Verifies that get_milestones extends TTL on the milestone entry.
#[test]
fn persistent_milestone_ttl_extended_on_read() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    // Read milestones — this should extend TTL
    let milestones = client.get_milestones(&contract_id);
    assert_eq!(milestones.len(), 3);

    // Advance ledger significantly
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Milestones should still be accessible — TTL was extended on read
    let milestones_after = client.get_milestones(&contract_id);
    assert_eq!(milestones_after.len(), 3);
}

/// Verifies that get_refundable_balance extends TTL on the contract entry.
#[test]
fn persistent_ttl_extended_on_get_refundable_balance() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &1_200_0000000_i128));

    let balance = client.get_refundable_balance(&contract_id);
    assert_eq!(balance, 1_200_0000000_i128);

    // Advance ledger significantly
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Balance check should still work — TTL was extended
    let balance_after = client.get_refundable_balance(&contract_id);
    assert_eq!(balance_after, 1_200_0000000_i128);
}

/// Verifies that approve_milestone_release extends TTL on contract and milestone entries.
#[test]
fn persistent_ttl_extended_on_approve_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &1_200_0000000_i128));

    // Approve milestone — reads contract and milestones, extending TTL
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));

    // Advance ledger significantly
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Contract should still be accessible — TTL was extended on approval
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Funded);
}

/// Verifies that the NextContractId counter TTL is extended on create,
/// so sequential contract creation works even after ledger advances.
#[test]
fn next_contract_id_ttl_extended_on_create() {
    let env = setup_env();
    let client = register_client(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];

    // Create first contract
    let id1 = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id1, 1);

    // Advance ledger significantly
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Create second contract — NextContractId must still be accessible
    let id2 = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id2, 2);

    // Both contracts should be accessible
    assert_eq!(client.get_contract(&id1).status, ContractStatus::Created);
    assert_eq!(client.get_contract(&id2).status, ContractStatus::Created);
}

/// Invariant: any contract touched within its TTL window remains live.
/// Simulates a long-running contract with periodic access across multiple
/// TTL cycles, verifying fund-state is never lost.
#[test]
fn persistent_ttl_prevents_fund_state_loss_in_long_running_contract() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &1_200_0000000_i128));
    let initial_funded = client.get_contract(&contract_id).funded_amount;

    // Simulate periodic access across multiple bump-threshold cycles.
    // Each get_contract call extends TTL, preventing eviction.
    for cycle in 1_u32..=3 {
        env.ledger().set_sequence_number(cycle * (PERSISTENT_BUMP_THRESHOLD / 2));
        let contract = client.get_contract(&contract_id);
        // Fund state must be preserved across all cycles
        assert_eq!(contract.funded_amount, initial_funded);
        assert_eq!(contract.status, ContractStatus::Funded);
    }

    // Release a milestone after simulated long time — accounting must be correct
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let final_contract = client.get_contract(&contract_id);
    assert_eq!(final_contract.funded_amount, initial_funded);
    assert_eq!(final_contract.released_amount, 200_0000000_i128);
}

/// Verifies the complete lifecycle with ledger advances between each stage,
/// confirming TTL extension at every mutating step keeps all data live.
#[test]
fn persistent_ttl_survives_complete_lifecycle() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    // Stage 1: Deposit (advances ~5.7 days)
    env.ledger().set_sequence_number(100_000);
    assert!(client.deposit_funds(&contract_id, &client_addr, &1_200_0000000_i128));

    // Stage 2: First release (~28.9 days total)
    env.ledger().set_sequence_number(500_000);
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    // Stage 3: Second release (~57.8 days total)
    env.ledger().set_sequence_number(1_000_000);
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));

    // Stage 4: Final release (~115.7 days total)
    env.ledger().set_sequence_number(2_000_000);
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));
    assert!(client.release_milestone(&contract_id, &client_addr, &2));

    // Verify final state is correct and all accounting is intact
    let final_contract = client.get_contract(&contract_id);
    assert_eq!(final_contract.status, ContractStatus::Completed);
    assert_eq!(final_contract.released_amount, 1_200_0000000_i128);
    assert_eq!(final_contract.funded_amount, 1_200_0000000_i128);
    assert_eq!(final_contract.refunded_amount, 0);
}

/// Verifies that TTL constants are within Soroban's documented limits.
/// PERSISTENT_MAX_TTL_LEDGERS must not exceed the network's max TTL.
/// This is a compile-time sanity check expressed as a runtime assertion.
#[test]
fn ttl_constants_are_within_soroban_limits() {
    // Soroban mainnet max persistent TTL is ~1 year (6,312,000 ledgers at 5s each)
    const SOROBAN_MAX_PERSISTENT_TTL: u32 = 6_312_000;
    assert!(
        PERSISTENT_MAX_TTL_LEDGERS <= SOROBAN_MAX_PERSISTENT_TTL,
        "PERSISTENT_MAX_TTL_LEDGERS ({}) exceeds Soroban network limit ({})",
        PERSISTENT_MAX_TTL_LEDGERS,
        SOROBAN_MAX_PERSISTENT_TTL
    );
    // Bump threshold must be less than max TTL
    assert!(
        PERSISTENT_BUMP_THRESHOLD < PERSISTENT_MAX_TTL_LEDGERS,
        "PERSISTENT_BUMP_THRESHOLD ({}) must be less than PERSISTENT_MAX_TTL_LEDGERS ({})",
        PERSISTENT_BUMP_THRESHOLD,
        PERSISTENT_MAX_TTL_LEDGERS
    );
    // Threshold should be at least 25% of max TTL to avoid excessive bumping
    assert!(
        PERSISTENT_BUMP_THRESHOLD >= PERSISTENT_MAX_TTL_LEDGERS / 4,
        "PERSISTENT_BUMP_THRESHOLD ({}) is too low relative to max TTL ({})",
        PERSISTENT_BUMP_THRESHOLD,
        PERSISTENT_MAX_TTL_LEDGERS
    );
}

/// Verifies that multiple contracts have independent TTL lifetimes.
/// Touching contract A should not affect contract B's TTL.
#[test]
fn multiple_contracts_have_independent_ttl_lifetimes() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);

    let id1 = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    let client_addr2 = Address::generate(&env);
    let freelancer_addr2 = Address::generate(&env);
    let id2 = create_default_contract(&env, &client, &client_addr2, &freelancer_addr2);

    // Deposit into contract 1 only
    assert!(client.deposit_funds(&id1, &client_addr, &600_0000000_i128));

    // Advance ledger
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Both contracts should be accessible
    let c1 = client.get_contract(&id1);
    let c2 = client.get_contract(&id2);
    assert_eq!(c1.funded_amount, 600_0000000_i128);
    assert_eq!(c2.funded_amount, 0);
    assert_eq!(c1.client, client_addr);
    assert_eq!(c2.client, client_addr2);
}

/// Verifies that a mixed release+refund lifecycle preserves accounting
/// across ledger advances, confirming TTL extension on both operations.
#[test]
fn mixed_release_and_refund_preserves_accounting_across_ledger_advances() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = create_client(&env);
    let contract_id = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&contract_id, &client_addr, &1_200_0000000_i128));

    // Release milestone 0 (200 XLM)
    env.ledger().set_sequence_number(200_000);
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    // Refund milestone 1 (400 XLM)
    env.ledger().set_sequence_number(500_000);
    let refund_indices = vec![&env, 1_u32];
    let refunded = client.refund_unreleased_milestones(&contract_id, &refund_indices);
    assert_eq!(refunded, 400_0000000_i128);

    // Advance ledger significantly
    env.ledger().set_sequence_number(PERSISTENT_BUMP_THRESHOLD - 1_000);

    // Verify accounting is intact
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.released_amount, 200_0000000_i128);
    assert_eq!(contract.refunded_amount, 400_0000000_i128);
    assert_eq!(contract.funded_amount, 1_200_0000000_i128);
    // Refundable balance = funded - released - refunded = 1200 - 200 - 400 = 600
    assert_eq!(client.get_refundable_balance(&contract_id), 600_0000000_i128);
}
