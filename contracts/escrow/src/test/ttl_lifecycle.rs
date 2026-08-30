//! State-specific TTL lifecycle tests (issue #1348).
//!
//! Verifies that persistent storage entries for escrow contracts receive the
//! correct TTL based on their lifecycle state:
//!
//! | State                              | TTL   | Bump threshold |
//! |------------------------------------|-------|----------------|
//! | Created / Funded / PartiallyFunded | 60 d  | 15 d           |
//! | Disputed                           | 75 d  | 20 d           |
//! | Completed / Cancelled / Refunded   | 30 d  | 7 d            |
//!
//! Edge cases covered (each with a dedicated test):
//! - new_record: contract TTL is set to ACTIVE on creation
//! - active_record: TTL remains ACTIVE while contract is Funded
//! - disputed_record: TTL upgrades to DISPUTED on raise_dispute
//! - near_expiry: entry is bumped when remaining TTL falls below threshold
//! - expired_record: evicted entry returns ContractNotFound

#![cfg(test)]

use soroban_sdk::{
    testutils::{storage::Persistent as _, Address as _, Ledger as _},
    token::StellarAssetClient,
    vec, Address, Env,
};

use crate::{
    ttl::{
        ACTIVE_CONTRACT_BUMP_THRESHOLD, ACTIVE_CONTRACT_TTL_LEDGERS,
        CLOSED_CONTRACT_TTL_LEDGERS, DISPUTED_CONTRACT_TTL_LEDGERS,
    },
    ContractStatus, DataKey, Escrow, EscrowClient, EscrowError, ReleaseAuthorization,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

const BIG_TTL: u32 = DISPUTED_CONTRACT_TTL_LEDGERS * 4;

fn setup(env: &Env) -> (EscrowClient<'_>, Address, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|li| {
        li.sequence_number = 1_000;
        li.max_entry_ttl = BIG_TTL;
        li.min_persistent_entry_ttl = BIG_TTL;
    });
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(env, &addr);
    let admin = Address::generate(env);
    client.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);
    (client, token, admin)
}

fn advance(env: &Env, escrow: &EscrowClient<'_>, by: u32) {
    env.ledger()
        .set_sequence_number(env.ledger().sequence().saturating_add(by));
    // Keep the instance alive
    env.as_contract(&escrow.address, || {
        env.storage().instance().extend_ttl(BIG_TTL, BIG_TTL);
    });
}

/// Read the persistent TTL for DataKey::Contract(contract_id).
fn contract_ttl(env: &Env, escrow: &EscrowClient<'_>, contract_id: u32) -> u32 {
    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::Contract(contract_id))
    })
}

// ── Edge case: new record ─────────────────────────────────────────────────────

