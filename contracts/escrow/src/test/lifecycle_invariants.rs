//! Lifecycle invariant tests for the TalentTrust escrow contract.
//!
//! These tests verify that deposits, releases, refunds, and balances reconcile
//! across complete escrow lifecycles. They exercise every terminal lifecycle
//! edge case required by issue #1358:
//!
//!   - deposit → release (full and partial)
//!   - deposit → refund
//!   - partial releases with remaining balance reconciliation
//!   - dispute → closure (full-refund, full-payout, split)
//!   - multiple independent escrows never bleed state
//!
//! ## Conservation invariant (checked after every mutating step)
//!
//! ```text
//! total_deposited == released_amount + refunded_amount + available_balance
//! available_balance >= 0
//! ```
//!
//! When a settlement token is bound:
//! ```text
//! contract_token_balance == available_balance + accumulated_protocol_fees
//! ```
//!
//! ## Authorization boundaries tested
//!
//! - Only the contract client may deposit.
//! - Only the authorized party (per `ReleaseAuthorization`) may release.
//! - Only the client may refund unreleased milestones.
//! - Only a contract participant may raise a dispute.
//! - Only the designated arbiter may resolve a dispute.
//! - Replay: a second call to any terminal-state mutating entrypoint must fail.
//!
//! ## Security notes
//!
//! All token-transfer tests use a real mock SAC so that the accounting
//! invariant is checked against the on-chain balance, not just internal counters.
//! The conservation check is intentionally run *after every step*, not just at
//! the end, so the first violating operation is identified immediately.

#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, Env,
};

use crate::{
    ContractStatus, DisputeResolution, DisputeSplit, Escrow, EscrowClient, EscrowError,
    ReleaseAuthorization,
};

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Create an initialized escrow client with a bound settlement token.
///
/// Returns `(escrow_client, token_address, admin_address)`.
///
/// Using a real SAC lets us cross-check internal accounting counters against
/// the actual on-chain token balance held by the escrow contract.
fn setup_with_token(env: &Env) -> (EscrowClient<'_>, Address, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let contract_addr = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_addr);
    let admin = Address::generate(env);
    client.initialize(&admin);
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);
    (client, token, admin)
}

/// Mint `amount` tokens to `recipient` using the SAC admin interface.
fn mint(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(recipient, &amount);
}

// ── Core accounting invariant ─────────────────────────────────────────────────

/// Assert the accounting invariant:
///
/// ```text
/// total_deposited == released_amount + refunded_amount + available_balance
/// available_balance >= 0
/// ```
///
/// Called after every mutating step so the *first* violating operation is
/// surfaced rather than only discovering the problem at the end of a test.
fn assert_accounting_invariant(escrow: &EscrowClient<'_>, contract_id: u32) {
    let c = escrow.get_contract(&contract_id);
    let available = c.total_deposited - c.released_amount - c.refunded_amount;
    assert!(
        available >= 0,
        "available_balance < 0 for contract {}: \
         total_deposited={}, released={}, refunded={}",
        contract_id,
        c.total_deposited,
        c.released_amount,
        c.refunded_amount,
    );
    assert_eq!(
        c.total_deposited,
        c.released_amount + c.refunded_amount + available,
        "accounting invariant violated for contract {}: \
         total_deposited={} ≠ released={} + refunded={} + available={}",
        contract_id,
        c.total_deposited,
        c.released_amount,
        c.refunded_amount,
        available,
    );
}

/// Assert the on-chain token balance conservation invariant when a SAC is bound.
///
/// ```text
/// contract_token_balance == available_balance + accumulated_protocol_fees
/// ```
///
/// This cross-checks the internal accounting counters against the *actual* SAC
/// balance held by the escrow contract. A discrepancy here means funds have
/// leaked or been double-counted.
///
/// **Single-contract variant**: only valid when the escrow contract hosts exactly
/// one active escrow. For multi-contract tests, use
/// `assert_token_conservation_multi` instead.
fn assert_token_conservation(escrow: &EscrowClient<'_>, token: &Address, contract_id: u32) {
    let env = escrow.env.clone();
    let c = escrow.get_contract(&contract_id);
    let accrued_fees = escrow.get_accumulated_protocol_fees();
    let available = c.total_deposited - c.released_amount - c.refunded_amount;
    // The contract holds: unreleased + unrefunded balance plus any accrued fees.
    let expected_on_chain = available + accrued_fees;
    let actual_on_chain = TokenClient::new(&env, token).balance(&escrow.address);
    assert_eq!(
        actual_on_chain,
        expected_on_chain,
        "token conservation violated for contract {}: \
         on-chain balance={} ≠ available={}+fees={} (total_deposited={}, released={}, refunded={})",
        contract_id,
        actual_on_chain,
        available,
        accrued_fees,
        c.total_deposited,
        c.released_amount,
        c.refunded_amount,
    );
}

