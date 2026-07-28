//! Pause-gate regression tests for the mutating escrow entrypoints.
//!
//! Issue #692 / #1049: All mutating milestone entrypoints — `create_contract`,
//! `deposit_funds`, `approve_milestone_release`, `release_milestone`,
//! `refund_unreleased_milestones`, `cancel_contract`, `submit_work_evidence`,
//! and `issue_reputation` — must honor the `Paused` flag and reject calls with
//! `ContractPaused` while paused, then resume normally after unpause.
//!
//! This module closes issue #1049 by adding explicit pause-rejection tests for
//! `approve_milestone_release`, which is fully gated by `require_not_paused` in
//! `lib.rs`, and verifying the guard fires before any approval state is mutated.
//!
//! Emergency-mode coverage lives in emergency_controls.rs; this module exercises
//! the plain `pause()` / `unpause()` path only. The pause check runs before
//! `require_auth`, so a paused contract rejects uniformly regardless of caller.
//!
//! ## Helper strategy
//!
//! * `setup_initialized` — registers a fresh contract and calls `initialize`.
//! * `setup_created_contract` — creates an escrow in `Created` status (no SAC
//!   binding, no deposit). Sufficient for any "pause blocks" test because the
//!   pause gate fires before any SAC call or funding check.
//! * `setup_funded_contract` — binds a Stellar Asset Contract, mints tokens,
//!   and deposits so the contract reaches `Funded` status. Required for
//!   `release_milestone`, which needs an on-chain token balance to pay out.
//!
//! ## Error codes
//!
//! The pause guard calls `env.panic_with_error(Error::ContractPaused)` where
//! `Error` is the canonical enum in `types.rs` (`ContractPaused = 37`).  Tests
//! therefore assert against `Error::ContractPaused`, NOT `EscrowError::ContractPaused`
//! (a separate `#[contracterror]` enum in `lib.rs` with code 16).

use crate::{Error, Escrow, EscrowClient, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env, String};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Register and initialize a fresh escrow.
///
/// Returns `(env, contract_address, admin)`.  All auths are mocked so that
/// `initialize`, `pause`, `unpause`, and other admin operations succeed without
/// setting up explicit auth entries.
fn setup_initialized() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    (env, contract_id, admin)
}

/// Create a contract in `Created` status with no SAC binding and no deposit.
///
/// This is sufficient for any "pause blocks X" test because `require_not_paused`
/// fires before SAC checks or funding validation, guaranteeing `ContractPaused`
/// (code 37) is returned regardless of contract state.
fn setup_created_contract(env: &Env, client: &EscrowClient) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 100_i128, 200_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    (client_addr, freelancer_addr, id)
}

/// Create and fully fund a contract via a bound SAC, producing a `Funded` contract.
///
/// Required for tests that need to verify a successful operation after unpause
/// (e.g., `release_milestone`) because the release path calls
/// `token::Client::transfer` under the hood.
///
/// Uses `mock_all_auths_allowing_non_root_auth` to permit the SAC `transfer`
/// call that originates from inside the escrow contract.
fn setup_funded_contract_env() -> (Env, Address, Address, Address, Address, u32) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let escrow_addr = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128, 200_i128];

    // Bind a Stellar Asset Contract so deposit_funds and release_milestone work.
    let token_addr = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token_addr);

    // Mint enough tokens to the client so the full deposit succeeds.
    StellarAssetClient::new(&env, &token_addr).mint(&client_addr, &300_i128);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &client_addr, &300_i128);

    (env, escrow_addr, admin, client_addr, freelancer_addr, id)
}

// ---------------------------------------------------------------------------
// Pause / unpause state
// ---------------------------------------------------------------------------

