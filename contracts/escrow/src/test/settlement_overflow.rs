//! Overflow and saturation tests for settlement arithmetic (#895).
//!
//! Covers all arithmetic hot-paths in dispute resolution payouts and fund
//! accumulation that execute during settlement:
//!
//! | Path                             | Operation                        | Fix                    |
//! |----------------------------------|----------------------------------|------------------------|
//! | `resolution_payouts` FullRefund  | `available` pass-through         | n/a (no arithmetic)    |
//! | `resolution_payouts` FullPayout  | `available` pass-through         | n/a (no arithmetic)    |
//! | `resolution_payouts` PartialRef  | `available * 30 / 100`           | `checked_mul`/`checked_div` |
//! | `resolution_payouts` Split       | `client + freelancer`            | `checked_add`          |
//! | `resolve_dispute` accounting     | `refunded += client_payout`      | `checked_add`          |
//! | `resolve_dispute` accounting     | `released += freelancer_payout`  | `checked_add`          |
//! | `refund_unreleased_milestones`   | `refunded += total_refund`       | `checked_add`          |
//! | `accumulate_amounts`             | milestone total summation        | `checked_add` chain    |
//!
//! Test categories:
//! - i128 extremes: `i128::MAX`, `i128::MIN`
//! - Sum near max: values close to `i128::MAX`
//! - Subtraction near zero: boundary at 0 and below
//! - Conservation invariant: payout sums always equal available at extremes

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env};

use crate::{
    safe_add_amounts, safe_subtract_amounts, validate_single_amount,
    Contract, ContractStatus, DisputeResolution, DisputeSplit, Error, EscrowError,
    ReleaseAuthorization, MAX_SINGLE_AMOUNT_STROOPS,
};

use super::{assert_contract_error, EscrowFixture};

// ── Shared helpers ────────────────────────────────────────────────────────────

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env
}

/// Build a `Contract` struct with controlled accounting fields for unit-level
/// arithmetic tests. `funded` is stored as both `total_deposited` and
/// `funded_amount`.
fn payout_contract(env: &Env, funded: i128, released: i128, refunded: i128) -> Contract {
    Contract {
        client: Address::generate(env),
        freelancer: Address::generate(env),
        arbiter: Some(Address::generate(env)),
        status: ContractStatus::Disputed,
        total_deposited: funded,
        funded_amount: funded,
        released_amount: released,
        refunded_amount: refunded,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    }
}

