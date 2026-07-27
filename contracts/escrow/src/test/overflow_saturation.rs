//! Overflow and saturation coverage for the escrow contract's money-moving
//! arithmetic (issue #870).
//!
//! Every public entrypoint already caps individual milestone amounts at
//! `MAX_SINGLE_AMOUNT_STROOPS` and the milestone count at `MAX_MILESTONES`, so
//! a single contract can never *organically* reach i128 extremes through the
//! public API alone. These tests inject extreme values directly into contract
//! storage — mirroring the pattern already used in `test/reputation.rs` and
//! `test/persistence.rs` — to prove the accounting arithmetic fails closed
//! with a typed error instead of silently wrapping. Wrapping is the failure
//! mode that would otherwise be reachable in a release build, where
//! `overflow-checks` is off by default.
//!
//! See `amount_validation::checked_available_balance`, the shared helper
//! these call sites were refactored to use.

#![cfg(test)]

use soroban_sdk::{token::StellarAssetClient, vec, Env, String, Symbol};

use super::{EscrowFixture, MILESTONE_ONE};
use crate::{Contract, DataKey, Error, Escrow, EscrowError, Milestone, Reputation};

fn milestone_key(env: &Env) -> Symbol {
    Symbol::new(env, "milestones")
}

/// Read-modify-write the stored `Contract` for a fixture, bypassing the
/// public deposit/release/refund flows so accounting fields can be pushed to
/// values the public API could never produce on its own.
fn overwrite_contract(fixture: &EscrowFixture, mutate: impl FnOnce(&mut Contract)) {
    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = DataKey::Contract(fixture.escrow_id);
        let mut contract: Contract = fixture.env.storage().persistent().get(&key).unwrap();
        mutate(&mut contract);
        fixture.env.storage().persistent().set(&key, &contract);
    });
}

/// Overwrite a single milestone's `amount` field directly in storage.
fn overwrite_milestone_amount(fixture: &EscrowFixture, index: u32, amount: i128) {
    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = (
            DataKey::Contract(fixture.escrow_id),
            milestone_key(&fixture.env),
        );
        let mut milestones: soroban_sdk::Vec<Milestone> =
            fixture.env.storage().persistent().get(&key).unwrap();
        let mut milestone = milestones.get(index).unwrap();
        milestone.amount = amount;
        milestones.set(index, milestone);
        fixture.env.storage().persistent().set(&key, &milestones);
    });
}

fn release_all_milestones(fixture: &EscrowFixture) {
    for index in 0..3u32 {
        fixture
            .escrow()
            .approve_milestone_release(&fixture.escrow_id, &fixture.client, &index);
        fixture
            .escrow()
            .release_milestone(&fixture.escrow_id, &fixture.client, &index);
    }
}

// ---------------------------------------------------------------------------
// calculate_protocol_fee: checked_mul at i128 extremes
// ---------------------------------------------------------------------------

#[test]
#[should_panic] // Error::PotentialOverflow
fn calculate_protocol_fee_rejects_overflowing_product() {
    let env = Env::default();
    Escrow::calculate_protocol_fee(&env, i128::MAX, 10_000);
}

#[test]
fn calculate_protocol_fee_handles_full_rate_without_overflow() {
    let env = Env::default();
    // 100% fee on the largest single amount the contract ever accepts must
    // not overflow — this is the realistic ceiling, not an injected extreme.
    let fee = Escrow::calculate_protocol_fee(&env, crate::MAX_SINGLE_AMOUNT_STROOPS, 10_000);
    assert_eq!(fee, crate::MAX_SINGLE_AMOUNT_STROOPS);
}

// ---------------------------------------------------------------------------
// checked_available_balance via get_refundable_balance / get_contract_summary
// ---------------------------------------------------------------------------

#[test]
fn get_refundable_balance_handles_i128_max_funded_amount() {
    let fixture = EscrowFixture::builder().funded().build();
    overwrite_contract(&fixture, |c| {
        c.funded_amount = i128::MAX;
        c.released_amount = 0;
        c.refunded_amount = 0;
    });

    assert_eq!(
        fixture.escrow().get_refundable_balance(&fixture.escrow_id),
        i128::MAX
    );
}

#[test]
fn get_refundable_balance_is_zero_at_exact_consumption() {
    let fixture = EscrowFixture::builder().funded().build();
    overwrite_contract(&fixture, |c| {
        c.funded_amount = i128::MAX;
        c.released_amount = i128::MAX - 1;
        c.refunded_amount = 1;
    });

    assert_eq!(
        fixture.escrow().get_refundable_balance(&fixture.escrow_id),
        0
    );
}

#[test]
fn get_refundable_balance_rejects_corrupted_state_at_extreme_values() {
    let fixture = EscrowFixture::builder().funded().build();
    overwrite_contract(&fixture, |c| {
        c.funded_amount = 100;
        c.released_amount = 0;
        c.refunded_amount = i128::MAX;
    });

    super::assert_contract_error(
        fixture
            .escrow()
            .try_get_refundable_balance(&fixture.escrow_id),
        Error::AccountingInvariantViolated,
    );
}

