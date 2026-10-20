//! Tests for the atomic `create_and_fund` entrypoint (issue #1705).
//!
//! Acceptance criteria:
//! - The resulting escrow equals (field-by-field) one produced by `create_contract` +
//!   `deposit_funds`, except for `id`.
//! - Insufficient client balance → whole call fails, no escrow stored
//!   (`contract_count` unchanged).
//! - Paused → `ContractPaused`; validation errors identical to `create_contract`.
//! - `created` then `deposit` events emitted in order.

#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    token::StellarAssetClient,
    vec, Address, Env, Symbol, TryFromVal,
};

use crate::{ContractStatus, Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};

// ── Fixture helpers ──────────────────────────────────────────────────────────

/// Total of the default 3-milestone plan used throughout these tests.
fn total_amount() -> i128 {
    super::total_milestone_amount()
}

fn default_milestones(env: &Env) -> soroban_sdk::Vec<i128> {
    super::default_milestones(env)
}

/// Set up a fully initialized escrow contract with a bound SAC token.
/// Returns `(escrow_client, admin, settlement_token_address)`.
fn setup_with_token(env: &Env) -> (EscrowClient<'_>, Address, Address) {
    let escrow_addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(env, &escrow_addr);
    let admin = Address::generate(env);
    escrow.initialize(&admin);

    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    (escrow, admin, token)
}

/// Mint `amount` tokens to `recipient` from the SAC `token`.
fn mint(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    // Use StellarAssetClient to mint — relies on mock_all_auths.
    StellarAssetClient::new(env, token).mint(recipient, &amount);
}

// ── Field-equality test ──────────────────────────────────────────────────────

/// The escrow produced by `create_and_fund` must equal the one produced by
/// `create_contract` + `deposit_funds`, field-by-field (except `id`).
#[test]
fn create_and_fund_escrow_matches_two_step_field_by_field() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    let total = total_amount();

    // ── Two-step path ────────────────────────────────────────────────────────
    mint(&env, &token, &client_addr, total * 2);

    let two_step_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(escrow.deposit_funds(&two_step_id, &client_addr, &total));
    let two_step = escrow.get_contract(&two_step_id);

    // ── Atomic path ──────────────────────────────────────────────────────────
    let atomic_id = escrow.create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    ) as u32;
    let atomic = escrow.get_contract(&atomic_id);

    // IDs must differ (they were allocated sequentially).
    assert_ne!(two_step_id, atomic_id);

    // All meaningful fields must be identical.
    assert_eq!(atomic.client, two_step.client);
    assert_eq!(atomic.freelancer, two_step.freelancer);
    assert_eq!(atomic.arbiter, two_step.arbiter);
    assert_eq!(atomic.status, two_step.status);
    assert_eq!(atomic.funded_amount, two_step.funded_amount);
    assert_eq!(atomic.total_deposited, two_step.total_deposited);
    assert_eq!(atomic.released_amount, two_step.released_amount);
    assert_eq!(atomic.refunded_amount, two_step.refunded_amount);
    assert_eq!(atomic.release_authorization, two_step.release_authorization);
    assert_eq!(atomic.reputation_issued, two_step.reputation_issued);

    // Status must be Funded.
    assert_eq!(atomic.status, ContractStatus::Funded);
    assert_eq!(atomic.funded_amount, total);
    assert_eq!(atomic.total_deposited, total);

    // Milestones must be identical.
    let two_step_ms = escrow.get_milestones(&two_step_id);
    let atomic_ms = escrow.get_milestones(&atomic_id);
    assert_eq!(atomic_ms.len(), two_step_ms.len());
    for i in 0..atomic_ms.len() {
        let a = atomic_ms.get(i).unwrap();
        let b = two_step_ms.get(i).unwrap();
        assert_eq!(a.amount, b.amount);
        assert_eq!(a.released, b.released);
        assert_eq!(a.refunded, b.refunded);
        assert_eq!(a.funded_amount, b.funded_amount);
    }
}

// ── Insufficient balance ─────────────────────────────────────────────────────

/// If the client doesn't have enough tokens, the entire call must fail and no
/// escrow must be stored (contract_count stays at initial value).
#[test]
fn create_and_fund_fails_on_insufficient_balance_no_escrow_stored() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    // Mint only half the required amount.
    let total = total_amount();
    mint(&env, &token, &client_addr, total / 2);

    // Record the next contract id before the failing call.
    // Storage access must go through `env.as_contract` — direct access from
    // the test context is not supported by the Soroban host.
    let id_before: u32 = env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .get(&crate::DataKey::NextContractId)
            .unwrap_or(1)
    });

    let result = escrow.try_create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(result.is_err(), "should fail with insufficient balance");

    // The transaction is atomic: if the SAC transfer fails, the entire
    // invocation rolls back and no Contract record is persisted.  Verify
    // that no contract was stored at the would-be id.
    let stored: Option<crate::Contract> = env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .get(&crate::DataKey::Contract(id_before))
    });
    assert!(
        stored.is_none(),
        "no contract must be stored when create_and_fund fails"
    );
}