/// Helper to overwrite specific fields in a fixture's contract in storage.
fn overwrite_contract<F: FnOnce(&mut Contract)>(fixture: &EscrowFixture, f: F) {
    let mut contract = fixture.escrow().get_contract(&fixture.escrow_id);
    f(&mut contract);
    fixture.env.as_contract(&fixture.escrow_address, || {
        fixture
            .env
            .storage()
            .persistent()
            .set(&crate::DataKey::Contract(fixture.escrow_id), &contract);
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. resolution_payouts — FullRefund at extreme values
// ═══════════════════════════════════════════════════════════════════════════════

/// FullRefund with `i128::MAX` available succeeds — no arithmetic needed.
#[test]
fn full_refund_i128_max_available_succeeds() {
    let env = make_env();
    let contract = payout_contract(&env, i128::MAX, 0, 0);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::FullRefund);
    assert_eq!(
        result,
        Ok(crate::DisputeInfo {
            available_balance: i128::MAX,
            client_payout: i128::MAX,
            freelancer_payout: 0,
        })
    );
}

/// FullRefund with zero available (fully distributed) succeeds.
#[test]
fn full_refund_zero_available_succeeds() {
    let env = make_env();
    let contract = payout_contract(&env, 1_000, 500, 500);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::FullRefund);
    assert_eq!(
        result,
        Ok(crate::DisputeInfo {
            available_balance: 0,
            client_payout: 0,
            freelancer_payout: 0,
        })
    );
}

/// FullRefund correctly computes available with prior releases/refunds.
#[test]
fn full_refund_correct_available_with_releases_and_refunds() {
    let env = make_env();
    // funded=1000, released=200, refunded=300 → available = 500
    let contract = payout_contract(&env, 1_000, 200, 300);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::FullRefund);
    assert_eq!(
        result,
        Ok(crate::DisputeInfo {
            available_balance: 500,
            client_payout: 500,
            freelancer_payout: 0,
        })
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. resolution_payouts — FullPayout at extreme values
// ═══════════════════════════════════════════════════════════════════════════════

/// FullPayout with `i128::MAX` succeeds.
#[test]
fn full_payout_i128_max_available_succeeds() {
    let env = make_env();
    let contract = payout_contract(&env, i128::MAX, 0, 0);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::FullPayout);
    assert_eq!(
        result,
        Ok(crate::DisputeInfo {
            available_balance: i128::MAX,
            client_payout: 0,
            freelancer_payout: i128::MAX,
        })
    );
}

/// FullPayout with zero available succeeds.
#[test]
fn full_payout_zero_available_succeeds() {
    let env = make_env();
    let contract = payout_contract(&env, 500, 0, 500);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::FullPayout);
    assert_eq!(
        result,
        Ok(crate::DisputeInfo {
            available_balance: 0,
            client_payout: 0,
            freelancer_payout: 0,
        })
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. resolution_payouts — PartialRefund at extreme values
// ═══════════════════════════════════════════════════════════════════════════════

/// PartialRefund with `i128::MAX` available overflows `i128::MAX * 30` and must
/// return `PotentialOverflow`.
#[test]
fn partial_refund_i128_max_overflows() {
    let env = make_env();
    let contract = payout_contract(&env, i128::MAX, 0, 0);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::PartialRefund);
    assert_eq!(result, Err(Error::PotentialOverflow));
}

/// The largest safe available amount for PartialRefund: `i128::MAX / 30`
/// does NOT overflow.
#[test]
fn partial_refund_max_safe_amount_succeeds() {
    let env = make_env();
    let safe_max = i128::MAX / 30; // multiplication is safe at this value
    let contract = payout_contract(&env, safe_max, 0, 0);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::PartialRefund);
    assert!(result.is_ok(), "expected Ok for safe_max, got {:?}", result);
    let (client, freelancer) = result.unwrap();
    // freelancer = floor(safe_max * 30 / 100)
    let expected_freelancer = (safe_max * 30) / 100;
    assert_eq!(freelancer, expected_freelancer);
    assert_eq!(client + freelancer, safe_max, "conservation violated");
}

/// PartialRefund with one stroop past the safe max. `(safe_max + 1) * 30 > i128::MAX`.
#[test]
fn partial_refund_one_past_safe_max_overflows() {
    let env = make_env();
    let overflow_amount = (i128::MAX / 30) + 1;
    let contract = payout_contract(&env, overflow_amount, 0, 0);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::PartialRefund);
    assert_eq!(result, Err(Error::PotentialOverflow));
}

/// PartialRefund floor rounding: very small amounts ensure floor division
/// does not create value.
#[test]
fn partial_refund_small_amounts_floor_rounding() {
    let env = make_env();
    // 1 stroop: freelancer = floor(1 * 30 / 100) = 0; client = 1
    let contract = payout_contract(&env, 1, 0, 0);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::PartialRefund);
    assert!(result.is_ok(), "expected Ok for 1 stroop, got {:?}", result);
    let (client, freelancer) = result.unwrap();
    assert_eq!(freelancer, 0);
    assert_eq!(client, 1);
    assert_eq!(client + freelancer, 1);
}

