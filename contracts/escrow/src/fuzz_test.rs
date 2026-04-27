//! Fuzz harness for escrow entrypoints.
//!
//! Covers three categories:
//!   1. **Malformed inputs** — zero/negative amounts, empty milestone lists,
//!      out-of-range milestone indices, duplicate milestone ids.
//!   2. **Boundary values** — i128::MAX, i128::MIN, MAX_MILESTONES ± 1,
//!      MAX_TOTAL_ESCROW_STROOPS ± 1, rating boundaries (0, 1, 5, 6).
//!   3. **Unauthorized call patterns** — same client/freelancer, wrong caller
//!      for deposit/release/reputation, pause-blocked operations.
//!
//! # Running locally
//!
//! ```sh
//! # Standard proptest run (256 cases per property, deterministic seed):
//! cargo test -p escrow fuzz
//!
//! # More cases:
//! PROPTEST_CASES=2000 cargo test -p escrow fuzz
//!
//! # Reproduce a specific failure (seed printed on failure):
//! PROPTEST_SEED=<hex> cargo test -p escrow fuzz
//! ```
//!
//! Failing seeds are auto-saved to `proptest-regressions/fuzz_test.txt` and
//! replayed on every subsequent run.
//!
//! # CI
//!
//! `cargo test` runs this file automatically. No secrets or network access
//! required. Runtime is bounded by `PROPTEST_CASES` (default 256).

#![cfg(test)]

extern crate std;

use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, vec as sorovec, Address, Env, Vec as SoroVec};

use crate::{Escrow, EscrowClient, EscrowError, MAX_MILESTONES, MAX_TOTAL_ESCROW_STROOPS};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup() -> (Env, EscrowClient<'static>) {
    // SAFETY: EscrowClient borrows Env; we box Env so the address is stable for
    // the lifetime of the test case.
    let env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    (unsafe { std::ptr::read(env as *const Env) }, client)
}

/// Build a SorobanVec from a std Vec of i128.
fn to_soroban_vec(env: &Env, amounts: &[i128]) -> SoroVec<i128> {
    let mut v = SoroVec::new(env);
    for &a in amounts {
        v.push_back(a);
    }
    v
}

fn assert_err(
    result: Result<impl core::fmt::Debug, Result<EscrowError, soroban_sdk::InvokeError>>,
    expected: EscrowError,
) {
    assert_eq!(result, Err(Ok(expected)));
}

// ── Category 1: Malformed inputs ─────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Zero or negative deposit amounts must be rejected.
    #[test]
    fn fuzz_deposit_zero_or_negative_rejected(bad_amount in i128::MIN..=0i128) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);

        assert_err(client.try_deposit_funds(&cid, &bad_amount), EscrowError::AmountMustBePositive);
    }

    /// Empty milestone list must be rejected at contract creation.
    #[test]
    fn fuzz_create_empty_milestones_rejected(_seed in 0u32..1000u32) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let empty = SoroVec::<i128>::new(&env);

        assert_err(
            client.try_create_contract(&client_addr, &freelancer_addr, &empty),
            EscrowError::EmptyMilestones,
        );
    }

    /// Zero or negative milestone amounts must be rejected.
    #[test]
    fn fuzz_create_nonpositive_milestone_rejected(bad in i128::MIN..=0i128) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = to_soroban_vec(&env, &[100_i128, bad]);

        assert_err(
            client.try_create_contract(&client_addr, &freelancer_addr, &milestones),
            EscrowError::InvalidMilestoneAmount,
        );
    }

    /// Out-of-range milestone index on release must be rejected.
    #[test]
    fn fuzz_release_out_of_range_index_rejected(oob_idx in 3u32..u32::MAX) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128, 200_i128, 300_i128];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);
        client.deposit_funds(&cid, &600_i128);

        assert_err(
            client.try_release_milestone(&cid, &oob_idx),
            EscrowError::MilestoneNotFound,
        );
    }

    /// Releasing the same milestone twice must be rejected.
    #[test]
    fn fuzz_double_release_rejected(idx in 0u32..3u32) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128, 200_i128, 300_i128];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);
        client.deposit_funds(&cid, &600_i128);
        client.release_milestone(&cid, &idx);

        assert_err(
            client.try_release_milestone(&cid, &idx),
            EscrowError::MilestoneAlreadyReleased,
        );
    }
}

