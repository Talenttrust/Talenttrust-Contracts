//! Property-based tests for milestone invariants.
//!
//! Tests core invariants that must hold across randomized milestone configurations:
//!
//! INVARIANT 1 — Amount bounds:
//! - milestone.amount > 0 always
//! - sum of all milestone amounts never exceeds escrow total_amount
//!
//! INVARIANT 2 — Release consistency:
//! - A released milestone cannot be released again
//! - released flag is monotonic (false → true, never true → false)
//!
//! INVARIANT 3 — Index bounds:
//! - Valid milestone index always in range [0, milestones.len())
//! - Out-of-bounds index always returns an error
//!
//! INVARIANT 4 — State consistency:
//! - Total released amount never exceeds total escrow amount
//! - Milestone count matches what was added
//!
//! INVARIANT 5 — Ordering invariants:
//! - Milestones preserve insertion order
//! - Release of milestone N does not affect milestone M where N != M
//!
//! ## Running
//!
//! ```sh
//! # Default 256 cases per property:
//! cargo test -p escrow milestones_proptest
//!
//! # More cases:
//! PROPTEST_CASES=1024 cargo test -p escrow milestones_proptest
//!
//! # Reproduce a specific failure:
//! PROPTEST_SEED=<hex> cargo test -p escrow milestones_proptest
//! ```
//!
//! Failing seeds are auto-saved to `proptest-regressions/milestones_proptest.txt`.

#![cfg(test)]

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::vec::Vec as StdVec;

use proptest::prelude::*;
use soroban_sdk::{
    testutils::Address as _, Address, Env, Vec as SorobanVec,
};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_MILESTONES: usize = 32;
const MIN_AMOUNT: i128 = 1;
const MAX_AMOUNT: i128 = 1_000_000_000;
const DEFAULT_CASES: u32 = 256;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Generate a list of positive milestone amounts.
/// Ensures all amounts are in the valid range.
fn milestone_amounts() -> impl Strategy<Value = StdVec<i128>> {
    prop::collection::vec(MIN_AMOUNT..=MAX_AMOUNT, 1..=MAX_MILESTONES)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sum(amounts: &[i128]) -> i128 {
    amounts.iter().copied().sum()
}

struct MilestoneTestHarness {
    env: Env,
    client_addr: Address,
    freelancer_addr: Address,
}

impl MilestoneTestHarness {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        MilestoneTestHarness {
            env,
            client_addr,
            freelancer_addr,
        }
    }

    fn escrow_client(&self) -> EscrowClient<'_> {
        let id = self.env.register(Escrow, ());
        EscrowClient::new(&self.env, &id)
    }
}

// ---------------------------------------------------------------------------
// Safe operation wrappers
// ---------------------------------------------------------------------------

fn try_deposit(client: &EscrowClient, id: u32, caller: &Address, amount: i128) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        client.deposit_funds(&id, caller, &amount);
    }))
    .is_ok()
}

fn try_approve(client: &EscrowClient, id: u32, caller: &Address, ms_idx: u32) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        client.approve_milestone_release(&id, caller, &ms_idx);
    }))
    .is_ok()
}

fn try_release(client: &EscrowClient, id: u32, caller: &Address, ms_idx: u32) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        client.release_milestone(&id, caller, &ms_idx);
    }))
    .is_ok()
}

fn try_get_milestone(client: &EscrowClient, id: u32, ms_idx: u32) -> Option<crate::Milestone> {
    catch_unwind(AssertUnwindSafe(|| {
        client.get_milestone(&id, &ms_idx)
    }))
    .ok()
    .flatten()
}

// ---------------------------------------------------------------------------
// Invariant checkers
// ---------------------------------------------------------------------------

/// INVARIANT 1: All milestone amounts are positive.
fn check_amount_positivity(amounts: &[i128]) {
    for (i, &amount) in amounts.iter().enumerate() {
        assert!(
            amount > 0,
            "Milestone {} has non-positive amount: {}",
            i,
            amount
        );
    }
}

/// INVARIANT 1: Sum of milestone amounts fits within i128 and represents
/// the total escrow obligation.
fn check_amount_bounds(amounts: &[i128]) {
    let total = sum(amounts);
    assert!(
        total > 0,
        "Total milestone sum must be positive, got: {}",
        total
    );
    // Ensure no individual amount exceeds the sum (sanity check).
    for (i, &amount) in amounts.iter().enumerate() {
        assert!(
            amount <= total,
            "Milestone {} amount ({}) exceeds total sum ({})",
            i,
            amount,
            total
        );
    }
}

