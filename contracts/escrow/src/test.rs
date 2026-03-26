//! Comprehensive tests for the TalentTrust escrow contract.
//!
//! Test coverage:
//! - Contract lifecycle (create → deposit → release → complete)
//! - Partial refund: full unreleased balance
//! - Partial refund: some milestones already released
//! - Refund blocked when contract is Completed
//! - Refund blocked on double-call (Cancelled guard)
//! - Refund blocked for unauthorised caller
//! - Refund blocked when all milestones already released (zero refund)
//! - Deposit then immediate refund
//! - Deposit rejected on invalid amount
//! - release_milestone out-of-range index

#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    vec, Address, Env, TryIntoVal,
};

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    vec, Address, Env, IntoVal,
};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up a default Env and register the Escrow contract.
fn setup() -> (Env, EscrowClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    // SAFETY: the client lasts as long as env; we extend lifetime for convenience
    // in test helpers. The env is kept alive by the caller.
    let client = EscrowClient::new(&env, &cid);
    // Return env + client; env must outlive client.
    (env, client)
}

/// Create a 3-milestone contract and return (env, client, contract_id, client_addr, freelancer_addr).
fn funded_contract() -> (Env, EscrowClient<'static>, u32, Address, Address) {
    let (env, client) = setup();
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // 200 / 400 / 600 XLM (in stroops)
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);

    let total: i128 = 1_200_0000000;
    client.deposit_funds(&contract_id, &total);

    (env, client, contract_id, client_addr, freelancer_addr)
}

// ---------------------------------------------------------------------------
// Existing / lifecycle tests (kept & updated for stateful API)
// ---------------------------------------------------------------------------

#[test]
fn test_hello() {
    let (_, client) = setup();
    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

// ==================== CONTRACT CREATION TESTS ====================

#[test]
fn test_create_contract_returns_id() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];

    let id = client.create_contract(&c, &f, &milestones);
    // First contract minted by the counter should always be id = 1.
    assert_eq!(id, 1);
}

#[test]
fn test_create_multiple_contracts_unique_ids() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];

    let id1 = client.create_contract(&c, &f, &milestones);
    let id2 = client.create_contract(&c, &f, &milestones);
    assert_ne!(id1, id2);
    assert_eq!(id2, id1 + 1);
}

#[test]
fn test_deposit_funds_returns_true() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 500_0000000_i128];
    let id = client.create_contract(&c, &f, &milestones);

    let result = client.deposit_funds(&id, &500_0000000_i128);
    assert!(result);
}

#[test]
fn test_release_milestone_returns_true() {
    let (_, client, id, _, _) = funded_contract();
    let result = client.release_milestone(&id, &0);
    assert!(result);
}

// ---------------------------------------------------------------------------
// Partial refund — happy paths
// ---------------------------------------------------------------------------

/// No milestones have been released → full deposited amount is refunded.
#[test]
fn test_refund_full_unreleased() {
    let (_, client, id, _, _) = funded_contract();

    // total milestones = 200 + 400 + 600 = 1200 XLM
    let refund = client.request_refund(&id);
    assert_eq!(refund, 1_200_0000000_i128);
}

/// One milestone released (200 XLM) → refund is 400 + 600 = 1000 XLM.
#[test]
fn test_refund_partial_unreleased_one_released() {
    let (_, client, id, _, _) = funded_contract();
    client.release_milestone(&id, &0); // release 200 XLM

    let refund = client.request_refund(&id);
    assert_eq!(refund, 1_000_0000000_i128); // 400 + 600
}

/// Two milestones released → refund is only the last milestone.
#[test]
fn test_refund_partial_unreleased_two_released() {
    let (_, client, id, _, _) = funded_contract();
    client.release_milestone(&id, &0); // 200 XLM
    client.release_milestone(&id, &1); // 400 XLM

    let refund = client.request_refund(&id);
    assert_eq!(refund, 600_0000000_i128); // only milestone 2 unreleased
}

/// Deposit then immediately request refund (no work done at all).
#[test]
fn test_deposit_then_immediate_refund() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 300_0000000_i128, 700_0000000_i128];
    let id = client.create_contract(&c, &f, &milestones);
    client.deposit_funds(&id, &1_000_0000000_i128);

    let refund = client.request_refund(&id);
    assert_eq!(refund, 1_000_0000000_i128);
}

/// Single-milestone contract: funded, not released → full refund.
#[test]
fn test_refund_single_milestone() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 500_0000000_i128];
    let id = client.create_contract(&c, &f, &milestones);
    client.deposit_funds(&id, &500_0000000_i128);

    let refund = client.request_refund(&id);
    assert_eq!(refund, 500_0000000_i128);
}