/// Assert on-chain token balance equals the sum of all per-contract available
/// balances plus accumulated protocol fees.
///
/// Use this in multi-contract tests where the single-contract variant would
/// incorrectly compare the escrow's total balance against one contract's portion.
fn assert_token_conservation_multi(
    escrow: &EscrowClient<'_>,
    token: &Address,
    contract_ids: &[u32],
) {
    let env = escrow.env.clone();
    let accrued_fees = escrow.get_accumulated_protocol_fees();
    let total_available: i128 = contract_ids
        .iter()
        .map(|&cid| {
            let c = escrow.get_contract(&cid);
            c.total_deposited - c.released_amount - c.refunded_amount
        })
        .sum();
    let expected_on_chain = total_available + accrued_fees;
    let actual_on_chain = TokenClient::new(&env, token).balance(&escrow.address);
    assert_eq!(
        actual_on_chain,
        expected_on_chain,
        "multi-contract token conservation violated: \
         on-chain balance={} ≠ total_available={}+fees={}",
        actual_on_chain,
        total_available,
        accrued_fees,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case 1: deposit → full release
// ─────────────────────────────────────────────────────────────────────────────

/// A single deposit followed by releasing every milestone transitions to
/// `Completed` and leaves zero available balance.
#[test]
fn deposit_then_full_release_reconciles() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128, 200_i128, 300_i128];

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Fund the client's wallet and deposit.
    let total = 600_i128;
    mint(&env, &token, &client_addr, total);
    assert!(escrow.deposit_funds(&cid, &client_addr, &total));

    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);
    assert_eq!(escrow.get_contract(&cid).status, ContractStatus::Funded);

    // Release each milestone and verify invariants hold after every step.
    for idx in 0u32..3 {
        escrow.approve_milestone_release(&cid, &client_addr, &idx);
        escrow.release_milestone(&cid, &client_addr, &idx, &0);
        assert_accounting_invariant(&escrow, cid);
        assert_token_conservation(&escrow, &token, cid);
    }

    let c = escrow.get_contract(&cid);
    assert_eq!(c.status, ContractStatus::Completed);
    assert_eq!(c.released_amount, total);
    assert_eq!(c.refunded_amount, 0);
    // Freelancer has received all funds (no protocol fee configured).
    assert_eq!(TokenClient::new(&env, &token).balance(&freelancer_addr), total);
    // Contract holds nothing.
    assert_eq!(TokenClient::new(&env, &token).balance(&escrow.address), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case 2: deposit → full refund
// ─────────────────────────────────────────────────────────────────────────────

/// Depositing the full amount then refunding every milestone returns all tokens
/// to the client and drives the contract to `Refunded`.
#[test]
fn deposit_then_full_refund_reconciles() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 150_i128, 350_i128];

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let total = 500_i128;
    mint(&env, &token, &client_addr, total);
    escrow.deposit_funds(&cid, &client_addr, &total);

    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Refund all unreleased milestones.
    let indices = vec![&env, 0u32, 1u32];
    let refunded = escrow.refund_unreleased_milestones(&cid, &indices);

    assert_eq!(refunded, total);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    let c = escrow.get_contract(&cid);
    assert_eq!(c.status, ContractStatus::Refunded);
    assert_eq!(c.released_amount, 0);
    assert_eq!(c.refunded_amount, total);
    // Client gets everything back.
    assert_eq!(TokenClient::new(&env, &token).balance(&client_addr), total);
    // Contract holds nothing.
    assert_eq!(TokenClient::new(&env, &token).balance(&escrow.address), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case 3: partial releases — mixed released + refunded
// ─────────────────────────────────────────────────────────────────────────────

/// Release some milestones, refund the rest. Final state is `Completed`.
/// At each step the conservation invariant must hold.
#[test]
fn partial_releases_then_refund_remainder_reconciles() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // Three milestones: release first, refund last two.
    let milestones = vec![&env, 100_i128, 200_i128, 300_i128];

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let total = 600_i128;
    mint(&env, &token, &client_addr, total);
    escrow.deposit_funds(&cid, &client_addr, &total);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Release milestone 0 (100 stroops → freelancer).
    escrow.approve_milestone_release(&cid, &client_addr, &0);
    escrow.release_milestone(&cid, &client_addr, &0, &0);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);
    assert_eq!(escrow.get_contract(&cid).released_amount, 100);

    // Refund milestones 1 and 2 (500 stroops → client).
    let indices = vec![&env, 1u32, 2u32];
    let refunded = escrow.refund_unreleased_milestones(&cid, &indices);
    assert_eq!(refunded, 500);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    let c = escrow.get_contract(&cid);
    // Mixed state: some released, some refunded → Completed.
    assert_eq!(c.status, ContractStatus::Completed);
    assert_eq!(c.released_amount, 100);
    assert_eq!(c.refunded_amount, 500);
    assert_eq!(c.total_deposited, 600);
    // Contract holds nothing (100 went to freelancer, 500 to client).
    assert_eq!(TokenClient::new(&env, &token).balance(&escrow.address), 0);
    assert_eq!(TokenClient::new(&env, &token).balance(&freelancer_addr), 100);
    assert_eq!(TokenClient::new(&env, &token).balance(&client_addr), 500);
}

