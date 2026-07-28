//! Property-based tests for the disputes module.
//!
//! Randomized, deterministic coverage of every dispute invariant under
//! bounded random inputs. The module splits into two layers:
//!
//! 1. **Pure-arithmetic invariants** — `resolution_payouts`,
//!    `final_status_after_resolution` and the [`DisputeResolution`] enum are
//!    exercised across all (`funded`, `released`, `refunded`) triples within
//!    safe `i128` bounds, without spinning up a Soroban test environment.
//!
//! 2. **End-to-end integration invariants** — the live
//!    [`EscrowClient`] is driven through raise → resolve cycles for every
//!    variant of [`DisputeResolution`], asserting conservation of the
//!    `released + refunded` accounting invariant and final-status correctness.
//!
//! ## Invariants under test
//!
//! - **Conservation** — `client_payout + freelancer_payout == available`.
//! - **Non-negativity** — both payout legs are non-negative for any accepted
//!   [`DisputeResolution`].
//! - **PartialRefund flooring** — `freelancer_payout = floor(available * 30 / 100)`
//!   for every non-negative `available`.
//! - **Split exactness** — a [`DisputeResolution::Split`] is accepted iff
//!   `client_amount + freelancer_amount == available`, both legs non-negative,
//!   neither leg exceeds `available`, and the sum does not overflow `i128`.
//!   All failure modes are rejected with the appropriate typed error.
//! - **Corrupted accounting is fail-closed** — any pair where
//!   `released + refunded > funded` is rejected with
//!   `AccountingInvariantViolated` for every [`DisputeResolution`] variant.
//! - **Final-status correctness** — `final_status_after_resolution` returns
//!   `Refunded` iff `refunded == funded`, otherwise `Completed`; the function
//!   never panics regardless of `i128` inputs.
//! - **Discriminator uniqueness** — [`DisputeResolution::code`] returns a
//!   stable, distinct `u32` per variant.
//! - **End-to-end conservation** — resolving a dispute through the live
//!   contract conserves `released + refunded == funded` and lands the contract
//!   in the [`ContractStatus`] dictated by `final_status_after_resolution`.
//!
//! ## Running
//!
//! ```sh
//! # Default 256 cases per property:
//! cargo test -p escrow dispute_proptest
//!
//! # More cases:
//! PROPTEST_CASES=1024 cargo test -p escrow dispute_proptest
//!
//! # Reproduce a specific failure (seed is auto-printed on failure):
//! PROPTEST_SEED=<hex> cargo test -p escrow dispute_proptest
//! ```
//!
//! Failing seeds are auto-saved to `proptest-regressions/dispute_proptest.txt`.

#![cfg(test)]

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};

use proptest::prelude::*;
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, vec, Address, Env, Vec as SdkVec,
};

use crate::{
    Contract, ContractStatus, DisputeResolution, DisputeSplit, Error, Escrow, EscrowClient,
    ReleaseAuthorization,
};

// Reuse the existing dispute-test helper rather than reimplementing a
// `Contract` builder — sibling tests at `test/dispute.rs::payout_contract`
// already do exactly this.
use super::dispute::payout_contract;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default number of proptest cases per property. Override with the
/// `PROPTEST_CASES` environment variable at run time.
const DEFAULT_CASES: u32 = 256;

/// Upper bound used for the pure-arithmetic i128 properties. We need this to
/// be small enough that `available.checked_mul(30).and_then(|v| v.checked_div(100))`
/// in `resolution_payouts` does not overflow on the largest randomly-generated
/// inputs — `MAX_LARGE * 30 < i128::MAX` keeps the product inside `i128`.
const MAX_LARGE: i128 = i128::MAX / 100;

// ---------------------------------------------------------------------------
// Pure-arithmetic properties — `resolution_payouts`
// ---------------------------------------------------------------------------