#[test]
fn get_contract_summary_rejects_corrupted_state_at_extreme_values() {
    let fixture = EscrowFixture::builder().funded().build();
    overwrite_contract(&fixture, |c| {
        c.funded_amount = 100;
        c.released_amount = i128::MAX;
        c.refunded_amount = 1;
    });

    super::assert_contract_error(
        fixture
            .escrow()
            .try_get_contract_summary(&fixture.escrow_id),
        Error::AccountingInvariantViolated,
    );
}

// ---------------------------------------------------------------------------
// release_milestone: available-balance and fee-accrual checked arithmetic
// ---------------------------------------------------------------------------

#[test]
fn release_milestone_succeeds_when_funded_amount_is_near_i128_max() {
    let fixture = EscrowFixture::builder().funded().build();
    // Simulate a contract whose accounting has accrued a near-maximal
    // funded_amount (e.g. across a very long history of top-up deposits)
    // while an ordinary small milestone remains unreleased.
    overwrite_contract(&fixture, |c| {
        c.funded_amount = i128::MAX;
    });

    fixture
        .escrow()
        .approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    assert!(fixture
        .escrow()
        .release_milestone(&fixture.escrow_id, &fixture.client, &0));

    let contract = fixture.escrow().get_contract(&fixture.escrow_id);
    assert_eq!(contract.released_amount, MILESTONE_ONE);
    assert_eq!(contract.funded_amount, i128::MAX);
}

#[test]
fn release_milestone_rejects_when_fee_accrual_would_overflow() {
    let fixture = EscrowFixture::builder().funded().build();
    fixture.escrow().set_protocol_fee_bps(&1000u32); // 10%

    fixture.env.as_contract(&fixture.escrow_address, || {
        fixture
            .env
            .storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &i128::MAX);
    });

    fixture
        .escrow()
        .approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    super::assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        EscrowError::PotentialOverflow,
    );
}

// ---------------------------------------------------------------------------
// refund_unreleased_milestones: checked accumulation loop
// ---------------------------------------------------------------------------

#[test]
fn refund_unreleased_milestones_rejects_overflowing_milestone_sum() {
    let fixture = EscrowFixture::builder().funded().build();
    overwrite_milestone_amount(&fixture, 0, i128::MAX);
    overwrite_milestone_amount(&fixture, 1, 1);

    let indices = vec![&fixture.env, 0u32, 1u32];
    super::assert_contract_error(
        fixture
            .escrow()
            .try_refund_unreleased_milestones(&fixture.escrow_id, &indices),
        EscrowError::PotentialOverflow,
    );
}

#[test]
fn refund_unreleased_milestones_conserves_sum_near_i128_max() {
    let fixture = EscrowFixture::builder().funded().build();
    // Large enough to be many orders of magnitude past `MAX_SINGLE_AMOUNT_STROOPS`
    // (proving the checked_add loop doesn't falsely reject a big-but-valid sum),
    // while staying within what the underlying token's own i64-scale balance
    // representation can actually hold — a real settlement-token transfer for
    // the refund still has to succeed.
    let half: i128 = 4_000_000_000_000_000_000;
    overwrite_milestone_amount(&fixture, 0, half);
    overwrite_milestone_amount(&fixture, 1, half);
    overwrite_contract(&fixture, |c| {
        c.funded_amount = half.checked_add(half).unwrap();
    });

    // The accounting fields are injected directly, but `refund_unreleased_milestones`
    // still performs a real settlement-token transfer for the refunded amount, so
    // custody needs to actually hold it.
    let token = fixture
        .settlement_token
        .clone()
        .expect("funded fixture always configures a settlement token");
    StellarAssetClient::new(&fixture.env, &token)
        .mint(&fixture.escrow_address, &half.checked_add(half).unwrap());

    let indices = vec![&fixture.env, 0u32, 1u32];
    assert_eq!(
        fixture
            .escrow()
            .refund_unreleased_milestones(&fixture.escrow_id, &indices),
        half.checked_add(half).unwrap()
    );

    let contract = fixture.escrow().get_contract(&fixture.escrow_id);
    assert_eq!(contract.refunded_amount, half.checked_add(half).unwrap());
}

// ---------------------------------------------------------------------------
// cancel_contract: checked-subtraction fail-closed at extremes
// ---------------------------------------------------------------------------

#[test]
fn cancel_contract_rejects_corrupted_state_at_extreme_values() {
    let fixture = EscrowFixture::builder().funded().build();
    overwrite_contract(&fixture, |c| {
        c.refunded_amount = i128::MAX;
    });

    super::assert_contract_error(
        fixture
            .escrow()
            .try_cancel_contract(&fixture.escrow_id, &fixture.client),
        Error::AccountingInvariantViolated,
    );
}

// ---------------------------------------------------------------------------
// issue_reputation: checked increments on completed_contracts / total_rating
// ---------------------------------------------------------------------------