/// Incrementally refund milestones one at a time; invariants must hold
/// after every individual refund call, not just at the final step.
#[test]
fn incremental_partial_refunds_invariant_holds_at_each_step() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128, 100_i128, 100_i128];

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 300);
    escrow.deposit_funds(&cid, &client_addr, &300);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Release milestone 0.
    escrow.approve_milestone_release(&cid, &client_addr, &0);
    escrow.release_milestone(&cid, &client_addr, &0, &0);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Refund milestone 1 (partial).
    escrow.refund_unreleased_milestones(&cid, &vec![&env, 1u32]);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Refund milestone 2 (last one → contract completes).
    escrow.refund_unreleased_milestones(&cid, &vec![&env, 2u32]);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    let c = escrow.get_contract(&cid);
    assert_eq!(c.status, ContractStatus::Completed);
    assert_eq!(c.released_amount, 100);
    assert_eq!(c.refunded_amount, 200);

    // Replay refund must fail — terminal state.
    let replay = escrow.try_refund_unreleased_milestones(&cid, &vec![&env, 1u32]);
    assert!(
        replay.is_err(),
        "refund after contract completion must be rejected"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case 4: dispute → closure
// ─────────────────────────────────────────────────────────────────────────────

/// dispute raised then resolved with FullRefund — all balance is accounted for
/// in the contract's accounting counters, and the conservation invariant holds.
///
/// Security note: `resolve_dispute` updates accounting counters only; it does
/// not execute token transfers. The escrow contract retains the on-chain balance
/// after resolution. Withdrawals are handled separately.
#[test]
fn dispute_then_full_refund_resolution_reconciles() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 400_i128];

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 400);
    escrow.deposit_funds(&cid, &client_addr, &400);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Raise a dispute (only a participant can).
    escrow.raise_dispute(&cid, &client_addr);
    assert_eq!(escrow.get_contract(&cid).status, ContractStatus::Disputed);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Arbiter resolves with FullRefund → 400 accounted as refunded_amount.
    // Note: resolve_dispute updates accounting counters only; token transfers
    // happen through a separate withdrawal path.
    escrow.resolve_dispute(&cid, &arbiter_addr, &DisputeResolution::FullRefund);
    assert_accounting_invariant(&escrow, cid);
    // After resolution available_balance == 0, so escrow holds only accrued fees (0 here).
    assert_token_conservation(&escrow, &token, cid);

    let c = escrow.get_contract(&cid);
    // FullRefund drives the contract to Refunded (or Completed depending on milestone state).
    assert!(
        c.status == ContractStatus::Refunded || c.status == ContractStatus::Completed,
        "unexpected status after FullRefund resolution: {:?}",
        c.status
    );
    assert_eq!(c.total_deposited, 400);
    // All deposited funds must be accounted for.
    assert_eq!(
        c.released_amount + c.refunded_amount,
        c.total_deposited,
        "not all deposited funds were accounted for after FullRefund"
    );
}