const PURE_CASES: u32 = DEFAULT_CASES;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PURE_CASES))]

    /// Conservation invariant for [`DisputeResolution::FullRefund`].
    ///
    /// For any non-negative `available`, FullRefund routes the entire
    /// `available` to the client (`freelancer_payout == 0`).
    #[test]
    fn prop_full_refund_conserves_available(funded in 0i128..=MAX_LARGE) {
        let env = Env::default();
        let contract = payout_contract(&env, funded, 0, 0);
        let (client, freelancer) = crate::dispute::resolution_payouts(
            &contract,
            &DisputeResolution::FullRefund,
        )
        .expect("FullRefund never errors for funded-only state");
        prop_assert_eq!(client, funded);
        prop_assert_eq!(freelancer, 0);
        prop_assert_eq!(client + freelancer, funded);
    }

    /// Conservation invariant for [`DisputeResolution::FullPayout`].
    ///
    /// For any non-negative `available`, FullPayout routes the entire
    /// `available` to the freelancer (`client_payout == 0`).
    #[test]
    fn prop_full_payout_conserves_available(funded in 0i128..=MAX_LARGE) {
        let env = Env::default();
        let contract = payout_contract(&env, funded, 0, 0);
        let (client, freelancer) = crate::dispute::resolution_payouts(
            &contract,
            &DisputeResolution::FullPayout,
        )
        .expect("FullPayout never errors for funded-only state");
        prop_assert_eq!(client, 0);
        prop_assert_eq!(freelancer, funded);
        prop_assert_eq!(client + freelancer, funded);
    }

    /// PartialRefund flooring invariant.
    ///
    /// For every `available >= 0`, the freelancer leg is
    /// `floor(available * 30 / 100)` and the client leg is the remainder so
    /// that `client + freelancer == available`. Both legs are non-negative.
    #[test]
    fn prop_partial_refund_floor_30pct(funded in 0i128..=MAX_LARGE) {
        let env = Env::default();
        let contract = payout_contract(&env, funded, 0, 0);
        let (client, freelancer) = crate::dispute::resolution_payouts(
            &contract,
            &DisputeResolution::PartialRefund,
        )
        .expect("PartialRefund never errors for funded-only state");
        let expected_freelancer = funded.saturating_mul(30) / 100;
        prop_assert_eq!(freelancer, expected_freelancer);
        prop_assert_eq!(client, funded - expected_freelancer);
        prop_assert_eq!(client + freelancer, funded);
        // Non-negativity.
        prop_assert!(client >= 0);
        prop_assert!(freelancer >= 0);
    }

    /// Conservation invariant across arbitrary `(funded, released, refunded)`
    /// triples that produce a non-negative `available` balance.
    ///
    /// For every [`DisputeResolution`] variant, the resulting payout pair
    /// must (a) be non-negative, (b) sum exactly to `available`, and
    /// (c) leave `funded_amount` untouched.
    #[test]
    fn prop_arbitrary_three_legs_conserve_under_all_variants(
        funded in 0i128..=MAX_LARGE,
        released_raw in 0i128..=MAX_LARGE,
        refunded_raw in 0i128..=MAX_LARGE,
        variant in 0u32..4,
    ) {
        // Clamp so `released + refunded <= funded`. The pre-clamp `..=MAX_LARGE`
        // bounds give proptest a generous reduce/shrink surface.
        let released = released_raw.min(funded);
        let refunded = refunded_raw.min(funded - released);
        let available = funded - released - refunded;

        let env = Env::default();
        let contract = payout_contract(&env, funded, released, refunded);
        let resolution = match variant {
            0 => DisputeResolution::FullRefund,
            1 => DisputeResolution::PartialRefund,
            2 => DisputeResolution::FullPayout,
            _ => {
                // Half-and-half Split — exact conservation.
                let split_client = available / 2;
                let split_freelancer = available - split_client;
                DisputeResolution::Split(DisputeSplit {
                    client_amount: split_client,
                    freelancer_amount: split_freelancer,
                })
            }
        };

        let (client_amt, freelancer_amt) = crate::dispute::resolution_payouts(&contract, &resolution)
            .expect("valid state + valid resolution must not error");
        prop_assert!(client_amt >= 0);
        prop_assert!(freelancer_amt >= 0);
        prop_assert_eq!(client_amt + freelancer_amt, available);
        // Funded amount must be untouched by the pure arithmetic helper.
        prop_assert_eq!(contract.funded_amount, funded);
    }

    /// Corrupted accounting state must fail closed for every variant.
    ///
    /// Any `(funded, released, refunded)` where `released + refunded > funded`
    /// produces a negative `available` and must be rejected with
    /// [`Error::AccountingInvariantViolated`].
    #[test]
    fn prop_corrupted_accounting_rejected_everywhere(
        funded in 1i128..=MAX_LARGE,
        released_extra in 1i128..=MAX_LARGE,
        refunded_in in 0i128..=MAX_LARGE,
        variant in 0u32..3,
    ) {
        // Force `released + refunded > funded`.
        let released = funded.saturating_sub(1).saturating_add(released_extra);
        let refunded = refunded_in.min(released.saturating_sub(1));
        prop_assume!(released + refunded > funded);

        let env = Env::default();
        let contract = payout_contract(&env, funded, released, refunded);
        let resolution = match variant {
            0 => DisputeResolution::FullRefund,
            1 => DisputeResolution::PartialRefund,
            _ => DisputeResolution::FullPayout,
        };
        let result = crate::dispute::resolution_payouts(&contract, &resolution);
        prop_assert_eq!(
            result.err(),
            Some(Error::AccountingInvariantViolated),
            "corrupted accounting must be rejected (funded={}, released={}, refunded={})",
            funded, released, refunded
        );

        // Split variant on the same corrupted state must also fail with the
        // same error — checked-sub happens before the Split match.
        let split = DisputeSplit {
            client_amount: 0,
            freelancer_amount: 0,
        };
        prop_assert_eq!(
            crate::dispute::resolution_payouts(&contract, &DisputeResolution::Split(split)).err(),
            Some(Error::AccountingInvariantViolated),
        );
    }

    /// Valid Split: any `(client_amount, freelancer_amount)` non-negative pair
    /// summing exactly to `available` must be accepted and returned as the
    /// payout legs. The strategy picks a `client_amount` in
    /// `0..=available` and computes `freelancer_amount = available - client_amount`,
    /// guaranteeing sum equality and absence of overflow.
    ///
    /// Uses `prop_flat_map` so the inner range's upper bound can reference
    /// the outer parameter's value — proptest 1.4.0's `proptest!` macro
    /// parses dependent tuple strategies but can mis-evaluate `RangeInclusive`
    /// value types at strategy-construction time without this lift.
    #[test]
    fn prop_split_accepts_exact_conservation(
        (funded, client_amount) in (0i128..=MAX_LARGE)
            .prop_flat_map(|funded| (Just(funded), 0i128..=funded)),
    ) {
        let env = Env::default();
        let contract = payout_contract(&env, funded, 0, 0);
        let freelancer_amount = funded - client_amount;
        let split = DisputeSplit {
            client_amount,
            freelancer_amount,
        };
        let (a, b) = crate::dispute::resolution_payouts(&contract, &DisputeResolution::Split(split))
            .expect("exact split must succeed");
        prop_assert_eq!(a, client_amount);
        prop_assert_eq!(b, freelancer_amount);
        prop_assert_eq!(a + b, funded);
        prop_assert!(a >= 0);
        prop_assert!(b >= 0);
    }

    /// Invalid Split `(client_amount, freelancer_amount)` rejection matrix.
    ///
    /// Every member of {negative leg, leg exceeding available, sum != available,
    /// individually-conserved-but-jointly-exceeding-available} is rejected with
    /// [`Error::InvalidDisputeSplit`] or [`Error::PotentialOverflow`] as
    /// appropriate.
    ///
    /// Uses `prop_flat_map` for the dependent ranges — see
    /// `prop_split_accepts_exact_conservation` for rationale.
    #[test]
    fn prop_split_rejects_invalid_inputs(
        (funded, client_in, freelancer_in) in (1i128..=MAX_LARGE).prop_flat_map(|funded| {
            let upper = funded.saturating_add(10);
            (Just(funded), -2i128..=upper, -2i128..=upper)
        }),
    ) {
        let env = Env::default();
        let contract = payout_contract(&env, funded, 0, 0);
        let split = DisputeSplit {
            client_amount: client_in,
            freelancer_amount: freelancer_in,
        };
        let result = crate::dispute::resolution_payouts(&contract, &DisputeResolution::Split(split));
        let sum = client_in.checked_add(freelancer_in);

        let is_neg = client_in < 0 || freelancer_in < 0;
        let either_over = client_in > funded || freelancer_in > funded;
        // Both legs are bounded above by `funded + 10`, which is well within
        // `i128::MAX` for the chosen `funded` strategy — `sum` can never
        // overflow, so the `PotentialOverflow` branch is unreachable here.
        prop_assume!(sum.is_some());
        let sum_matches = sum == Some(funded);

        // The only happy path is: no negative leg and the sum exactly equals
        // funded. All other paths must reject with InvalidDisputeSplit.
        if !is_neg && sum_matches {
            prop_assert!(
                result.is_ok(),
                "exact-conserving split must be accepted (c={}, f={}, funded={})",
                client_in, freelancer_in, funded,
            );
        } else if is_neg {
            prop_assert_eq!(
                result.err(),
                Some(Error::InvalidDisputeSplit),
                "negative leg must be InvalidDisputeSplit (c={}, f={})",
                client_in, freelancer_in,
            );
        } else {
            // either_over and !sum_matches collapse here: a non-negative leg
            // exceeding `funded` cannot sum to `funded`, and a non-overflowing
            // sum not equalling `funded` is rejected by the issue #572 fix
            // and the sum-equality guard respectively.
            prop_assert_eq!(
                result.err(),
                Some(Error::InvalidDisputeSplit),
                "non-conserving split must be InvalidDisputeSplit (c={}, f={}, funded={}, sum={:?})",
                client_in, freelancer_in, funded, sum,
            );
        }
    }
}