/// PartialRefund with 99 stroops: floor(99*30/100) = 29; client = 70.
#[test]
fn partial_refund_99_stroops_rounding() {
    let env = make_env();
    let contract = payout_contract(&env, 99, 0, 0);
    let result = crate::resolution_payouts(&contract, &DisputeResolution::PartialRefund);
    assert!(result.is_ok());
    let (client, freelancer) = result.unwrap();
    assert_eq!(freelancer, 29);
    assert_eq!(client, 70);
    assert_eq!(client + freelancer, 99);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. resolution_payouts — Split at extreme values
// ═══════════════════════════════════════════════════════════════════════════════

/// Split at `i128::MAX` available: (MAX, 0) succeeds (sum == available).
#[test]
fn split_i128_max_zero_succeeds() {
    let env = make_env();
    let contract = payout_contract(&env, i128::MAX, 0, 0);
    let split = DisputeSplit {
        client_amount: i128::MAX,
        freelancer_amount: 0,
    };
    let result = crate::resolution_payouts(&contract, &DisputeResolution::Split(split));
    assert!(result.is_ok());
    let (client, freelancer) = result.unwrap();
    assert_eq!(client, i128::MAX);
    assert_eq!(freelancer, 0);
}

/// Split at `i128::MAX` available: (0, MAX) succeeds.
#[test]
fn split_zero_i128_max_succeeds() {
    let env = make_env();
    let contract = payout_contract(&env, i128::MAX, 0, 0);
    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: i128::MAX,
    };
    let result = crate::resolution_payouts(&contract, &DisputeResolution::Split(split));
    assert!(result.is_ok());
    let (client, freelancer) = result.unwrap();
    assert_eq!(client, 0);
    assert_eq!(freelancer, i128::MAX);
}

/// Split with components that individually overflow when added together
/// is rejected with PotentialOverflow.
#[test]
fn split_overflowing_sum_rejected() {
    let env = make_env();
    let contract = payout_contract(&env, i128::MAX, 0, 0);
    let split = DisputeSplit {
        client_amount: i128::MAX - 1,
        freelancer_amount: 2, // (MAX-1) + 2 = MAX+1 → overflow
    };
    let result = crate::resolution_payouts(&contract, &DisputeResolution::Split(split));
    assert_eq!(result, Err(Error::PotentialOverflow));
}

/// Split with client_amount > available is rejected.
#[test]
fn split_client_exceeds_available_rejected() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 101,
        freelancer_amount: 0,
    };
    let result = crate::resolution_payouts(&contract, &DisputeResolution::Split(split));
    assert_eq!(result, Err(Error::InvalidDisputeSplit));
}

/// Split with freelancer_amount > available is rejected.
#[test]
fn split_freelancer_exceeds_available_rejected() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: 101,
    };
    let result = crate::resolution_payouts(&contract, &DisputeResolution::Split(split));
    assert_eq!(result, Err(Error::InvalidDisputeSplit));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Conservation invariant at extreme values
// ═══════════════════════════════════════════════════════════════════════════════

/// Table-driven conservation test for PartialRefund across a range of
/// values from tiny to `i128::MAX / 30`.
#[test]
fn partial_refund_conservation_invariant() {
    let env = make_env();
    let test_values: &[i128] = &[
        1, 2, 3, 5, 7, 10, 33, 99, 100, 101, 1_000,
        1_000_000, 100_000_000, MAX_SINGLE_AMOUNT_STROOPS,
        i128::MAX / 30, // max safe
    ];

    for &available in test_values {
        let contract = payout_contract(&env, available, 0, 0);
        let result = crate::resolution_payouts(&contract, &DisputeResolution::PartialRefund);
        assert!(
            result.is_ok(),
            "PartialRefund failed at available={}", available
        );
        let (client, freelancer) = result.unwrap();
        assert_eq!(
            client + freelancer,
            available,
            "conservation violated at available={}: client={} + freelancer={} != {}",
            available, client, freelancer, available
        );
        let expected_freelancer = (available * 30) / 100;
        assert_eq!(
            freelancer, expected_freelancer,
            "floor rounding mismatch at available={}", available
        );
    }
}