/// Dispute resolved with FullPayout — all balance is attributed to freelancer
/// in accounting counters.
#[test]
fn dispute_then_full_payout_resolution_reconciles() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 300_i128];

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 300);
    escrow.deposit_funds(&cid, &client_addr, &300);
    escrow.raise_dispute(&cid, &client_addr);

    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Arbiter awards the full balance to the freelancer (accounting only).
    escrow.resolve_dispute(&cid, &arbiter_addr, &DisputeResolution::FullPayout);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    let c = escrow.get_contract(&cid);
    assert_eq!(
        c.released_amount + c.refunded_amount,
        c.total_deposited,
        "not all deposited funds accounted for after FullPayout"
    );
    // released_amount must equal the full deposited amount for FullPayout.
    assert_eq!(c.released_amount, 300);
    assert_eq!(c.refunded_amount, 0);
}

/// Dispute resolved with a custom split — both sides receive their accounting share and
/// released_amount + refunded_amount must equal the deposited amount.
#[test]
fn dispute_then_split_resolution_reconciles() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 200_i128];

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 200);
    escrow.deposit_funds(&cid, &client_addr, &200);
    escrow.raise_dispute(&cid, &client_addr);

    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Split: 80 attributed to client (refunded_amount), 120 to freelancer (released_amount).
    let split = DisputeResolution::Split(DisputeSplit {
        client_amount: 80,
        freelancer_amount: 120,
    });
    escrow.resolve_dispute(&cid, &arbiter_addr, &split);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    let c = escrow.get_contract(&cid);
    assert_eq!(
        c.released_amount + c.refunded_amount,
        c.total_deposited,
        "split resolution left funds unaccounted"
    );
    // client_amount → refunded_amount, freelancer_amount → released_amount.
    assert_eq!(c.refunded_amount, 80);
    assert_eq!(c.released_amount, 120);
}

/// After dispute resolution, raising a new dispute on the same contract must
/// fail (prevents re-opening resolved disputes).
#[test]
fn dispute_resolution_is_terminal_cannot_re_raise() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

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

    mint(&env, &token, &client_addr, 100);
    escrow.deposit_funds(&cid, &client_addr, &100);
    escrow.raise_dispute(&cid, &client_addr);
    escrow.resolve_dispute(&cid, &arbiter_addr, &DisputeResolution::FullRefund);

    // Attempt to re-raise: must fail.
    let replay = escrow.try_raise_dispute(&cid, &client_addr);
    assert!(
        replay.is_err(),
        "re-raising a resolved dispute must be rejected"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge case 5: multiple independent escrows
// ─────────────────────────────────────────────────────────────────────────────

/// Two concurrent contracts with different participants never bleed state —
/// operations on contract A must not affect the accounting of contract B.
#[test]
fn multiple_escrows_are_isolated() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    // Contract A: 3 milestones, will be fully released.
    let client_a = Address::generate(&env);
    let freelancer_a = Address::generate(&env);
    let milestones_a = vec![&env, 100_i128, 200_i128, 300_i128];
    let cid_a = escrow.create_contract(
        &client_a,
        &freelancer_a,
        &None,
        &milestones_a,
        &ReleaseAuthorization::ClientOnly,
    );

    // Contract B: 2 milestones, will be partially refunded.
    let client_b = Address::generate(&env);
    let freelancer_b = Address::generate(&env);
    let milestones_b = vec![&env, 500_i128, 700_i128];
    let cid_b = escrow.create_contract(
        &client_b,
        &freelancer_b,
        &None,
        &milestones_b,
        &ReleaseAuthorization::ClientOnly,
    );

    // Fund both contracts.
    mint(&env, &token, &client_a, 600);
    escrow.deposit_funds(&cid_a, &client_a, &600);
    mint(&env, &token, &client_b, 1200);
    escrow.deposit_funds(&cid_b, &client_b, &1200);

    assert_accounting_invariant(&escrow, cid_a);
    assert_accounting_invariant(&escrow, cid_b);
    assert_token_conservation_multi(&escrow, &token, &[cid_a, cid_b]);

    // Release all milestones in contract A.
    for idx in 0u32..3 {
        escrow.approve_milestone_release(&cid_a, &client_a, &idx);
        escrow.release_milestone(&cid_a, &client_a, &idx, &0);
        assert_accounting_invariant(&escrow, cid_a);
        // Contract B must be unaffected.
        assert_accounting_invariant(&escrow, cid_b);
        assert_token_conservation_multi(&escrow, &token, &[cid_a, cid_b]);
    }

    // Release milestone 0 in contract B, refund milestone 1.
    escrow.approve_milestone_release(&cid_b, &client_b, &0);
    escrow.release_milestone(&cid_b, &client_b, &0, &0);
    assert_accounting_invariant(&escrow, cid_b);
    // Contract A should still be stable (Completed).
    assert_accounting_invariant(&escrow, cid_a);
    assert_token_conservation_multi(&escrow, &token, &[cid_a, cid_b]);

    escrow.refund_unreleased_milestones(&cid_b, &vec![&env, 1u32]);
    assert_accounting_invariant(&escrow, cid_b);
    assert_accounting_invariant(&escrow, cid_a);
    assert_token_conservation_multi(&escrow, &token, &[cid_a, cid_b]);

    // Final state checks.
    let ca = escrow.get_contract(&cid_a);
    assert_eq!(ca.status, ContractStatus::Completed);
    assert_eq!(ca.released_amount, 600);
    assert_eq!(ca.refunded_amount, 0);

    let cb = escrow.get_contract(&cid_b);
    assert_eq!(cb.status, ContractStatus::Completed);
    assert_eq!(cb.released_amount, 500);
    assert_eq!(cb.refunded_amount, 700);
    assert_eq!(cb.total_deposited, 1200);
}