/// Overflow guard for Split — `i128::MAX + 1` must surface as
/// `PotentialOverflow`, never panic.
#[test]
fn split_overflow_surfaces_potential_overflow() {
    let env = Env::default();
    let contract = payout_contract(&env, i128::MAX, 0, 0);
    let split = DisputeSplit {
        client_amount: i128::MAX,
        freelancer_amount: 1,
    };
    assert_eq!(
        crate::dispute::resolution_payouts(&contract, &DisputeResolution::Split(split)).err(),
        Some(Error::PotentialOverflow),
    );
}

// ---------------------------------------------------------------------------
// Pure-arithmetic properties — `final_status_after_resolution`
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PURE_CASES))]

    /// `final_status_after_resolution` returns [`ContractStatus::Refunded`]
    /// iff `refunded_amount == funded_amount`, regardless of `released_amount`.
    /// In every other case it returns [`ContractStatus::Completed`].
    #[test]
    fn prop_final_status_refunded_iff_fully_refunded(
        funded_raw in 0i128..=MAX_LARGE,
        released_raw in 0i128..=MAX_LARGE,
        refunded_raw in 0i128..=MAX_LARGE,
    ) {
        let funded = funded_raw;
        let released = released_raw.min(funded);
        let refunded = refunded_raw.min(funded);
        let env = Env::default();
        let contract = payout_contract(&env, funded, released, refunded);
        let status =
            crate::dispute::final_status_after_resolution(&contract);
        if refunded == funded {
            prop_assert_eq!(status, ContractStatus::Refunded);
        } else {
            prop_assert_eq!(status, ContractStatus::Completed);
        }
    }

    /// `final_status_after_resolution` is total over arbitrary (possibly
    /// corrupted) accounting — it never panics and only emits one of the
    /// two terminal absorption states.
    #[test]
    fn prop_final_status_total_no_panic(
        funded in 0i128..=MAX_LARGE,
        released in 0i128..=MAX_LARGE,
        refunded in 0i128..=MAX_LARGE,
    ) {
        let env = Env::default();
        let contract = payout_contract(&env, funded, released, refunded);
        let status =
            crate::dispute::final_status_after_resolution(&contract);
        prop_assert!(
            status == ContractStatus::Refunded || status == ContractStatus::Completed,
            "final_status must be Refunded or Completed, got {:?}",
            status,
        );
    }
}

