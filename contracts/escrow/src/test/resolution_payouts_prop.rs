//! Property tests for multi-release sequences with protocol fees.
//!
//! Verifies, for arbitrary milestone amounts and fee rates, that after every
//! release the accounting invariant holds:
//!
//!   released_net + accumulated_fees + refunded <= funded_amount
//!
//! Equivalently: the sum of net payouts and accrued fees never exceeds deposits.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SdkVec};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

// ── Deterministic property-style tests ───────────────────────────────────────
//
// The Soroban test environment does not support `proptest` or `quickcheck`
// (no std threading, no arbitrary trait impls for SDK types). We instead use
// a parametrized helper run over a representative set of (amounts, fee_bps)
// pairs that collectively cover edge cases: zero fee, tiny amounts, large
// amounts, non-divisible amounts, and maximum basis points.

/// Run the full multi-release sequence for the given milestone amounts and
/// fee rate, asserting the invariant at every step.
fn run_multi_release(amounts: &[i128], fee_bps: u32) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    if fee_bps > 0 {
        client.set_protocol_fee_bps(&fee_bps);
    }

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let mut sdk_milestones: SdkVec<i128> = SdkVec::new(&env);
    for &a in amounts {
        sdk_milestones.push_back(a);
    }

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &sdk_milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let total: i128 = amounts.iter().sum();
    client.deposit_funds(&id, &client_addr, &total);

    let mut expected_gross_released = 0i128;

    for (idx, &gross) in amounts.iter().enumerate() {
        client.approve_milestone_release(&id, &client_addr, &(idx as u32));
        client.release_milestone(&id, &client_addr, &(idx as u32));

        expected_gross_released += gross;

        let s = client.get_contract_summary(&id);
        let fees = client.get_accumulated_protocol_fees();

        // Invariant 1: released_net + fees == gross released so far
        assert_eq!(
            s.released_amount + fees,
            expected_gross_released,
            "[fee_bps={fee_bps}] net+fees != gross at milestone {idx}: \
             released={}, fees={fees}, expected_gross={expected_gross_released}",
            s.released_amount,
        );

        // Invariant 2: gross released so far must not exceed funded_amount
        assert!(
            s.released_amount + fees <= s.funded_amount,
            "[fee_bps={fee_bps}] released+fees ({}) exceeds funded ({}) at milestone {idx}",
            s.released_amount + fees,
            s.funded_amount,
        );
    }

    // Final: released_net + fees == funded_amount (no refunds in this flow)
    let s = client.get_contract_summary(&id);
    let fees = client.get_accumulated_protocol_fees();
    assert_eq!(
        s.released_amount + fees,
        s.funded_amount,
        "[fee_bps={fee_bps}] final: released_net+fees != funded"
    );
}

// ── Zero-fee sequences ────────────────────────────────────────────────────────

#[test]
fn prop_zero_fee_single_milestone() {
    run_multi_release(&[1_000], 0);
}

#[test]
fn prop_zero_fee_two_milestones() {
    run_multi_release(&[500, 1_500], 0);
}

#[test]
fn prop_zero_fee_three_milestones() {
    run_multi_release(&[200, 400, 600], 0);
}

// ── Non-zero-fee sequences ────────────────────────────────────────────────────

#[test]
fn prop_100bps_three_milestones() {
    // 1% fee
    run_multi_release(&[1_000, 2_000, 3_000], 100);
}

#[test]
fn prop_500bps_three_milestones() {
    // 5% fee
    run_multi_release(&[1_000, 2_000, 3_000], 500);
}

#[test]
fn prop_1000bps_three_milestones() {
    // 10% fee
    run_multi_release(&[1_000, 2_500, 3_333], 1000);
}

#[test]
fn prop_1000bps_four_milestones() {
    run_multi_release(&[1_000, 2_500, 3_333, 500], 1000);
}

// ── Tiny amounts (floor division → fee = 0) ───────────────────────────────────

#[test]
fn prop_1000bps_tiny_milestones_fee_rounds_to_zero() {
    // 9 * 1000 / 10000 = 0 — fee rounds to zero for tiny amounts
    // Both milestones produce zero fee; net == gross.
    run_multi_release(&[9, 9], 1000);
}

#[test]
fn prop_1000bps_boundary_milestone() {
    // 10 * 1000 / 10000 = 1 — just above the zero-fee threshold
    run_multi_release(&[10, 100, 1_000], 1000);
}

// ── Maximum fee rate ──────────────────────────────────────────────────────────

#[test]
fn prop_max_fee_bps_single_milestone() {
    // 10000 bps = 100%: all funds become fees, freelancer gets 0
    run_multi_release(&[1_000], 10_000);
}

#[test]
fn prop_max_fee_bps_two_milestones() {
    run_multi_release(&[1_000, 2_000], 10_000);
}

// ── Large amounts ─────────────────────────────────────────────────────────────

#[test]
fn prop_1000bps_large_milestones() {
    // Use amounts that fit within i128 when multiplied by fee_bps (1000)
    // Max safe: i128::MAX / 1000 ≈ 1.7e35 — well above realistic escrow sizes
    run_multi_release(&[1_000_000_000, 2_000_000_000, 500_000_000], 1000);
}

// ── Non-divisible amounts (floor division discards remainder) ─────────────────

#[test]
fn prop_1000bps_nondivisible_amounts() {
    // 333 * 1000 / 10000 = 33 (not 33.3)
    // 3333 * 1000 / 10000 = 333 (not 333.3)
    run_multi_release(&[333, 3_333, 10_001], 1000);
}

#[test]
fn prop_333bps_nondivisible() {
    // 1000 * 333 / 10000 = 33 (remainder discarded)
    run_multi_release(&[1_000, 3_000, 7_000], 333);
}