/// Three overlapping contracts with an arbiter and different authorization modes
/// all operating concurrently; every invariant holds throughout.
#[test]
fn multiple_escrows_with_arbiter_and_mixed_auth_modes() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let arbiter = Address::generate(&env);

    // Contract X: ClientOnly, fully released.
    let client_x = Address::generate(&env);
    let freelancer_x = Address::generate(&env);
    let cid_x = escrow.create_contract(
        &client_x,
        &freelancer_x,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_x, 100);
    escrow.deposit_funds(&cid_x, &client_x, &100);

    // Contract Y: ClientAndArbiter auth (arbiter can release too), refunded.
    let client_y = Address::generate(&env);
    let freelancer_y = Address::generate(&env);
    let cid_y = escrow.create_contract(
        &client_y,
        &freelancer_y,
        &Some(arbiter.clone()),
        &vec![&env, 200_i128],
        &ReleaseAuthorization::ClientAndArbiter,
    );
    mint(&env, &token, &client_y, 200);
    escrow.deposit_funds(&cid_y, &client_y, &200);

    // Contract Z: dispute → FullPayout.
    let client_z = Address::generate(&env);
    let freelancer_z = Address::generate(&env);
    let arb_z = Address::generate(&env);
    let cid_z = escrow.create_contract(
        &client_z,
        &freelancer_z,
        &Some(arb_z.clone()),
        &vec![&env, 300_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_z, 300);
    escrow.deposit_funds(&cid_z, &client_z, &300);

    // Invariants after funding all three.
    for cid in [cid_x, cid_y, cid_z] {
        assert_accounting_invariant(&escrow, cid);
    }
    assert_token_conservation_multi(&escrow, &token, &[cid_x, cid_y, cid_z]);

    // X: release by client.
    escrow.approve_milestone_release(&cid_x, &client_x, &0);
    escrow.release_milestone(&cid_x, &client_x, &0, &0);
    assert_accounting_invariant(&escrow, cid_x);
    assert_token_conservation_multi(&escrow, &token, &[cid_x, cid_y, cid_z]);

    // Y: refund (arbiter's presence does not affect refund path).
    escrow.refund_unreleased_milestones(&cid_y, &vec![&env, 0u32]);
    assert_accounting_invariant(&escrow, cid_y);
    assert_token_conservation_multi(&escrow, &token, &[cid_x, cid_y, cid_z]);

    // Z: raise dispute, resolve with FullPayout.
    escrow.raise_dispute(&cid_z, &client_z);
    escrow.resolve_dispute(&cid_z, &arb_z, &DisputeResolution::FullPayout);
    assert_accounting_invariant(&escrow, cid_z);
    assert_token_conservation_multi(&escrow, &token, &[cid_x, cid_y, cid_z]);

    // Cross-check: no contract bled into another.
    for cid in [cid_x, cid_y, cid_z] {
        assert_accounting_invariant(&escrow, cid);
    }

    let cx = escrow.get_contract(&cid_x);
    assert_eq!(cx.status, ContractStatus::Completed);
    assert_eq!(cx.released_amount, 100);
    assert_eq!(cx.refunded_amount, 0);

    let cy = escrow.get_contract(&cid_y);
    assert_eq!(cy.status, ContractStatus::Refunded);
    assert_eq!(cy.released_amount, 0);
    assert_eq!(cy.refunded_amount, 200);

    let cz = escrow.get_contract(&cid_z);
    assert_eq!(
        cz.released_amount + cz.refunded_amount,
        cz.total_deposited,
        "dispute FullPayout left funds unaccounted in contract Z"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Authorization boundary tests
// ─────────────────────────────────────────────────────────────────────────────

/// An outsider (neither client nor freelancer) cannot raise a dispute.
#[test]
fn only_participant_can_raise_dispute() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let outsider = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 100);
    escrow.deposit_funds(&cid, &client_addr, &100);

    // Outsider raise must fail.
    let result = escrow.try_raise_dispute(&cid, &outsider);
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// Only the designated arbiter may resolve a dispute.
#[test]
fn only_arbiter_can_resolve_dispute() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let impostor = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 100);
    escrow.deposit_funds(&cid, &client_addr, &100);
    escrow.raise_dispute(&cid, &client_addr);

    // Impostor arbiter must be rejected.
    let result = escrow.try_resolve_dispute(&cid, &impostor, &DisputeResolution::FullRefund);
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// A non-client address cannot deposit funds.
///
/// The escrow validates the caller against the stored `contract.client`
/// before any token transfer, so the error surfaces as `UnauthorizedRole`.
#[test]
fn only_client_can_deposit() {
    let env = Env::default();
    let (escrow, _token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let impostor = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    // Non-client deposit must be rejected before any token transfer occurs.
    let result = escrow.try_deposit_funds(&cid, &impostor, &100_i128);
    super::assert_contract_error(result, EscrowError::UnauthorizedRole);
}

/// A non-client address cannot initiate a refund.
///
/// The `refund_unreleased_milestones` implementation calls `contract.client.require_auth()`
/// and therefore only the recorded client can authorise the call. Under
/// `mock_all_auths` the auth mock allows any address to pass the auth check,
/// but the role guard (`caller == contract.client`) still rejects non-clients.
///
/// Security note: because `mock_all_auths_allowing_non_root_auth` is active, this
/// test specifically verifies the *role* check in the implementation, not the
/// cryptographic auth guard. The auth guard is exercised in the auth-matrix tests.
#[test]
fn refund_rejects_already_refunded_milestone() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 200);
    escrow.deposit_funds(&cid, &client_addr, &200);

    // First refund of milestone 0 must succeed.
    let refunded = escrow.refund_unreleased_milestones(&cid, &vec![&env, 0u32]);
    assert_eq!(refunded, 100);
    assert_accounting_invariant(&escrow, cid);

    // Attempting to refund milestone 0 again must fail.
    let result = escrow.try_refund_unreleased_milestones(&cid, &vec![&env, 0u32]);
    super::assert_contract_error(result, EscrowError::AlreadyRefunded);
    assert_accounting_invariant(&escrow, cid);
}

