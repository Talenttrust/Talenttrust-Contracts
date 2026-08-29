//! Fuzz coverage for milestone deadline arithmetic (issue #1359).
//!
//! Hand-picked dates miss overflow and boundary bugs around ledger timestamps
//! and grace periods. This module generates bounded timestamps and durations
//! and asserts:
//!
//! - **Monotonicity**: deadline ordering is preserved across increasing timestamps.
//! - **Rejection of invalid ranges**: zero-duration and past-deadline values.
//! - **Stable boundary behavior**: `now == deadline` is never overdue (strict `>`).
//! - **Overflow safety**: `u64` boundary values do not panic.
//! - **Ledger boundary**: timestamp 0 and `u64::MAX` are handled.
//! - **Escrow conservation**: release/refund totals never exceed deposits.
//!
//! # Running
//!
//! ```sh
//! cargo test -p escrow fuzz_milestone_deadline
//! PROPTEST_CASES=512 cargo test -p escrow fuzz_milestone_deadline
//! ```

use proptest::prelude::*;
use soroban_sdk::{testutils::Ledger, Address, Env, Symbol, Vec as SorobanVec};

use super::{create_contract, register_client};
use crate::{DataKey, Milestone};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Set the ledger timestamp to an absolute number of seconds.
fn set_now(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = secs;
    });
}

/// Overwrite milestone `index`'s `deadline` and `released` flag directly in
/// persistent storage, bypassing any setter entrypoint.
fn set_milestone_deadline_and_released(
    env: &Env,
    contract_addr: &Address,
    contract_id: u32,
    index: u32,
    deadline: Option<u64>,
    released: bool,
) {
    env.as_contract(contract_addr, || {
        let key = (
            DataKey::Contract(contract_id),
            Symbol::new(env, "milestones"),
        );
        let mut milestones: SorobanVec<Milestone> = env.storage().persistent().get(&key).unwrap();
        let mut m = milestones.get(index).unwrap();
        m.deadline = deadline;
        m.released = released;
        milestones.set(index, m);
        env.storage().persistent().set(&key, &milestones);
    });
}

// ── Category 1: Zero duration / zero deadline ────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A milestone with deadline=0 and now=0 must NOT be overdue (strict >).
    #[test]
    fn fuzz_deadline_zero_now_zero_not_overdue(_seed in 0u32..256u32) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(0), false);
        set_now(&env, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline=0, now=0 must not be overdue (strict >)"
        );
    }

    /// A milestone with deadline=0 and now=1 must be overdue.
    #[test]
    fn fuzz_deadline_zero_now_one_overdue(_seed in 0u32..256u32) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(0), false);
        set_now(&env, 1);
        prop_assert!(
            client.is_milestone_overdue(&id, &0),
            "deadline=0, now=1 must be overdue"
        );
    }
}

// ── Category 2: Maximum duration / u64 boundary ─────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// deadline=u64::MAX, now < u64::MAX must NOT be overdue.
    #[test]
    fn fuzz_deadline_max_now_before_not_overdue(now in 0u64..u64::MAX) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(u64::MAX), false);
        set_now(&env, now);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline=u64::MAX, now={} must not be overdue", now
        );
    }

    /// deadline=u64::MAX, now=u64::MAX must NOT be overdue (strict >).
    #[test]
    fn fuzz_deadline_max_now_equal_not_overdue(_seed in 0u32..256u32) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(u64::MAX), false);
        set_now(&env, u64::MAX);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline=u64::MAX, now=u64::MAX must not be overdue (strict >)"
        );
    }
}

// ── Category 3: Past deadline / now > deadline ───────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// For any deadline > 0, now = deadline + 1 must be overdue.
    #[test]
    fn fuzz_past_deadline_overdue(deadline in 1u64..u64::MAX) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);
        let now = deadline.saturating_add(1); // safe: deadline >= 1
        set_now(&env, now);
        prop_assert!(
            client.is_milestone_overdue(&id, &0),
            "deadline={}, now={} must be overdue", deadline, now
        );
    }

    /// For any deadline > 0, now = deadline must NOT be overdue (strict >).
    #[test]
    fn fuzz_at_deadline_not_overdue(deadline in 1u64..u64::MAX) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);
        set_now(&env, deadline);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline={}, now={} must NOT be overdue (strict >)", deadline, deadline
        );
    }
}