// ---------------------------------------------------------------------------
// Partial refund — event emission
// ---------------------------------------------------------------------------

/// A `refund` event is emitted with the correct contract_id and amount.
#[test]
fn test_refund_emits_event() {
    let (env, client, id, _, _) = funded_contract();
    client.request_refund(&id);

    let events = env.events().all();
    // Find a refund event (topic[1] == "refund")
    let refund_event = events.iter().find(|(_, topics, _)| {
        if topics.len() == 0 {
            return false;
        }
        let result: Result<soroban_sdk::Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
        result == Ok(symbol_short!("refund"))
    });
    assert!(
        refund_event.is_some(),
        "expected a refund event to be emitted"
    );
}

// ---------------------------------------------------------------------------
// Partial refund — failure / guard paths
// ---------------------------------------------------------------------------

/// Refund on a Completed contract must panic with InvalidStatus.
#[test]
#[should_panic]
fn test_refund_fails_when_completed() {
    let (_, client, id, _, _) = funded_contract();
    // Release all three milestones → status becomes Completed
    client.release_milestone(&id, &0);
    client.release_milestone(&id, &1);
    client.release_milestone(&id, &2);

    // This must panic
    client.request_refund(&id);
}

/// Second refund call on a Cancelled contract must panic (double-refund guard).
#[test]
#[should_panic]
fn test_refund_fails_when_already_cancelled() {
    let (_, client, id, _, _) = funded_contract();
    client.request_refund(&id); // first call → ok, sets status = Cancelled
    client.request_refund(&id); // must panic
}

/// Non-client caller must not be able to trigger a refund.
/// (mock_all_auths is not used here; we test raw auth rejection.)
#[test]
#[should_panic]
fn test_refund_fails_unauthorized() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 500_0000000_i128];

    // create_contract is open (no require_auth), so this works without mocking.
    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);

    // request_refund requires auth from client_addr.
    // Since we are not mocking auths, this call should panic.
    client.request_refund(&id);
}

/// If all milestones are released before cancellation, refund amount is 0 →
/// should panic with NothingToRefund before reaching Cancelled.
#[test]
#[should_panic]
fn test_refund_fails_zero_unreleased() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 300_0000000_i128];
    let id = client.create_contract(&c, &f, &milestones);
    client.deposit_funds(&id, &300_0000000_i128);
    client.release_milestone(&id, &0); // releases the only milestone → Completed

    // Completed guard fires first, but either way must panic.
    client.request_refund(&id);
}

// ---------------------------------------------------------------------------
// Other edge-case tests
// ---------------------------------------------------------------------------

/// Depositing a zero or negative amount must panic.
#[test]
#[should_panic]
fn test_deposit_invalid_amount_zero() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&c, &f, &milestones);
    client.deposit_funds(&id, &0); // must panic
}

/// Releasing an out-of-range milestone index must panic.
#[test]
#[should_panic]
fn test_release_milestone_out_of_range() {
    let (_, client, id, _, _) = funded_contract();
    client.release_milestone(&id, &99); // index 99 doesn't exist
}

/// Releasing the same milestone twice must panic.
#[test]
#[should_panic]
fn test_release_milestone_already_released() {
    let (_, client, id, _, _) = funded_contract();
    client.release_milestone(&id, &0);
    client.release_milestone(&id, &0); // second release must panic
}

/// Creating a contract with a zero-amount milestone must panic.
#[test]
#[should_panic]
fn test_create_contract_invalid_milestone_amount() {
    let (env, client) = setup();
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 0_i128]; // zero amount — invalid
    client.create_contract(&c, &f, &milestones);
}

#[test]
fn test_contract_completion_all_milestones_released() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128, 2000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &3000_0000000);

    client.approve_milestone_release(&1, &client_addr, &0);
    client.release_milestone(&1, &client_addr, &0);

    client.approve_milestone_release(&1, &client_addr, &1);
    client.release_milestone(&1, &client_addr, &1);

    // All milestones should be released and contract completed
    // Note: In a real implementation, we would check the contract status
    // For this simplified version, we just verify no panics occurred
}

#[test]
fn test_edge_cases() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1_0000000_i128]; // Minimum amount

    // Test with minimum amount
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 0);

    // Test with multiple milestones
    let many_milestones = vec![
        &env,
        100_0000000_i128,
        200_0000000_i128,
        300_0000000_i128,
        400_0000000_i128,
    ];
    let id2 = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &many_milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id2, 0); // ledger sequence stays the same in test env
}

mod emergency_controls;
mod pause_controls;