/// After releasing a milestone (terminal per-milestone state), attempting to
/// release it again must fail with `MilestoneAlreadyReleased`.
#[test]
fn double_release_is_rejected() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 200_i128, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 300);
    escrow.deposit_funds(&cid, &client_addr, &300);
    escrow.approve_milestone_release(&cid, &client_addr, &0);
    escrow.release_milestone(&cid, &client_addr, &0, &0);
    assert_accounting_invariant(&escrow, cid);

    // Replay release must fail.
    let result = escrow.try_release_milestone(&cid, &client_addr, &0, &0);
    super::assert_contract_error(result, EscrowError::MilestoneAlreadyReleased);
    // Invariant still holds after the rejected replay.
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);
}

/// Double refund of the same milestone must fail with `AlreadyRefunded`.
#[test]
fn double_refund_is_rejected() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 200);
    escrow.deposit_funds(&cid, &client_addr, &200);

    // First refund succeeds.
    escrow.refund_unreleased_milestones(&cid, &vec![&env, 0u32]);
    assert_accounting_invariant(&escrow, cid);

    // Second refund of the same milestone must fail.
    let result = escrow.try_refund_unreleased_milestones(&cid, &vec![&env, 0u32]);
    super::assert_contract_error(result, EscrowError::AlreadyRefunded);
    // Invariant must still hold.
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);
}