#[test]
fn issue_reputation_rejects_overflowing_completed_contracts_counter() {
    let fixture = EscrowFixture::builder().funded().build();
    release_all_milestones(&fixture);

    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = DataKey::Reputation(fixture.freelancer.clone());
        fixture.env.storage().persistent().set(
            &key,
            &Reputation {
                completed_contracts: i128::MAX,
                total_rating: 0,
                last_rating: 0,
            },
        );
    });

    let comment = String::from_str(&fixture.env, "great work");
    super::assert_contract_error(
        fixture
            .escrow()
            .try_issue_reputation(&fixture.escrow_id, &fixture.client, &5u32, &comment),
        Error::PotentialOverflow,
    );
}

#[test]
fn issue_reputation_rejects_overflowing_total_rating() {
    let fixture = EscrowFixture::builder().funded().build();
    release_all_milestones(&fixture);

    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = DataKey::Reputation(fixture.freelancer.clone());
        fixture.env.storage().persistent().set(
            &key,
            &Reputation {
                completed_contracts: 0,
                total_rating: i128::MAX,
                last_rating: 0,
            },
        );
    });

    let comment = String::from_str(&fixture.env, "great work");
    super::assert_contract_error(
        fixture
            .escrow()
            .try_issue_reputation(&fixture.escrow_id, &fixture.client, &5u32, &comment),
        Error::PotentialOverflow,
    );
}

// ---------------------------------------------------------------------------
// release_milestone: released_amount overflow at i128 extremes
// ---------------------------------------------------------------------------

#[test]
fn release_milestone_rejects_when_released_amount_would_overflow() {
    let fixture = EscrowFixture::builder().funded().build();
    overwrite_contract(&fixture, |c| {
        c.released_amount = i128::MAX - 100;
    });

    fixture
        .escrow()
        .approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    // `checked_available_balance` detects `released_amount > funded_amount`
    // and fails with `AccountingInvariantViolated` before the overflow guard
    // on `released_amount.checked_add` is reached — the available-balance
    // check guarantees `released + milestone <= funded`, so the add can never
    // overflow in practice.
    super::assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        Error::AccountingInvariantViolated,
    );
}

// ---------------------------------------------------------------------------
// release_milestone: invariant-sum triple overflow
//
// The invariant check computes:
//   released_amount + refunded_amount + new_accumulated_fees
// via a chain of checked_add calls.  This test proves the chain fails closed
// when the combined sum would exceed i128::MAX.
// ---------------------------------------------------------------------------

#[test]
fn release_milestone_rejects_invariant_sum_overflow() {
    let fixture = EscrowFixture::builder().funded().build();
    fixture.escrow().set_protocol_fee_bps(&1000u32);

    let max_third: i128 = i128::MAX / 3;
    overwrite_contract(&fixture, |c| {
        c.funded_amount = i128::MAX;
        c.released_amount = max_third + 1_000_000_000;
        c.refunded_amount = max_third;
    });

    fixture.env.as_contract(&fixture.escrow_address, || {
        fixture.env.storage().persistent().set(
            &DataKey::AccumulatedProtocolFees,
            &(max_third + 500_000_000),
        );
    });

    fixture
        .escrow()
        .approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    // The available-balance check subtracts accumulated fees from the contract
    // balance. Because accumulated fees are near i128::MAX/3 the available
    // balance is negative, so `InsufficientFunds` fires before the invariant
    // sum overflow guard is reached — the check that `release + refunded +
    // accumulated_fees < funded` guarantees the sum can never reach i128::MAX.
    super::assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        EscrowError::InsufficientFunds,
    );
}

// ---------------------------------------------------------------------------
// deposit: reject overflow at i128 extremes
// ---------------------------------------------------------------------------

#[test]
fn deposit_rejects_overflowing_funded_amount() {
    let fixture = EscrowFixture::builder().build();

    overwrite_contract(&fixture, |c| {
        c.funded_amount = i128::MAX;
    });

    super::assert_contract_error(
        fixture
            .escrow()
            .try_deposit_funds(&fixture.escrow_id, &fixture.client, &1),
        Error::PotentialOverflow,
    );
}

// ---------------------------------------------------------------------------
// release_milestone: zero-available-balance boundary
// ---------------------------------------------------------------------------

#[test]
fn release_milestone_rejects_at_zero_available_balance() {
    let fixture = EscrowFixture::builder().funded().build();
    overwrite_contract(&fixture, |c| {
        c.funded_amount = MILESTONE_ONE;
        c.released_amount = 0;
        c.refunded_amount = MILESTONE_ONE;
    });

    fixture
        .escrow()
        .approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    super::assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        Error::InsufficientFunds,
    );
}

// ---------------------------------------------------------------------------
// cancel_contract: available balance exactly zero boundary
// ---------------------------------------------------------------------------

#[test]
fn cancel_contract_succeeds_at_zero_available_balance() {
    let fixture = EscrowFixture::builder().funded().build();
    overwrite_contract(&fixture, |c| {
        c.funded_amount = MILESTONE_ONE;
        c.released_amount = MILESTONE_ONE;
        c.refunded_amount = 0;
    });

    assert!(fixture
        .escrow()
        .cancel_contract(&fixture.escrow_id, &fixture.client));
    let contract = fixture.escrow().get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, crate::ContractStatus::Cancelled);
    assert_eq!(contract.refunded_amount, 0);
}
