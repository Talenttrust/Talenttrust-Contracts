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