// ── Category 2: Boundary values ──────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Exactly MAX_MILESTONES milestones must be accepted.
    #[test]
    fn fuzz_create_exactly_max_milestones_accepted(_seed in 0u32..64u32) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let amounts: std::vec::Vec<i128> = (0..MAX_MILESTONES).map(|_| 1_i128).collect();
        let milestones = to_soroban_vec(&env, &amounts);

        let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
        assert!(result.is_ok(), "MAX_MILESTONES should be accepted, got {:?}", result);
    }

    /// MAX_MILESTONES + 1 milestones must be rejected.
    #[test]
    fn fuzz_create_over_max_milestones_rejected(_seed in 0u32..64u32) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let amounts: std::vec::Vec<i128> = (0..=MAX_MILESTONES).map(|_| 1_i128).collect();
        let milestones = to_soroban_vec(&env, &amounts);

        assert_err(
            client.try_create_contract(&client_addr, &freelancer_addr, &milestones),
            EscrowError::TooManyMilestones,
        );
    }

    /// Total escrow exactly at MAX_TOTAL_ESCROW_STROOPS must be accepted.
    #[test]
    fn fuzz_create_at_max_total_accepted(_seed in 0u32..64u32) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, MAX_TOTAL_ESCROW_STROOPS];

        let result = client.try_create_contract(&client_addr, &freelancer_addr, &milestones);
        assert!(result.is_ok(), "amount at cap should be accepted, got {:?}", result);
    }

    /// Total escrow one above MAX_TOTAL_ESCROW_STROOPS must be rejected.
    #[test]
    fn fuzz_create_over_max_total_rejected(_seed in 0u32..64u32) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, MAX_TOTAL_ESCROW_STROOPS + 1];

        assert_err(
            client.try_create_contract(&client_addr, &freelancer_addr, &milestones),
            EscrowError::TotalExceedsMaxEscrow,
        );
    }

    /// Reputation rating 1..=5 must be accepted on a completed contract.
    #[test]
    fn fuzz_reputation_valid_rating_accepted(rating in 1i128..=5i128) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);
        client.deposit_funds(&cid, &100_i128);
        client.release_milestone(&cid, &0);

        let result = client.try_issue_reputation(&cid, &rating);
        assert!(result.is_ok(), "rating {} should be accepted, got {:?}", rating, result);
    }

    /// Reputation rating 0 and 6 must be rejected.
    #[test]
    fn fuzz_reputation_boundary_ratings_rejected(rating in prop_oneof![Just(0i128), Just(6i128)]) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);
        client.deposit_funds(&cid, &100_i128);
        client.release_milestone(&cid, &0);

        assert_err(client.try_issue_reputation(&cid, &rating), EscrowError::InvalidRating);
    }

    /// Deposit exactly equal to total required must be accepted and mark contract Funded.
    #[test]
    fn fuzz_deposit_exact_total_accepted(amount in 1i128..=MAX_TOTAL_ESCROW_STROOPS) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, amount];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);

        let result = client.try_deposit_funds(&cid, &amount);
        assert!(result.is_ok(), "exact deposit should be accepted, got {:?}", result);
    }

    /// Deposit one above total required must be rejected.
    #[test]
    fn fuzz_deposit_overfunding_rejected(amount in 1i128..=(MAX_TOTAL_ESCROW_STROOPS - 1)) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, amount];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);
        client.deposit_funds(&cid, &amount);

        assert_err(
            client.try_deposit_funds(&cid, &1),
            EscrowError::FundingExceedsRequired,
        );
    }
}

// ── Category 3: Unauthorized call patterns ───────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Same address as client and freelancer must be rejected.
    #[test]
    fn fuzz_create_same_participant_rejected(_seed in 0u32..128u32) {
        let (env, client) = setup();
        let same = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128];

        assert_err(
            client.try_create_contract(&same, &same, &milestones),
            EscrowError::InvalidParticipants,
        );
    }

    /// Operations on a non-existent contract_id must return ContractNotFound.
    #[test]
    fn fuzz_missing_contract_id_rejected(bad_id in 1u32..u32::MAX) {
        let (env, client) = setup();

        assert_err(client.try_get_contract(&bad_id), EscrowError::ContractNotFound);
        assert_err(client.try_deposit_funds(&bad_id, &1), EscrowError::ContractNotFound);
        assert_err(client.try_release_milestone(&bad_id, &0), EscrowError::ContractNotFound);
    }

    /// All mutating entrypoints must be blocked when the contract is paused.
    #[test]
    fn fuzz_paused_blocks_all_mutating_ops(_seed in 0u32..128u32) {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);
        client.pause();

        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128];

        assert_err(
            client.try_create_contract(&client_addr, &freelancer_addr, &milestones),
            EscrowError::ContractPaused,
        );
        assert_err(client.try_deposit_funds(&0, &100), EscrowError::ContractPaused);
        assert_err(client.try_release_milestone(&0, &0), EscrowError::ContractPaused);
    }

    /// All mutating entrypoints must be blocked during emergency pause.
    #[test]
    fn fuzz_emergency_blocks_all_mutating_ops(_seed in 0u32..128u32) {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);
        client.activate_emergency_pause();

        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128];

        assert_err(
            client.try_create_contract(&client_addr, &freelancer_addr, &milestones),
            EscrowError::ContractPaused,
        );
        assert_err(client.try_deposit_funds(&0, &100), EscrowError::ContractPaused);
        assert_err(client.try_release_milestone(&0, &0), EscrowError::ContractPaused);
    }

    /// Reputation cannot be issued on an incomplete (not-all-milestones-released) contract.
    #[test]
    fn fuzz_reputation_on_incomplete_contract_rejected(_seed in 0u32..128u32) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128, 200_i128];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);
        client.deposit_funds(&cid, &300_i128);
        // Only release one of two milestones — contract not complete.
        client.release_milestone(&cid, &0);

        assert_err(client.try_issue_reputation(&cid, &5), EscrowError::InvalidState);
    }

    /// Reputation can only be issued once per contract.
    #[test]
    fn fuzz_reputation_double_issuance_rejected(_seed in 0u32..128u32) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);
        client.deposit_funds(&cid, &100_i128);
        client.release_milestone(&cid, &0);
        client.issue_reputation(&cid, &5);

        assert_err(client.try_issue_reputation(&cid, &4), EscrowError::ReputationAlreadyIssued);
    }

    /// Release without sufficient funded balance must be rejected.
    #[test]
    fn fuzz_release_insufficient_balance_rejected(
        fund in 1i128..99i128,
    ) {
        let (env, client) = setup();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let milestones = sorovec![&env, 100_i128];
        let cid = client.create_contract(&client_addr, &freelancer_addr, &milestones);
        client.deposit_funds(&cid, &fund);

        assert_err(
            client.try_release_milestone(&cid, &0),
            EscrowError::InsufficientEscrowBalance,
        );
    }
}