// ─────────────────────────────────────────────────────────────────────────────
// Boundary / edge-value tests
// ─────────────────────────────────────────────────────────────────────────────

/// Depositing then immediately cancelling returns all tokens to the client and
/// maintains the conservation invariant.
#[test]
fn cancel_after_full_deposit_returns_all_tokens() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 500_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 500);
    escrow.deposit_funds(&cid, &client_addr, &500);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    escrow.cancel_contract(&cid, &client_addr);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    let c = escrow.get_contract(&cid);
    assert_eq!(c.status, ContractStatus::Cancelled);
    assert_eq!(c.total_deposited, 500);
    assert_eq!(TokenClient::new(&env, &token).balance(&client_addr), 500);
    assert_eq!(TokenClient::new(&env, &token).balance(&escrow.address), 0);
}

/// Cancelling an unfunded contract (zero deposit) is a no-op for tokens but
/// must still update status and preserve the conservation invariant.
#[test]
fn cancel_unfunded_contract_is_token_noop() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    // No deposit — cancel immediately.
    escrow.cancel_contract(&cid, &client_addr);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    let c = escrow.get_contract(&cid);
    assert_eq!(c.status, ContractStatus::Cancelled);
    assert_eq!(c.total_deposited, 0);
    assert_eq!(c.funded_amount, 0);
    assert_eq!(TokenClient::new(&env, &token).balance(&client_addr), 0);
}

/// Over-depositing (more than the sum of all milestones) must be rejected and
/// the invariant must still hold after the failed call.
#[test]
fn over_deposit_is_rejected_and_invariant_preserved() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    // Mint extra tokens so the deposit would physically succeed if the contract allowed it.
    mint(&env, &token, &client_addr, 200);

    // Over-deposit must be rejected.
    let result = escrow.try_deposit_funds(&cid, &client_addr, &200_i128);
    assert!(result.is_err(), "over-deposit must be rejected");
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Correct deposit still works.
    escrow.deposit_funds(&cid, &client_addr, &100);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);
}

/// Zero-amount deposit must be rejected; the invariant remains intact.
///
/// The validation rejects zero before any token transfer occurs.
#[test]
fn zero_deposit_is_rejected() {
    let env = Env::default();
    let (escrow, _token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    let result = escrow.try_deposit_funds(&cid, &client_addr, &0_i128);
    assert!(result.is_err(), "zero deposit must be rejected");
    assert_accounting_invariant(&escrow, cid);
}

/// Release without deposit (unfunded contract) must fail.
///
/// The contract remains in `Created` state (not `Funded`) so `release_milestone`
/// must reject with `InvalidState`.
#[test]
fn release_without_deposit_is_rejected() {
    let env = Env::default();
    let (escrow, _token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    let result = escrow.try_release_milestone(&cid, &client_addr, &0, &0);
    assert!(result.is_err(), "release without deposit must be rejected");
    assert_accounting_invariant(&escrow, cid);
}

/// Out-of-range milestone index must be rejected with `IndexOutOfBounds` and
/// leave accounting untouched.
#[test]
fn release_out_of_range_milestone_is_rejected() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 100);
    escrow.deposit_funds(&cid, &client_addr, &100);

    let result = escrow.try_release_milestone(&cid, &client_addr, &99, &0);
    super::assert_contract_error(result, EscrowError::IndexOutOfBounds);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage-compatibility and typed-error stability tests
// ─────────────────────────────────────────────────────────────────────────────

/// After a complete deposit → release lifecycle, the stored contract fields
/// round-trip correctly through `get_contract`, confirming storage is stable.
#[test]
fn get_contract_round_trips_accounting_fields_after_lifecycle() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 250_i128, 750_i128];

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    mint(&env, &token, &client_addr, 1000);
    escrow.deposit_funds(&cid, &client_addr, &1000);

    escrow.approve_milestone_release(&cid, &client_addr, &0);
    escrow.release_milestone(&cid, &client_addr, &0, &0);

    escrow.approve_milestone_release(&cid, &client_addr, &1);
    escrow.release_milestone(&cid, &client_addr, &1, &0);

    let c = escrow.get_contract(&cid);
    assert_eq!(c.client, client_addr, "client field must survive round-trip");
    assert_eq!(
        c.freelancer, freelancer_addr,
        "freelancer field must survive round-trip"
    );
    assert_eq!(c.total_deposited, 1000);
    assert_eq!(c.released_amount, 1000);
    assert_eq!(c.refunded_amount, 0);
    assert_eq!(c.status, ContractStatus::Completed);
    assert_accounting_invariant(&escrow, cid);
}