// ---------------------------------------------------------------------------
// Discriminator uniqueness — `DisputeResolution::code`
// ---------------------------------------------------------------------------

/// `DisputeResolution::code()` returns a stable distinct `u32` per variant.
#[test]
fn dispute_resolution_code_uniqueness() {
    let full_refund = DisputeResolution::FullRefund.code();
    let partial_refund = DisputeResolution::PartialRefund.code();
    let full_payout = DisputeResolution::FullPayout.code();
    let split = DisputeResolution::Split(DisputeSplit {
        client_amount: 0,
        freelancer_amount: 0,
    })
    .code();
    let mut codes: std::vec::Vec<u32> = std::vec![full_refund, partial_refund, full_payout, split];
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), 4, "codes must be unique: {:?}", codes);
}

// ---------------------------------------------------------------------------
// End-to-end integration properties via the live Soroban contract
// ---------------------------------------------------------------------------
//
// Mirrors the pattern from `resolution_payouts_prop.rs` — drive the
// entrypoints through `raise_dispute` → `resolve_dispute` for every variant
// and assert conservation + final-status correctness.

/// Run the full dispute flow on a freshly-minted contract and return the
/// resulting state. Asserts conservation (`released + refunded == funded`)
/// before returning so failing runs surface a clear diagnostic.
///
/// Wrapped in `catch_unwind` because Soroban test-env panics (auth failures,
/// settled-state assertions) are otherwise opaque to proptest's failure
/// reporting.
fn run(end_amounts: &[i128], resolution: &DisputeResolution) -> Contract {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);

    let mut milestones: SdkVec<i128> = vec![&env];
    for &a in end_amounts {
        milestones.push_back(a);
    }

    let total: i128 = end_amounts.iter().sum();
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    StellarAssetClient::new(&env, &token).mint(&client_addr, &total);
    client.deposit_funds(&contract_id, &client_addr, &total);
    client.raise_dispute(&contract_id, &client_addr);
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed,
    );

    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, resolution));
    let contract = client.get_contract(&contract_id);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount,
        "conservation violated: released={} refunded={} funded={}",
        contract.released_amount,
        contract.refunded_amount,
        contract.funded_amount,
    );
    contract
}