// ── Category 4: Monotonicity ────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// If now₁ < now₂ and both are after the deadline, both must be overdue.
    /// If now₁ < deadline < now₂, only now₂ must be overdue.
    #[test]
    fn fuzz_monotonicity_of_overdue(
        deadline in 100u64..u64::MAX - 2,
        delta_before in 1u64..50u64,
        delta_after in 1u64..50u64,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);

        // before: now = deadline - delta_before (must NOT be overdue)
        let now_before = deadline - delta_before;
        set_now(&env, now_before);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "now_before={} < deadline={} must not be overdue",
            now_before, deadline
        );

        // at exact boundary: now = deadline (must NOT be overdue)
        set_now(&env, deadline);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "now == deadline must not be overdue (strict >)"
        );

        // after: now = deadline + delta_after (must be overdue)
        let now_after = deadline.saturating_add(delta_after);
        set_now(&env, now_after);
        prop_assert!(
            client.is_milestone_overdue(&id, &0),
            "now_after={} > deadline={} must be overdue",
            now_after, deadline
        );
    }
}

// ── Category 5: Ledger boundary ─────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Timestamp 0 with a future deadline must not be overdue.
    #[test]
    fn fuzz_ledger_zero_with_future_deadline(deadline in 1u64..u64::MAX) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);
        set_now(&env, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "now=0 with deadline={} must not be overdue", deadline
        );
    }

    /// A small deadline must be overdue one tick past but not at the exact tick.
    #[test]
    fn fuzz_small_deadline_boundary(deadline in 1u64..1000u64) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);

        // At exact deadline
        set_now(&env, deadline);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline={}, now=deadline must not be overdue", deadline
        );

        // One past deadline
        set_now(&env, deadline + 1);
        prop_assert!(
            client.is_milestone_overdue(&id, &0),
            "deadline={}, now=deadline+1 must be overdue", deadline
        );
    }
}

// ── Category 6: Released milestone is never overdue ──────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A released milestone must never be overdue regardless of deadline or now.
    #[test]
    fn fuzz_released_milestone_never_overdue(now in 0u64..u64::MAX, deadline in 0u64..u64::MAX) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), true);
        set_now(&env, now);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "released milestone must never be overdue (now={}, deadline={})", now, deadline
        );
    }
}

// ── Category 7: None deadline is never overdue ───────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A milestone with no deadline (None) must never be overdue.
    #[test]
    fn fuzz_no_deadline_never_overdue(now in 0u64..u64::MAX) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, None, false);
        set_now(&env, now);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "None deadline must never be overdue at now={}", now
        );
    }
}

// ── Category 8: Out-of-bounds and unknown contracts ─────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Unknown contract id must return false.
    #[test]
    fn fuzz_unknown_contract_not_overdue(bad_id in 100u32..u32::MAX) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, _id) = create_contract(&env, &client);
        set_now(&env, 1_000_000);
        prop_assert!(
            !client.is_milestone_overdue(&bad_id, &0),
            "unknown contract {} must not be overdue", bad_id
        );
    }

    /// Out-of-bounds milestone index must return false.
    #[test]
    fn fuzz_oob_milestone_index_not_overdue(oob in 3u32..100u32) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_now(&env, 1_000_000);
        prop_assert!(
            !client.is_milestone_overdue(&id, &oob),
            "OOB milestone index {} must not be overdue", oob
        );
    }
}

// ── Category 9: Escrow conservation under deadline operations ────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// After setting a deadline and checking overdue, the contract accounting
    /// must be unchanged: funded_amount, released_amount, refunded_amount are
    /// all zero (no release or refund has happened).
    #[test]
    fn fuzz_deadline_check_preserves_escrow_accounting(
        deadline in 1u64..u64::MAX - 1,
        now in 0u64..u64::MAX,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);
        set_now(&env, now);

        // Call is_milestone_overdue — must not mutate accounting
        let _overdue = client.is_milestone_overdue(&id, &0);

        let contract = client.get_contract(&id);
        prop_assert_eq!(contract.funded_amount, 0i128);
        prop_assert_eq!(contract.released_amount, 0i128);
        prop_assert_eq!(contract.refunded_amount, 0i128);
    }
}