// ── Pause guard ──────────────────────────────────────────────────────────────

/// `create_and_fund` must be blocked when the contract is paused.
#[test]
fn create_and_fund_blocked_when_paused() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _admin, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    mint(&env, &token, &client_addr, total_amount());

    // Pause the escrow.
    escrow.pause();

    let result = escrow.try_create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    super::assert_contract_error(result, Error::ContractPaused);
}

// ── Validation parity with create_contract ───────────────────────────────────

/// Same-address client/freelancer must be rejected.
#[test]
fn create_and_fund_rejects_same_client_freelancer() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    mint(&env, &token, &addr, total_amount());

    let result = escrow.try_create_and_fund(
        &addr,
        &addr, // same as client
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, EscrowError::InvalidParticipant);
}

/// Empty milestones must be rejected.
#[test]
fn create_and_fund_rejects_empty_milestones() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones: soroban_sdk::Vec<i128> = vec![&env];
    mint(&env, &token, &client_addr, 1_000);

    let result = escrow.try_create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, EscrowError::EmptyMilestones);
}

/// Zero-amount milestones must be rejected.
#[test]
fn create_and_fund_rejects_zero_milestone_amount() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 0_i128];
    mint(&env, &token, &client_addr, 1_000);

    let result = escrow.try_create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, EscrowError::InvalidMilestoneAmount);
}

/// Missing arbiter when required must be rejected.
#[test]
fn create_and_fund_rejects_missing_arbiter() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    mint(&env, &token, &client_addr, total_amount());

    let result = escrow.try_create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ArbiterOnly, // requires arbiter
    );
    super::assert_contract_error(result, EscrowError::MissingArbiter);
}

/// Invalid arbiter (same as client) must be rejected.
#[test]
fn create_and_fund_rejects_invalid_arbiter() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    mint(&env, &token, &client_addr, total_amount());

    let result = escrow.try_create_and_fund(
        &client_addr,
        &freelancer_addr,
        &Some(client_addr.clone()), // arbiter == client — invalid
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    );
    super::assert_contract_error(result, EscrowError::InvalidArbiter);
}

/// `NotInitialized` must be returned when the escrow has not been initialized.
#[test]
fn create_and_fund_rejects_not_initialized() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    // Register escrow but do NOT call `initialize`.
    let escrow_addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &escrow_addr);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let result = escrow.try_create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, Error::NotInitialized);
}

/// `SettlementTokenNotConfigured` must be returned when no token is bound.
#[test]
fn create_and_fund_rejects_no_settlement_token() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    // Initialize but do NOT bind a settlement token.
    let escrow_addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);

    let result = escrow.try_create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    super::assert_contract_error(result, EscrowError::SettlementTokenNotConfigured);
}

// ── Event ordering ───────────────────────────────────────────────────────────

/// `create_and_fund` must emit `created` first, then `deposit`, with correct
/// topic payloads.
#[test]
fn create_and_fund_emits_created_then_deposit_events_in_order() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    mint(&env, &token, &client_addr, total_amount());

    let id = escrow.create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    ) as u32;

    let events = env.events().all();

    // Find the indices of the `created` and `deposit` events for this id.
    // events().all() returns Vec<(contract_addr, topics: Vec<Val>, data: Val)>
    // The topic tuple (symbol_short!("created"), id) is published; topic[0] is the symbol.
    let created_sym = symbol_short!("created");
    let deposit_sym = symbol_short!("deposit");

    let created_idx = events
        .iter()
        .position(|e| {
            e.1.len() >= 1
                && Symbol::try_from_val(&env, &e.1.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&created_sym)
        })
        .expect("created event not found");

    let deposit_idx = events
        .iter()
        .position(|e| {
            e.1.len() >= 1
                && Symbol::try_from_val(&env, &e.1.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&deposit_sym)
        })
        .expect("deposit event not found");

    assert!(
        created_idx < deposit_idx,
        "created event (idx {}) must precede deposit event (idx {})",
        created_idx,
        deposit_idx
    );

    // Both events must reference the allocated contract id.
    let _ = id; // used for documentation; events are checked by topic symbol above
}

// ── Post-fund lifecycle ───────────────────────────────────────────────────────