/// Conservation + final-status invariant for FullRefund.
#[test]
fn fullrefund_integration_mark_refunded_and_conserves_for_random_totals() {
    let totals: &[i128] = &[10, 100, 1_000, 1_000_000];
    for &total in totals {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let contract = run(&[total], &DisputeResolution::FullRefund);
            assert_eq!(contract.status, ContractStatus::Refunded);
            assert_eq!(contract.refunded_amount, total);
            assert_eq!(contract.released_amount, 0);
        }));
        assert!(result.is_ok(), "FullRefund integration panicked for total={total}");
    }
}

/// Conservation + final-status invariant for FullPayout.
#[test]
fn fullpayout_integration_mark_completed_and_conserves_for_random_totals() {
    let totals: &[i128] = &[10, 100, 1_000, 1_000_000];
    for &total in totals {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let contract = run(&[total], &DisputeResolution::FullPayout);
            assert_eq!(contract.status, ContractStatus::Completed);
            assert_eq!(contract.released_amount, total);
            assert_eq!(contract.refunded_amount, 0);
        }));
        assert!(result.is_ok(), "FullPayout integration panicked for total={total}");
    }
}

/// Conservation + final-status invariant for PartialRefund.
///
/// Contract lands in `Completed` (partial refund is not a full refund)
/// and the released/refunded legs always equal funded.
#[test]
fn partialrefund_integration_mark_completed_and_conserves_for_random_totals() {
    let totals: &[i128] = &[10, 33, 100, 333, 1_000, 999_999];
    for &total in totals {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let contract = run(&[total], &DisputeResolution::PartialRefund);
            assert_eq!(contract.status, ContractStatus::Completed);
            let expected_freelancer = total.saturating_mul(30) / 100;
            let expected_client = total - expected_freelancer;
            assert_eq!(contract.released_amount, expected_freelancer);
            assert_eq!(contract.refunded_amount, expected_client);
        }));
        assert!(result.is_ok(), "PartialRefund integration panicked for total={total}");
    }
}

/// Conservation + final-status invariant for Split.
///
/// Generates a representative `(client_amount, freelancer_amount)` pair
/// summing exactly to `funded` and asserts the contract lands in
/// `Completed` with the right released/refunded accounting.
#[test]
fn split_integration_conserves_for_random_legs() {
    let cases: &[(i128, i128)] = &[
        (0, 100),
        (1, 99),
        (33, 67),
        (50, 50),
        (75, 25),
        (100, 0),
    ];
    for &(client_amt, freelancer_amt) in cases {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let contract = run(
                &[client_amt + freelancer_amt],
                &DisputeResolution::Split(DisputeSplit {
                    client_amount: client_amt,
                    freelancer_amount: freelancer_amt,
                }),
            );
            assert_eq!(contract.status, ContractStatus::Completed);
            assert_eq!(contract.released_amount, freelancer_amt);
            assert_eq!(contract.refunded_amount, client_amt);
        }));
        assert!(result.is_ok(), "Split integration panicked for c={client_amt} f={freelancer_amt}");
    }
}