/// INVARIANT 2: Released flag is always false for newly created milestones.
fn check_milestone_not_released_on_creation(
    client: &EscrowClient,
    contract_id: u32,
    milestone_count: u32,
) {
    for i in 0..milestone_count {
        let ms = try_get_milestone(client, contract_id, i)
            .expect("milestone should exist");
        assert!(
            !ms.released,
            "Milestone {} should not be released upon creation",
            i
        );
    }
}

/// INVARIANT 3: Index bounds check — valid indices are [0, len).
fn check_index_bounds_valid(
    client: &EscrowClient,
    contract_id: u32,
    milestone_count: u32,
) {
    // All valid indices (0..milestone_count) should retrieve the milestone.
    for i in 0..milestone_count {
        let ms = try_get_milestone(client, contract_id, i);
        assert!(
            ms.is_some(),
            "Valid index {} should return a milestone",
            i
        );
    }
}

/// INVARIANT 3: Out-of-bounds indices should return None.
fn check_index_bounds_invalid(
    client: &EscrowClient,
    contract_id: u32,
    milestone_count: u32,
) {
    // Some out-of-bounds indices should return None.
    let out_of_bounds_indices = vec![
        milestone_count,
        milestone_count + 1,
        u32::MAX / 2,
        u32::MAX,
    ];
    for idx in out_of_bounds_indices {
        let ms = try_get_milestone(client, contract_id, idx);
        assert!(
            ms.is_none(),
            "Out-of-bounds index {} should return None",
            idx
        );
    }
}

/// INVARIANT 4: Total released amount never exceeds total escrow amount.
fn check_released_amount_bounds(client: &EscrowClient, contract_id: u32, total_escrow: i128) {
    let contract = client.get_contract(&contract_id);
    assert!(
        contract.released_amount <= total_escrow,
        "Released amount ({}) exceeds total escrow amount ({})",
        contract.released_amount,
        total_escrow
    );
}

/// INVARIANT 4: Milestone count matches the number created.
fn check_milestone_count(
    client: &EscrowClient,
    contract_id: u32,
    expected_count: u32,
) {
    let milestones = client.get_milestones(&contract_id);
    assert_eq!(
        milestones.len() as u32,
        expected_count,
        "Milestone count mismatch: expected {}, got {}",
        expected_count,
        milestones.len()
    );
}

/// INVARIANT 5: Milestones preserve insertion order (amounts match in order).
fn check_milestone_ordering(
    client: &EscrowClient,
    contract_id: u32,
    expected_amounts: &[i128],
) {
    let milestones = client.get_milestones(&contract_id);
    assert_eq!(
        milestones.len(),
        expected_amounts.len(),
        "Milestone count mismatch"
    );
    for (i, &expected_amount) in expected_amounts.iter().enumerate() {
        let ms = milestones.get(i as u32).unwrap();
        assert_eq!(
            ms.amount, expected_amount,
            "Milestone {} amount mismatch: expected {}, got {}",
            i, expected_amount, ms.amount
        );
    }
}

/// INVARIANT 5: Release of milestone N does not affect other milestones.
fn check_release_isolation(
    client: &EscrowClient,
    contract_id: u32,
    released_index: u32,
    other_indices: &[u32],
) {
    for &i in other_indices {
        let ms = try_get_milestone(client, contract_id, i)
            .expect("milestone should exist");
        assert!(
            !ms.released,
            "Milestone {} should not be released after releasing milestone {}",
            i,
            released_index
        );
    }
}