/// FullRefund/FullPayout conservation: sum always equals available, both
/// at zero and at MAX.
#[test]
fn full_refund_payout_conservation_at_extremes() {
    let env = make_env();

    for &available in &[0, 1, i128::MAX / 30, i128::MAX] {
        let contract = payout_contract(&env, available, 0, 0);

        // FullRefund
        let info = crate::resolution_payouts(&contract, &DisputeResolution::FullRefund).unwrap();
        assert_eq!(info.client_payout + info.freelancer_payout, available);
        assert_eq!(info.client_payout, available);
        assert_eq!(info.freelancer_payout, 0);

        // FullPayout
        let info = crate::resolution_payouts(&contract, &DisputeResolution::FullPayout).unwrap();
        assert_eq!(info.client_payout + info.freelancer_payout, available);
        assert_eq!(info.client_payout, 0);
        assert_eq!(info.freelancer_payout, available);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. safe_add_amounts / safe_subtract_amounts edge cases
// ═══════════════════════════════════════════════════════════════════════════════

/// safe_add_amounts at extremes.
#[test]
fn safe_add_i128_extremes() {
    // Normal values
    assert_eq!(safe_add_amounts(100, 200), Some(300));
    assert_eq!(safe_add_amounts(0, 0), Some(0));
    assert_eq!(safe_add_amounts(0, 1), Some(1));
    assert_eq!(safe_add_amounts(-1, 1), Some(0));

    // i128::MAX boundary
    assert_eq!(safe_add_amounts(i128::MAX, 0), Some(i128::MAX));
    assert_eq!(safe_add_amounts(i128::MAX, 1), None); // overflow
    assert_eq!(safe_add_amounts(i128::MAX, i128::MAX), None); // overflow
    assert_eq!(safe_add_amounts(i128::MAX - 1, 1), Some(i128::MAX));
    assert_eq!(safe_add_amounts(i128::MAX - 1, 2), None); // overflow

    // i128::MIN boundary
    assert_eq!(safe_add_amounts(i128::MIN, 0), Some(i128::MIN));
    assert_eq!(safe_add_amounts(i128::MIN, -1), None); // underflow
    assert_eq!(safe_add_amounts(i128::MIN, i128::MIN), None); // underflow
}

/// safe_subtract_amounts at extremes.
#[test]
fn safe_sub_i128_extremes() {
    // Normal values
    assert_eq!(safe_subtract_amounts(300, 100), Some(200));
    assert_eq!(safe_subtract_amounts(0, 0), Some(0));
    assert_eq!(safe_subtract_amounts(0, 1), Some(-1));

    // i128::MAX boundary
    assert_eq!(safe_subtract_amounts(i128::MAX, 0), Some(i128::MAX));
    assert_eq!(safe_subtract_amounts(i128::MAX, i128::MAX), Some(0));
    assert_eq!(safe_subtract_amounts(i128::MAX, 1), Some(i128::MAX - 1));

    // i128::MIN boundary
    assert_eq!(safe_subtract_amounts(i128::MIN, 0), Some(i128::MIN));
    assert_eq!(safe_subtract_amounts(i128::MIN, 1), None); // underflow
    assert_eq!(safe_subtract_amounts(i128::MIN, i128::MIN), Some(0));

    // Large subtraction near zero
    assert_eq!(safe_subtract_amounts(0, 1), Some(-1));
    assert_eq!(safe_subtract_amounts(1, 1), Some(0));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. validate_single_amount at boundary values
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn validate_single_amount_extremes() {
    // Minimum valid
    assert!(validate_single_amount(1).is_ok());

    // Maximum valid (MAX_SINGLE_AMOUNT_STROOPS)
    assert!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS).is_ok());

    // Zero: rejected as non-positive
    assert_eq!(
        validate_single_amount(0),
        Err(EscrowError::AmountMustBePositive)
    );

    // Negative values: rejected
    assert_eq!(
        validate_single_amount(-1),
        Err(EscrowError::AmountMustBePositive)
    );
    assert_eq!(
        validate_single_amount(i128::MIN),
        Err(EscrowError::AmountMustBePositive)
    );

    // One above max: rejected
    assert_eq!(
        validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS + 1),
        Err(EscrowError::InvalidMilestoneAmount)
    );

    // i128::MAX: rejected (exceeds max single amount)
    assert_eq!(
        validate_single_amount(i128::MAX),
        Err(EscrowError::InvalidMilestoneAmount)
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Integration: resolve_dispute rejects corrupted accounting state
// ═══════════════════════════════════════════════════════════════════════════════
//
// Note: The `resolution_payouts` function (called first inside
// `resolve_dispute`) computes `available = funded - released - refunded`
// via `checked_sub`. When `refunded_amount` (or `released_amount`) exceeds
// `funded_amount`, this underflows → `AccountingInvariantViolated`.
//
// The `checked_add` accumulation fix (`refunded_amount += client_payout`)
// is defense-in-depth: the available-balance invariant guarantees
// `refunded + client_payout ≤ funded ≤ i128::MAX`, so overflow is
// mathematically impossible through the normal entrypoint flow. The
// `checked_add` ensures safety even if the invariant were ever violated.

/// Dispute resolution on a contract where `refunded_amount > funded_amount`
/// (corrupted state) is rejected with `AccountingInvariantViolated`.
#[test]
fn resolve_dispute_catches_corrupted_refunded_amount() {
    let env = make_env();
    let arbiter = Address::generate(&env);

    let fixture = EscrowFixture::builder()
        .with_participants(
            Address::generate(&env),
            Address::generate(&env),
            Some(arbiter.clone()),
        )
        .with_settlement_token()
        .build();

    // Fund the contract.
    let sac = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, sac).mint(&fixture.client, &1_000_i128);
    fixture.escrow().deposit_funds(&fixture.escrow_id, &fixture.client, &1_000_i128);

    // Corrupt state: refunded > funded → available underflows
    overwrite_contract(&fixture, |c| {
        c.status = ContractStatus::Disputed;
        c.refunded_amount = i128::MAX;
    });

    assert_contract_error(
        fixture.escrow().try_resolve_dispute(
            &fixture.escrow_id,
            &arbiter,
            &DisputeResolution::FullRefund,
        ),
        Error::AccountingInvariantViolated,
    );
}

/// Dispute resolution on a contract where `released_amount > funded_amount`
/// (corrupted state) is rejected with `AccountingInvariantViolated`.
#[test]
fn resolve_dispute_catches_corrupted_released_amount() {
    let env = make_env();
    let arbiter = Address::generate(&env);

    let fixture = EscrowFixture::builder()
        .with_participants(
            Address::generate(&env),
            Address::generate(&env),
            Some(arbiter.clone()),
        )
        .with_settlement_token()
        .build();

    let sac = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, sac).mint(&fixture.client, &1_000_i128);
    fixture.escrow().deposit_funds(&fixture.escrow_id, &fixture.client, &1_000_i128);

    overwrite_contract(&fixture, |c| {
        c.status = ContractStatus::Disputed;
        c.released_amount = i128::MAX;
    });

    assert_contract_error(
        fixture.escrow().try_resolve_dispute(
            &fixture.escrow_id,
            &arbiter,
            &DisputeResolution::FullPayout,
        ),
        Error::AccountingInvariantViolated,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Integration: refund catches corrupted accounting state
// ═══════════════════════════════════════════════════════════════════════════════
//
/// Note: `check_sufficient_balance` (in `refund_impl.rs`) uses `checked_sub`
/// to compute `available = funded - released - refunded`. When `refunded_amount`
/// exceeds `funded_amount`, the subtraction underflows → `PotentialOverflow`.
///
/// The `checked_add` fix on `refunded_amount += total_refund` is
/// defense-in-depth: the balance check guarantees
/// `refunded + total_refund ≤ funded ≤ i128::MAX`.

/// Refunding on a contract where `refunded_amount > funded_amount` is
/// caught by `check_sufficient_balance`'s `checked_sub`.
#[test]
#[should_panic(expected = "HostError: Error(Contract, #28)")]
fn refund_catches_corrupted_state() {
    let fixture = EscrowFixture::builder()
        .with_settlement_token()
        .build();

    let sac = fixture.settlement_token.as_ref().unwrap();
    let total = 300_i128;
    StellarAssetClient::new(&fixture.env, sac).mint(&fixture.client, &total);
    fixture.escrow().deposit_funds(&fixture.escrow_id, &fixture.client, &total);

    // Corrupt state: refunded > funded → checked_sub underflows
    overwrite_contract(&fixture, |c| {
        c.refunded_amount = i128::MAX;
    });

    let indices = vec![&fixture.env, 0_u32];
    // check_sufficient_balance panics with PotentialOverflow (error #28)
    fixture.escrow().refund_unreleased_milestones(&fixture.escrow_id, &indices);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. resolution_payouts — available balance boundary conditions
// ═══════════════════════════════════════════════════════════════════════════════

/// When released + refunded > funded, available is negative and
/// AccountingInvariantViolated is returned.
#[test]
fn available_negative_rejected() {
    let env = make_env();
    // released(600) + refunded(500) = 1100 > funded(1000) → corrupted
    let contract = payout_contract(&env, 1000, 600, 500);

    let result = crate::resolution_payouts(&contract, &DisputeResolution::FullRefund);
    assert_eq!(result, Err(Error::AccountingInvariantViolated));

    let result = crate::resolution_payouts(&contract, &DisputeResolution::FullPayout);
    assert_eq!(result, Err(Error::AccountingInvariantViolated));

    let result = crate::resolution_payouts(&contract, &DisputeResolution::PartialRefund);
    assert_eq!(result, Err(Error::AccountingInvariantViolated));
}

/// When released + refunded exactly equals funded, available is zero and
/// all resolutions succeed with zero payouts.
#[test]
fn available_exactly_zero_succeeds_all_variants() {
    let env = make_env();
    let contract = payout_contract(&env, 500, 200, 300);

    let full_refund =
        crate::resolution_payouts(&contract, &DisputeResolution::FullRefund).unwrap();
    assert_eq!(full_refund.client_payout, 0);
    assert_eq!(full_refund.freelancer_payout, 0);

    let full_payout =
        crate::resolution_payouts(&contract, &DisputeResolution::FullPayout).unwrap();
    assert_eq!(full_payout.client_payout, 0);
    assert_eq!(full_payout.freelancer_payout, 0);

    let partial =
        crate::resolution_payouts(&contract, &DisputeResolution::PartialRefund).unwrap();
    assert_eq!(partial.client_payout, 0);
    assert_eq!(partial.freelancer_payout, 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Accumulate amounts and final_status at extreme values
// ═══════════════════════════════════════════════════════════════════════════════

/// Accumulating one valid large amount succeeds.
#[test]
fn accumulate_single_large_amount() {
    let result = crate::accumulate_amounts([MAX_SINGLE_AMOUNT_STROOPS]);
    assert_eq!(result, Ok(MAX_SINGLE_AMOUNT_STROOPS));
}

/// Accumulating two valid amounts that sum within bounds succeeds.
#[test]
fn accumulate_two_valid_amounts() {
    let half = MAX_SINGLE_AMOUNT_STROOPS / 2;
    let result = crate::accumulate_amounts([half, half]);
    assert_eq!(result, Ok(MAX_SINGLE_AMOUNT_STROOPS));
}

/// Accumulating a zero amount is rejected by validate_single_amount.
#[test]
fn accumulate_zero_rejected() {
    let result = crate::accumulate_amounts([100_i128, 0_i128]);
    assert_eq!(result, Err(EscrowError::AmountMustBePositive));
}

/// Accumulating a negative amount is rejected.
#[test]
fn accumulate_negative_rejected() {
    let result = crate::accumulate_amounts([100_i128, -1_i128]);
    assert_eq!(result, Err(EscrowError::AmountMustBePositive));
}

/// Accumulating empty yields zero.
#[test]
fn accumulate_empty_returns_zero() {
    let result = crate::accumulate_amounts::<[i128; 0]>([]);
    assert_eq!(result, Ok(0));
}

/// `final_status_after_resolution` edge cases.
#[test]
fn final_status_refunded_only_when_fully_refunded() {
    let env = make_env();

    // Fully refunded → Refunded
    let contract = payout_contract(&env, 100, 0, 100);
    assert_eq!(
        crate::final_status_after_resolution(&contract),
        ContractStatus::Refunded
    );

    // Partially refunded → Completed
    let contract = payout_contract(&env, 100, 20, 30);
    assert_eq!(
        crate::final_status_after_resolution(&contract),
        ContractStatus::Completed
    );

    // Nothing refunded → Completed
    let contract = payout_contract(&env, 100, 80, 0);
    assert_eq!(
        crate::final_status_after_resolution(&contract),
        ContractStatus::Completed
    );

    // Zero-funded, zero-refunded edge case
    let contract = payout_contract(&env, 0, 0, 0);
    assert_eq!(
        crate::final_status_after_resolution(&contract),
        ContractStatus::Refunded
    );
}