#[test]
fn pause_then_unpause_toggles_state() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(!client.is_paused());
    client.pause();
    assert!(client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// create_contract
// ---------------------------------------------------------------------------

#[test]
fn pause_blocks_create_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause();

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    super::assert_contract_error(
        client.try_create_contract(
            &a,
            &b,
            &None,
            &vec![&env, 50_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        Error::ContractPaused,
    );
}

#[test]
fn unpause_restores_create_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause();
    client.unpause();

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let id = client.create_contract(
        &a,
        &b,
        &None,
        &vec![&env, 50_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);
}

#[test]
fn pause_gate_runs_before_auth_on_create_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause();

    // Even an outsider address receives ContractPaused, not an auth error.
    let outsider = Address::generate(&env);
    let other = Address::generate(&env);
    super::assert_contract_error(
        client.try_create_contract(
            &outsider,
            &other,
            &None,
            &vec![&env, 50_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        Error::ContractPaused,
    );
}

// ---------------------------------------------------------------------------
// deposit_funds
// ---------------------------------------------------------------------------

/// Pausing must cause `deposit_funds` to fail with `ContractPaused` (code 37)
/// before any SAC transfer is attempted.
#[test]
fn pause_blocks_deposit_funds() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    // A Created-status contract is enough; the pause guard fires before SAC checks.
    let (client_addr, _freelancer, id) = setup_created_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_deposit_funds(&id, &client_addr, &50_i128),
        Error::ContractPaused,
    );
}

/// After unpausing, `deposit_funds` succeeds on a SAC-backed contract.
#[test]
fn unpause_restores_deposit_funds() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let escrow_addr = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_addr);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Pause and immediately unpause.
    client.pause();
    client.unpause();

    // Bind a SAC and mint so deposit can succeed.
    let depositor = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token_addr);
    StellarAssetClient::new(&env, &token_addr).mint(&depositor, &50_i128);

    let other = Address::generate(&env);
    let id = client.create_contract(
        &depositor,
        &other,
        &None,
        &vec![&env, 50_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&id, &depositor, &50_i128));
}

// ---------------------------------------------------------------------------
// approve_milestone_release   (issue #1049)
// ---------------------------------------------------------------------------

/// While paused, `approve_milestone_release` must be rejected immediately with
/// `ContractPaused` (code 37) before any approval state is written to temporary
/// storage.
#[test]
fn pause_blocks_approve_milestone_release() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    // A Created-status contract is sufficient: the pause guard is the first
    // statement in `approve_milestone_release` and fires before any storage read.
    let (client_addr, _freelancer, id) = setup_created_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_approve_milestone_release(&id, &client_addr, &0),
        Error::ContractPaused,
    );
}

/// After unpausing, `approve_milestone_release` is no longer blocked by the
/// pause gate.  The call may fail for other reasons (e.g. `InvalidState` because
/// the contract is still `Created`), but the error must NOT be `ContractPaused`.
#[test]
fn unpause_restores_approve_milestone_release() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer, id) = setup_created_contract(&env, &client);

    // Pause then immediately unpause.
    client.pause();
    client.unpause();

    // The contract is in `Created` status (not funded), so the call will fail
    // with `InvalidState` — but critically, NOT with `ContractPaused`.
    let result = client.try_approve_milestone_release(&id, &client_addr, &0);
    let paused_err: soroban_sdk::Error = Error::ContractPaused.into();
    match result {
        Err(Ok(e)) => {
            assert_ne!(
                e, paused_err,
                "approve_milestone_release must NOT return ContractPaused after unpause"
            );
        }
        Ok(_) => {
            // Approval succeeded — even better; pause is definitely not blocking.
        }
        Err(Err(_)) => {
            // Host-level error; unexpected in a mock env but not a pause issue.
        }
    }
}

/// The pause gate in `approve_milestone_release` runs before `require_auth`, so
/// even an unprivileged outsider address receives `ContractPaused`, not an auth error.
#[test]
fn pause_gate_runs_before_auth_on_approve_milestone_release() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (_client_addr, _freelancer, id) = setup_created_contract(&env, &client);
    client.pause();

    // Use an outsider address unrelated to the contract.
    let outsider = Address::generate(&env);
    super::assert_contract_error(
        client.try_approve_milestone_release(&id, &outsider, &0),
        Error::ContractPaused,
    );
}

/// No approval record must be written to temporary storage while paused.
/// After unpausing, `get_milestone_approvals` must return `None` for the
/// milestone that the blocked call targeted.
#[test]
fn pause_prevents_approval_state_mutation() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer, id) = setup_created_contract(&env, &client);
    client.pause();

    // Attempt approval while paused — it must be rejected.
    let _ = client.try_approve_milestone_release(&id, &client_addr, &0);

    // After unpausing, no stale approval record should exist.
    client.unpause();
    let approvals = client.get_milestone_approvals(&id, &0);
    assert!(
        approvals.is_none(),
        "no approval record must exist after a blocked (paused) approve attempt"
    );
}

// ---------------------------------------------------------------------------
// release_milestone
// ---------------------------------------------------------------------------

/// Pausing blocks `release_milestone` before any token transfer occurs.
#[test]
fn pause_blocks_release_milestone() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    // A Created-status contract is enough; pause check fires first.
    let (client_addr, _freelancer, id) = setup_created_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        Error::ContractPaused,
    );
}