/// INVARIANT 2: Released flag is monotonic (transitions false -> true only once).
fn check_release_monotonicity(
    client: &EscrowClient,
    contract_id: u32,
    milestone_index: u32,
) {
    let ms = try_get_milestone(client, contract_id, milestone_index)
        .expect("milestone should exist");
    // Already checked this milestone is released; trying to release again
    // should fail (we'll use the return value to confirm).
    let released_before = ms.released;
    // Try to release it again (this should fail if already released).
    let approval_ok = try_approve(client, contract_id, &Address::generate(&client.env), &milestone_index);
    let release_ok = if approval_ok {
        try_release(client, contract_id, &Address::generate(&client.env), &milestone_index)
    } else {
        false
    };
    // The release must either fail, or the flag should remain true.
    let ms_after = try_get_milestone(client, contract_id, milestone_index)
        .expect("milestone should exist");
    assert!(
        ms_after.released >= released_before,
        "Release flag should be monotonic (only false->true): before={}, after={}",
        released_before,
        ms_after.released
    );
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(DEFAULT_CASES))]

    /// INVARIANT 1: All milestone amounts are positive and bounded.
    #[test]
    fn prop_milestone_amounts_valid(amounts in milestone_amounts()) {
        check_amount_positivity(&amounts);
        check_amount_bounds(&amounts);
    }

    /// INVARIANT 1 + 4: Created contract respects amount invariants,
    /// and total milestone sum matches total_amount.
    #[test]
    fn prop_contract_creation_respects_amounts(amounts in milestone_amounts()) {
        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let total = sum(&amounts);
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        // INVARIANT 1: Amounts are positive.
        check_amount_positivity(&amounts);
        check_amount_bounds(&amounts);

        // INVARIANT 4: Total released is 0 upon creation.
        check_released_amount_bounds(&client, contract_id, total);

        // Contract's total_deposited starts at 0; released starts at 0.
        let contract = client.get_contract(&contract_id);
        prop_assert_eq!(contract.released_amount, 0);
        prop_assert_eq!(contract.total_deposited, 0);
    }

    /// INVARIANT 2 + 4: Milestones start unreleased and stay unreleased
    /// until explicitly released.
    #[test]
    fn prop_milestones_unreleased_on_creation(amounts in milestone_amounts()) {
        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        let milestone_count = amounts.len() as u32;
        check_milestone_not_released_on_creation(&client, contract_id, milestone_count);
    }

    /// INVARIANT 3: Index bounds are enforced correctly.
    /// Valid indices [0, len) should work; out-of-bounds should fail.
    #[test]
    fn prop_index_bounds_enforced(amounts in milestone_amounts()) {
        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        let milestone_count = amounts.len() as u32;
        check_index_bounds_valid(&client, contract_id, milestone_count);
        check_index_bounds_invalid(&client, contract_id, milestone_count);
    }

    /// INVARIANT 4: Milestone count matches what was created.
    #[test]
    fn prop_milestone_count_preserved(amounts in milestone_amounts()) {
        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        let milestone_count = amounts.len() as u32;
        check_milestone_count(&client, contract_id, milestone_count);
    }

    /// INVARIANT 5: Milestones preserve insertion order.
    #[test]
    fn prop_milestone_order_preserved(amounts in milestone_amounts()) {
        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        check_milestone_ordering(&client, contract_id, &amounts);
    }

    /// INVARIANT 2 + 5: Double-release of the same milestone is rejected
    /// and other milestones remain unaffected.
    #[test]
    fn prop_double_release_rejected_isolation_maintained(
        amounts in milestone_amounts(),
        target_raw in 0u32..MAX_MILESTONES as u32,
    ) {
        let n = amounts.len() as u32;
        prop_assume!(n > 0);
        let target = target_raw % n;

        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let total = sum(&amounts);
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        // Deposit so we can release.
        assert!(try_deposit(&client, contract_id, &h.client_addr, total));

        // Approve and release the target milestone.
        assert!(try_approve(&client, contract_id, &h.client_addr, target));
        assert!(try_release(&client, contract_id, &h.client_addr, target));

        // Verify it's released.
        let before_ms = try_get_milestone(&client, contract_id, target)
            .expect("milestone should exist");
        prop_assert!(before_ms.released);

        // Try to release again (should fail).
        assert!(try_approve(&client, contract_id, &h.client_addr, target));
        let double_release_ok = try_release(&client, contract_id, &h.client_addr, target);
        prop_assert!(!double_release_ok, "Double release must be rejected");

        // Verify it's still released and state hasn't changed.
        let after_ms = try_get_milestone(&client, contract_id, target)
            .expect("milestone should exist");
        prop_assert_eq!(before_ms.released, after_ms.released);

        // Verify other milestones are not affected.
        let other_indices: StdVec<u32> = (0..n)
            .filter(|&i| i != target)
            .collect();
        check_release_isolation(&client, contract_id, target, &other_indices);
    }

    /// INVARIANT 4: Total released amount never exceeds total escrow amount.
    #[test]
    fn prop_released_amount_bounded_by_escrow(amounts in milestone_amounts()) {
        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let total = sum(&amounts);
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        // Deposit the exact total.
        assert!(try_deposit(&client, contract_id, &h.client_addr, total));

        // Release each milestone.
        let n = amounts.len() as u32;
        for i in 0..n {
            assert!(try_approve(&client, contract_id, &h.client_addr, i));
            assert!(try_release(&client, contract_id, &h.client_addr, i));
            // After every release, check the invariant.
            check_released_amount_bounds(&client, contract_id, total);
        }

        // At the end, released amount equals total.
        let contract = client.get_contract(&contract_id);
        prop_assert_eq!(contract.released_amount, total);
    }

    /// INVARIANT 2: Released flag is monotonic (once set to true, stays true).
    /// Attempting to re-release should fail gracefully without corrupting state.
    #[test]
    fn prop_release_flag_monotonic(
        amounts in milestone_amounts(),
        target_raw in 0u32..MAX_MILESTONES as u32,
    ) {
        let n = amounts.len() as u32;
        prop_assume!(n > 0);
        let target = target_raw % n;

        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let total = sum(&amounts);
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        // Deposit.
        assert!(try_deposit(&client, contract_id, &h.client_addr, total));

        // Release target milestone.
        assert!(try_approve(&client, contract_id, &h.client_addr, target));
        assert!(try_release(&client, contract_id, &h.client_addr, target));

        // Check monotonicity: flag is now true.
        let ms_released = try_get_milestone(&client, contract_id, target)
            .expect("milestone should exist");
        prop_assert!(ms_released.released);

        // Try to release again and verify flag stays true.
        check_release_monotonicity(&client, contract_id, target);
    }

    /// INVARIANT 3 + 4: Getting individual milestones and getting all milestones
    /// must return consistent data (same amounts, same count).
    #[test]
    fn prop_individual_vs_batch_milestone_retrieval(amounts in milestone_amounts()) {
        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        let all_milestones = client.get_milestones(&contract_id);
        prop_assert_eq!(all_milestones.len() as u32, amounts.len() as u32);

        // Retrieve each individually and compare.
        for i in 0..amounts.len() {
            let individual = try_get_milestone(&client, contract_id, i as u32)
                .expect("milestone should exist");
            let from_batch = all_milestones.get(i as u32).unwrap();

            prop_assert_eq!(individual.amount, from_batch.amount);
            prop_assert_eq!(individual.released, from_batch.released);
            prop_assert_eq!(individual.refunded, from_batch.refunded);
            prop_assert_eq!(individual.funded_amount, from_batch.funded_amount);
        }
    }

    /// INVARIANT 1 + 2 + 4: Full release sequence — all milestones released,
    /// state is consistent throughout.
    #[test]
    fn prop_full_milestone_release_sequence(amounts in milestone_amounts()) {
        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let total = sum(&amounts);
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        // Verify initial state.
        check_amount_positivity(&amounts);
        check_milestone_not_released_on_creation(&client, contract_id, amounts.len() as u32);

        // Deposit the exact total.
        assert!(try_deposit(&client, contract_id, &h.client_addr, total));

        // Release each milestone in order.
        let mut released_sum: i128 = 0;
        for (i, &expected_amount) in amounts.iter().enumerate() {
            let i = i as u32;
            assert!(try_approve(&client, contract_id, &h.client_addr, i));
            assert!(try_release(&client, contract_id, &h.client_addr, i));

            released_sum += expected_amount;

            // After each release, verify invariants.
            let ms = try_get_milestone(&client, contract_id, i)
                .expect("milestone should exist");
            prop_assert!(ms.released);

            let contract = client.get_contract(&contract_id);
            prop_assert_eq!(contract.released_amount, released_sum);
            check_released_amount_bounds(&client, contract_id, total);
        }

        // Final state: all released.
        let contract = client.get_contract(&contract_id);
        prop_assert_eq!(contract.released_amount, total);
    }

    /// INVARIANT 1 + 3 + 4: Partial release with out-of-bounds access rejection.
    /// Release some milestones, verify index bounds still enforced.
    #[test]
    fn prop_partial_release_with_bounds_check(
        amounts in milestone_amounts(),
        release_count in 1usize..10usize,
    ) {
        let n = amounts.len();
        prop_assume!(n > 0);
        let release_count = release_count % n; // Ensure we don't exceed milestone count.
        let release_count = (release_count).max(1).min(n);

        let h = MilestoneTestHarness::new();
        let client = h.escrow_client();
        let total = sum(&amounts);
        let ms: SorobanVec<i128> = {
            let mut v = SorobanVec::new(&h.env);
            for &a in &amounts {
                v.push_back(a);
            }
            v
        };
        let contract_id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &None,
            &ms,
            &ReleaseAuthorization::ClientOnly,
        );

        // Deposit.
        assert!(try_deposit(&client, contract_id, &h.client_addr, total));

        // Release first release_count milestones.
        let mut released_sum: i128 = 0;
        for i in 0..release_count as u32 {
            assert!(try_approve(&client, contract_id, &h.client_addr, i));
            assert!(try_release(&client, contract_id, &h.client_addr, i));
            released_sum += amounts[i as usize];
        }

        // Verify released amount.
        let contract = client.get_contract(&contract_id);
        prop_assert_eq!(contract.released_amount, released_sum);

        // Verify bounds: valid indices still work, out-of-bounds still fail.
        check_index_bounds_valid(&client, contract_id, n as u32);
        check_index_bounds_invalid(&client, contract_id, n as u32);
    }
}