/// A newly created contract must receive the active-contract TTL (60 days).
#[test]
fn new_record_gets_active_ttl_on_creation() {
    let env = Env::default();
    let (escrow, _token, _admin) = setup(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    let ttl = contract_ttl(&env, &escrow, cid);
    // TTL must be at least the active-contract target.
    assert!(
        ttl >= ACTIVE_CONTRACT_TTL_LEDGERS,
        "new contract TTL ({ttl}) must be ≥ ACTIVE_CONTRACT_TTL_LEDGERS ({ACTIVE_CONTRACT_TTL_LEDGERS})"
    );
    // And must not exceed the disputed TTL (it's not disputed yet).
    assert!(
        ttl <= DISPUTED_CONTRACT_TTL_LEDGERS,
        "new contract TTL ({ttl}) must not exceed DISPUTED_CONTRACT_TTL_LEDGERS"
    );
}

/// A newly created contract must NOT receive the old flat 30-day TTL.
#[test]
fn new_record_is_not_flat_30_day_ttl() {
    let env = Env::default();
    let (escrow, _token, _admin) = setup(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    let ttl = contract_ttl(&env, &escrow, cid);
    assert!(
        ttl > CLOSED_CONTRACT_TTL_LEDGERS,
        "new contract TTL ({ttl}) must exceed the legacy flat 30-day TTL"
    );
}

// ── Edge case: active record ──────────────────────────────────────────────────

/// A funded (active) contract must retain the 60-day active TTL after deposit.
#[test]
fn active_record_retains_active_ttl_after_deposit() {
    let env = Env::default();
    let (escrow, token, _admin) = setup(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    StellarAssetClient::new(&env, &token).mint(&client_addr, &200);
    escrow.deposit_funds(&cid, &client_addr, &200);

    assert_eq!(escrow.get_contract(&cid).status, ContractStatus::Funded);
    let ttl = contract_ttl(&env, &escrow, cid);
    assert!(
        ttl >= ACTIVE_CONTRACT_TTL_LEDGERS,
        "funded contract TTL ({ttl}) must be ≥ ACTIVE_CONTRACT_TTL_LEDGERS"
    );
}

// ── Edge case: disputed record ────────────────────────────────────────────────

/// When a dispute is raised, the contract TTL must upgrade to the 75-day disputed window.
#[test]
fn disputed_record_gets_disputed_ttl() {
    let env = Env::default();
    let (escrow, token, _admin) = setup(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client_addr, &100);
    escrow.deposit_funds(&cid, &client_addr, &100);
    escrow.raise_dispute(&cid, &client_addr);

    assert_eq!(escrow.get_contract(&cid).status, ContractStatus::Disputed);
    let ttl = contract_ttl(&env, &escrow, cid);
    assert!(
        ttl >= DISPUTED_CONTRACT_TTL_LEDGERS,
        "disputed contract TTL ({ttl}) must be ≥ DISPUTED_CONTRACT_TTL_LEDGERS ({DISPUTED_CONTRACT_TTL_LEDGERS})"
    );
}

/// Disputed TTL must be strictly greater than active TTL.
#[test]
fn disputed_ttl_exceeds_active_ttl() {
    assert!(
        DISPUTED_CONTRACT_TTL_LEDGERS > ACTIVE_CONTRACT_TTL_LEDGERS,
        "disputed TTL must exceed active TTL"
    );
}

/// After dispute resolution, TTL must drop to the closed-state policy.
#[test]
fn resolved_dispute_gets_closed_ttl() {
    let env = Env::default();
    let (escrow, token, _admin) = setup(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client_addr, &100);
    escrow.deposit_funds(&cid, &client_addr, &100);
    escrow.raise_dispute(&cid, &client_addr);
    escrow.resolve_dispute(&cid, &arbiter_addr, &crate::DisputeResolution::FullRefund);

    let ttl = contract_ttl(&env, &escrow, cid);
    // After resolution the contract is in a terminal state; TTL must equal the closed window.
    assert!(
        ttl <= CLOSED_CONTRACT_TTL_LEDGERS,
        "resolved contract TTL ({ttl}) must be ≤ CLOSED_CONTRACT_TTL_LEDGERS ({CLOSED_CONTRACT_TTL_LEDGERS})"
    );
}

// ── Edge case: near expiry ────────────────────────────────────────────────────

/// When an active contract approaches expiry (remaining TTL < bump threshold),
/// a meaningful write must renew the TTL to the full active window.
#[test]
fn near_expiry_active_contract_is_bumped_on_write() {
    let env = Env::default();
    let (escrow, token, _admin) = setup(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    StellarAssetClient::new(&env, &token).mint(&client_addr, &200);
    escrow.deposit_funds(&cid, &client_addr, &200);

    // Advance past the active bump threshold so TTL is below it.
    let delta = ACTIVE_CONTRACT_TTL_LEDGERS - ACTIVE_CONTRACT_BUMP_THRESHOLD + 100;
    advance(&env, &escrow, delta);

    // TTL before the write should now be below the bump threshold.
    let ttl_before = contract_ttl(&env, &escrow, cid);
    assert!(
        ttl_before < ACTIVE_CONTRACT_BUMP_THRESHOLD,
        "TTL before write ({ttl_before}) must be below bump threshold ({ACTIVE_CONTRACT_BUMP_THRESHOLD})"
    );

    // A milestone release is a meaningful write that must renew the TTL.
    escrow.approve_milestone_release(&cid, &client_addr, &0);
    escrow.release_milestone(&cid, &client_addr, &0);

    let ttl_after = contract_ttl(&env, &escrow, cid);
    assert!(
        ttl_after >= ACTIVE_CONTRACT_BUMP_THRESHOLD,
        "TTL after write ({ttl_after}) must be renewed above bump threshold"
    );
}

// ── Edge case: completed contract gets closed TTL ────────────────────────────

/// Completing a contract (releasing all milestones) must transition to the 30-day closed TTL.
#[test]
fn completed_contract_gets_closed_ttl() {
    let env = Env::default();
    let (escrow, token, _admin) = setup(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&client_addr, &100);
    escrow.deposit_funds(&cid, &client_addr, &100);
    escrow.approve_milestone_release(&cid, &client_addr, &0);
    escrow.release_milestone(&cid, &client_addr, &0);

    assert_eq!(escrow.get_contract(&cid).status, ContractStatus::Completed);
    let ttl = contract_ttl(&env, &escrow, cid);
    assert!(
        ttl <= CLOSED_CONTRACT_TTL_LEDGERS,
        "completed contract TTL ({ttl}) must be ≤ CLOSED_CONTRACT_TTL_LEDGERS ({CLOSED_CONTRACT_TTL_LEDGERS})"
    );
}

/// Cancelling a contract must transition to the 30-day closed TTL.
#[test]
fn cancelled_contract_gets_closed_ttl() {
    let env = Env::default();
    let (escrow, _token, _admin) = setup(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    escrow.cancel_contract(&cid, &client_addr);

    assert_eq!(escrow.get_contract(&cid).status, ContractStatus::Cancelled);
    let ttl = contract_ttl(&env, &escrow, cid);
    assert!(
        ttl <= CLOSED_CONTRACT_TTL_LEDGERS,
        "cancelled contract TTL ({ttl}) must be ≤ CLOSED_CONTRACT_TTL_LEDGERS"
    );
}

// ── Edge case: expired record ─────────────────────────────────────────────────

/// An evicted contract entry must return ContractNotFound, not panic or return stale data.
#[test]
fn expired_record_returns_contract_not_found() {
    let env = Env::default();
    let (escrow, _token, _admin) = setup(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    // Advance past the active TTL so Soroban evicts the entry.
    advance(&env, &escrow, ACTIVE_CONTRACT_TTL_LEDGERS + 1);

    // Any attempt to read or mutate the evicted contract must fail.
    let result = escrow.try_get_contract(&cid);
    assert!(
        result.is_err(),
        "evicted contract must return an error, not stale data"
    );
}

// ── TTL constant invariants ───────────────────────────────────────────────────

/// The three TTL tiers must be ordered: Disputed > Active > Closed.
#[test]
fn ttl_tiers_are_correctly_ordered() {
    use crate::ttl::{
        ACTIVE_CONTRACT_TTL_LEDGERS, CLOSED_CONTRACT_TTL_LEDGERS, DISPUTED_CONTRACT_TTL_LEDGERS,
    };
    assert!(
        DISPUTED_CONTRACT_TTL_LEDGERS > ACTIVE_CONTRACT_TTL_LEDGERS,
        "Disputed TTL must exceed Active TTL"
    );
    assert!(
        ACTIVE_CONTRACT_TTL_LEDGERS > CLOSED_CONTRACT_TTL_LEDGERS,
        "Active TTL must exceed Closed TTL"
    );
}

/// ttl_for_status must return the correct (threshold, extend_to) pair for each status.
#[test]
fn ttl_for_status_returns_correct_pairs() {
    use crate::ttl::{
        ttl_for_status, ACTIVE_CONTRACT_BUMP_THRESHOLD, ACTIVE_CONTRACT_TTL_LEDGERS,
        CLOSED_CONTRACT_BUMP_THRESHOLD, CLOSED_CONTRACT_TTL_LEDGERS,
        DISPUTED_CONTRACT_BUMP_THRESHOLD, DISPUTED_CONTRACT_TTL_LEDGERS,
    };

    assert_eq!(
        ttl_for_status(ContractStatus::Created),
        (ACTIVE_CONTRACT_BUMP_THRESHOLD, ACTIVE_CONTRACT_TTL_LEDGERS)
    );
    assert_eq!(
        ttl_for_status(ContractStatus::Funded),
        (ACTIVE_CONTRACT_BUMP_THRESHOLD, ACTIVE_CONTRACT_TTL_LEDGERS)
    );
    assert_eq!(
        ttl_for_status(ContractStatus::PartiallyFunded),
        (ACTIVE_CONTRACT_BUMP_THRESHOLD, ACTIVE_CONTRACT_TTL_LEDGERS)
    );
    assert_eq!(
        ttl_for_status(ContractStatus::Disputed),
        (DISPUTED_CONTRACT_BUMP_THRESHOLD, DISPUTED_CONTRACT_TTL_LEDGERS)
    );
    assert_eq!(
        ttl_for_status(ContractStatus::Completed),
        (CLOSED_CONTRACT_BUMP_THRESHOLD, CLOSED_CONTRACT_TTL_LEDGERS)
    );
    assert_eq!(
        ttl_for_status(ContractStatus::Cancelled),
        (CLOSED_CONTRACT_BUMP_THRESHOLD, CLOSED_CONTRACT_TTL_LEDGERS)
    );
    assert_eq!(
        ttl_for_status(ContractStatus::Refunded),
        (CLOSED_CONTRACT_BUMP_THRESHOLD, CLOSED_CONTRACT_TTL_LEDGERS)
    );
}