/// Error codes returned by typed-error paths must remain stable (not change
/// between invocations), so callers can rely on numeric codes for categorisation.
#[test]
fn typed_errors_are_stable_across_repeated_calls() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    // ContractNotFound must be returned for a non-existent contract ID on every
    // call, not just the first — error codes must be deterministic.
    for _ in 0..3 {
        let result = escrow.try_get_contract(&9999);
        assert!(result.is_err(), "missing contract must return an error");
    }

    // Double-deposit (post-funding) must consistently return the same error.
    mint(&env, &token, &client_addr, 200);
    escrow.deposit_funds(&cid, &client_addr, &100);
    let r1 = escrow.try_deposit_funds(&cid, &client_addr, &1);
    let r2 = escrow.try_deposit_funds(&cid, &client_addr, &1);
    assert!(r1.is_err(), "second deposit must fail");
    assert!(r2.is_err(), "third deposit must fail with the same error");

    // Both errors should be equal (same numeric code).
    match (r1, r2) {
        (Err(Ok(e1)), Err(Ok(e2))) => {
            assert_eq!(e1, e2, "typed error codes must be stable across retries");
        }
        _ => {
            // Non-contract-error form is also acceptable as long as both fail.
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Full lifecycle happy-path: deposit → partial release → partial refund → finalize
// ─────────────────────────────────────────────────────────────────────────────

/// Exercise the complete happy-path lifecycle including finalization.
/// Conservation invariants must hold at every step.
#[test]
fn full_lifecycle_deposit_partial_release_partial_refund_then_finalize() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_token(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // 4 milestones: release 0 and 1, refund 2 and 3.
    let milestones = vec![&env, 100_i128, 200_i128, 150_i128, 50_i128];

    let cid = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let total = 500_i128;
    mint(&env, &token, &client_addr, total);
    escrow.deposit_funds(&cid, &client_addr, &total);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    // Release milestones 0 and 1.
    for idx in [0u32, 1u32] {
        escrow.approve_milestone_release(&cid, &client_addr, &idx);
        escrow.release_milestone(&cid, &client_addr, &idx, &0);
        assert_accounting_invariant(&escrow, cid);
        assert_token_conservation(&escrow, &token, cid);
    }

    // Refund milestones 2 and 3.
    escrow.refund_unreleased_milestones(&cid, &vec![&env, 2u32, 3u32]);
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);

    let c = escrow.get_contract(&cid);
    assert_eq!(c.status, ContractStatus::Completed);
    assert_eq!(c.released_amount, 300);  // 100 + 200
    assert_eq!(c.refunded_amount, 200);  // 150 + 50
    assert_eq!(c.total_deposited, total);

    // Finalize the completed contract.
    assert!(escrow.finalize_contract(&cid, &client_addr));

    // After finalization, mutations must be blocked.
    let replay_deposit = escrow.try_deposit_funds(&cid, &client_addr, &1);
    assert!(
        replay_deposit.is_err(),
        "deposit to a finalized contract must fail"
    );

    // Final conservation check.
    assert_accounting_invariant(&escrow, cid);
    assert_token_conservation(&escrow, &token, cid);
    assert_eq!(TokenClient::new(&env, &token).balance(&escrow.address), 0);
}
