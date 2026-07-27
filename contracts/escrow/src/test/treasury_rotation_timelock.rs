#![cfg(test)]

//! Regression tests for the treasury/admin rotation timelock.
//!
//! Rule: `accept_governance_admin` MUST NOT succeed until at least
//! `ADMIN_ROTATION_MIN_DELAY_LEDGERS` have elapsed since the matching
//! `propose_governance_admin` call.

use crate::{EscrowError, ADMIN_ROTATION_MIN_DELAY_LEDGERS};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, LedgerInfo},
    Address, Env,
};

use super::register_client;

// ---------------------------------------------------------------------------
// Helper: advance the test ledger by `delta` ledgers.
// ---------------------------------------------------------------------------

fn advance_ledgers(env: &Env, delta: u32) {
    let info = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: info.sequence_number + delta,
        timestamp: info.timestamp + (delta as u64) * 5,
        protocol_version: info.protocol_version,
        network_id: info.network_id,
        base_reserve: info.base_reserve,
        min_temp_entry_ttl: info.min_temp_entry_ttl,
        min_persistent_entry_ttl: info.min_persistent_entry_ttl,
        max_entry_ttl: info.max_entry_ttl,
    });
}

// ---------------------------------------------------------------------------
// Happy path: accept succeeds exactly at min_delay boundary.
// ---------------------------------------------------------------------------

/// `accept_governance_admin` succeeds when the ledger has advanced by exactly
/// `ADMIN_ROTATION_MIN_DELAY_LEDGERS` since the proposal.
#[test]
fn accept_succeeds_after_min_delay_ledgers_elapse() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let next_admin = Address::generate(&env);
    client.propose_governance_admin(&next_admin);

    // Advance to exactly the minimum required delay.
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(next_admin));
    assert_eq!(client.get_pending_governance_admin(), None);
}

// ---------------------------------------------------------------------------
// Sad path: accept is rejected before min_delay ledgers elapse.
// ---------------------------------------------------------------------------

/// `accept_governance_admin` MUST fail with `TimelockNotElapsed` when called
/// immediately after the proposal (zero ledgers elapsed).
#[test]
fn accept_rejected_immediately_after_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let next_admin = Address::generate(&env);
    client.propose_governance_admin(&next_admin);

    // No ledger advancement — timelock has not elapsed.
    super::assert_contract_error(
        client.try_accept_governance_admin(),
        EscrowError::TimelockNotElapsed,
    );
}

/// `accept_governance_admin` MUST fail with `TimelockNotElapsed` when called
/// one ledger before the minimum delay is reached.
#[test]
fn accept_rejected_one_ledger_before_min_delay() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let next_admin = Address::generate(&env);
    client.propose_governance_admin(&next_admin);

    // Advance to one ledger short of the required minimum.
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS - 1);

    super::assert_contract_error(
        client.try_accept_governance_admin(),
        EscrowError::TimelockNotElapsed,
    );
}