/// After unpausing, a fully funded contract's milestone can be released normally.
#[test]
fn unpause_restores_release_milestone() {
    let (env, escrow_addr, _admin, client_addr, _freelancer, id) = setup_funded_contract_env();
    let client = EscrowClient::new(&env, &escrow_addr);

    client.pause();
    client.unpause();

    client.approve_milestone_release(&id, &client_addr, &0);
    assert!(client.release_milestone(&id, &client_addr, &0));
}

// ---------------------------------------------------------------------------
// refund_unreleased_milestones
// ---------------------------------------------------------------------------

/// Pausing blocks `refund_unreleased_milestones` before any balance check.
#[test]
fn pause_blocks_refund_unreleased_milestones() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (_client_addr, _freelancer, id) = setup_created_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_refund_unreleased_milestones(&id, &vec![&env, 0_u32]),
        Error::ContractPaused,
    );
}

/// After unpausing, refund succeeds on a funded contract where milestones have
/// no deadline (allowing immediate refund without an overdue check).
#[test]
fn unpause_restores_refund_unreleased_milestones() {
    let (env, escrow_addr, _admin, _client_addr, _freelancer, id) = setup_funded_contract_env();
    let client = EscrowClient::new(&env, &escrow_addr);

    client.pause();
    client.unpause();

    // Both milestones have no deadline (None) so they are refundable immediately.
    let refunded = client.refund_unreleased_milestones(&id, &vec![&env, 0_u32, 1_u32]);
    assert!(refunded > 0, "refund amount must be positive after unpause");
}

// ---------------------------------------------------------------------------
// cancel_contract
// ---------------------------------------------------------------------------

/// Pausing blocks `cancel_contract` before any authorization check.
#[test]
fn pause_blocks_cancel_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (client_addr, _freelancer, id) = setup_created_contract(&env, &client);
    client.pause();

    super::assert_contract_error(
        client.try_cancel_contract(&id, &client_addr),
        Error::ContractPaused,
    );
}

/// After unpausing, `cancel_contract` on a zero-balance `Created` contract
/// completes without a token transfer.
#[test]
fn unpause_restores_cancel_contract() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    // Created-status, zero-balance: cancel skips the SAC transfer since refund_amount == 0.
    let (client_addr, _freelancer, id) = setup_created_contract(&env, &client);
    client.pause();
    client.unpause();

    assert!(client.cancel_contract(&id, &client_addr));
}

// ---------------------------------------------------------------------------
// submit_work_evidence
// ---------------------------------------------------------------------------

/// Pausing blocks `submit_work_evidence` before any state mutation.
#[test]
fn pause_blocks_submit_work_evidence() {
    let (env, contract_id, _admin) = setup_initialized();
    let client = EscrowClient::new(&env, &contract_id);
    let (_client_addr, freelancer_addr, id) = setup_created_contract(&env, &client);
    client.pause();

    let evidence = String::from_str(&env, "ipfs://QmPaused");
    super::assert_contract_error(
        client.try_submit_work_evidence(&id, &freelancer_addr, &0, &evidence),
        Error::ContractPaused,
    );
}

/// After unpausing, the freelancer can submit evidence on a funded milestone.
#[test]
fn unpause_restores_submit_work_evidence() {
    let (env, escrow_addr, _admin, _client_addr, freelancer_addr, id) = setup_funded_contract_env();
    let client = EscrowClient::new(&env, &escrow_addr);

    client.pause();
    client.unpause();

    let evidence = String::from_str(&env, "ipfs://QmUnpaused");
    assert!(client.submit_work_evidence(&id, &freelancer_addr, &0, &evidence));
}

// ---------------------------------------------------------------------------
// issue_reputation
// ---------------------------------------------------------------------------

/// Pausing blocks `issue_reputation` before any state mutation.
#[test]
fn pause_blocks_issue_reputation() {
    // Need a Completed contract — build via full fund + release cycle.
    let (env, escrow_addr, _admin, client_addr, _freelancer, id) = setup_funded_contract_env();
    let client = EscrowClient::new(&env, &escrow_addr);

    // Release both milestones to reach Completed status.
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    client.approve_milestone_release(&id, &client_addr, &1);
    client.release_milestone(&id, &client_addr, &1);

    client.pause();

    let comment = String::from_str(&env, "Great work");
    super::assert_contract_error(
        client.try_issue_reputation(&id, &client_addr, &5_u32, &comment),
        Error::ContractPaused,
    );
}