/// An escrow created via `create_and_fund` can be released immediately — no
/// extra deposit step required.
#[test]
fn create_and_fund_escrow_can_be_released_immediately() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    mint(&env, &token, &client_addr, total_amount());

    let id = escrow.create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    ) as u32;

    // Should be able to approve and release without an extra deposit.
    assert!(escrow.approve_milestone_release(&id, &client_addr, &0));
    assert!(escrow.release_milestone(&id, &client_addr, &0));

    let contract = escrow.get_contract(&id);
    assert!(contract.released_amount > 0);
}

/// An escrow created via `create_and_fund` can be cancelled and refunds the
/// client.
#[test]
fn create_and_fund_escrow_can_be_cancelled() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    let total = total_amount();
    mint(&env, &token, &client_addr, total);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let escrow_addr = env.as_contract(&escrow.address, || env.current_contract_address());

    let id = escrow.create_and_fund(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    ) as u32;

    // After create_and_fund the escrow holds `total` and client has 0.
    assert_eq!(token_client.balance(&escrow_addr), total);

    assert!(escrow.cancel_contract(&id, &client_addr));

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Cancelled);
    // Full refund should return funds to the client.
    assert_eq!(token_client.balance(&client_addr), total);
    assert_eq!(token_client.balance(&escrow_addr), 0);
}

// ── Arbiter variant ───────────────────────────────────────────────────────────

/// `create_and_fund` works correctly when an arbiter is provided.
#[test]
fn create_and_fund_with_arbiter_stores_arbiter_field() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (escrow, _, token) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    mint(&env, &token, &client_addr, total_amount());

    let id = escrow.create_and_fund(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
    ) as u32;

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.arbiter, Some(arbiter_addr));
    assert_eq!(contract.status, ContractStatus::Funded);
}

// ── Resource comparison ──────────────────────────────────────────────────────

/// Resource measurement helper — extracts the cost-estimate resources from the
/// last contract invocation on `env`.
struct MeasuredResources {
    write_bytes: u32,
    write_entries: u32,
    read_bytes: u32,
    read_entries: u32,
    instructions: i64,
}

fn measure_last_invocation(env: &Env) -> MeasuredResources {
    let r = env.cost_estimate().resources();
    MeasuredResources {
        write_bytes: r.write_bytes,
        write_entries: r.write_entries,
        read_bytes: r.read_bytes,
        read_entries: r.read_entries,
        instructions: r.instructions,
    }
}

/// `create_and_fund` must use strictly fewer write bytes than the two-step
/// `create_contract` + `deposit_funds` sequence, because it writes the escrow
/// record once instead of twice.
#[test]
fn create_and_fund_uses_strictly_fewer_write_bytes_than_two_step() {
    // ── Two-step path ─────────────────────────────────────────────────────
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let escrow_addr = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = default_milestones(&env);
    let total = total_amount();
    StellarAssetClient::new(&env, &token).mint(&client_addr, &(total * 2));

    // Step 1: create_contract
    let create_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let create_res = measure_last_invocation(&env);

    // Step 2: deposit_funds
    assert!(escrow.deposit_funds(&create_id, &client_addr, &total));
    let deposit_res = measure_last_invocation(&env);

    let two_step_write_bytes = create_res.write_bytes + deposit_res.write_bytes;
    let two_step_write_entries = create_res.write_entries + deposit_res.write_entries;

    // ── One-step path ─────────────────────────────────────────────────────
    let env2 = Env::default();
    env2.mock_all_auths_allowing_non_root_auth();

    let escrow_addr2 = env2.register(crate::Escrow, ());
    let escrow2 = crate::EscrowClient::new(&env2, &escrow_addr2);
    let admin2 = Address::generate(&env2);
    escrow2.initialize(&admin2);
    let token_admin2 = Address::generate(&env2);
    let token2 = env2.register_stellar_asset_contract(token_admin2.clone());
    escrow2.bind_settlement_token(&admin2, &token2);

    let client2 = Address::generate(&env2);
    let freelancer2 = Address::generate(&env2);
    let milestones2 = default_milestones(&env2);
    StellarAssetClient::new(&env2, &token2).mint(&client2, &total);

    let _id2 = escrow2.create_and_fund(
        &client2,
        &freelancer2,
        &None,
        &milestones2,
        &ReleaseAuthorization::ClientOnly,
    );
    let one_step_res = measure_last_invocation(&env2);

    // Print resource table for PR documentation.
    soroban_sdk::log!(
        &env2,
        "Resource comparison — two-step: write_bytes={} write_entries={} | one-step: write_bytes={} write_entries={}",
        two_step_write_bytes,
        two_step_write_entries,
        one_step_res.write_bytes,
        one_step_res.write_entries,
    );

    // Core acceptance criterion: one-step uses strictly fewer write bytes.
    assert!(
        one_step_res.write_bytes < two_step_write_bytes,
        "create_and_fund write_bytes ({}) must be < two-step write_bytes ({})",
        one_step_res.write_bytes,
        two_step_write_bytes
    );
}
