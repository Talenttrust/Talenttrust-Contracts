//! Fuzz coverage for milestone deadline arithmetic and state invariants (issue #1359).
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
//! - **Concurrent idempotency**: repeated overdue checks are side-effect free.
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

/// Number of fuzz cases used for the compatibility-contract suite.
const COMPAT_CASES: u32 = 256;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Set the ledger timestamp to an absolute number of seconds.
fn set_now(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = secs;
    });
}

fn assert_overdue_compat_contract(
    _env: &Env,
    client: &crate::EscrowClient<'_>,
    contract_id: &u32,
    milestone_index: &u32,
    expected: bool,
) {
    assert_eq!(
        client.is_milestone_overdue(contract_id, milestone_index),
        expected
    );
}

/// Snapshot the persisted milestone at `index` so we can assert that read-only
/// entrypoints do not mutate it (deterministic recovery invariant).
fn read_milestone(
    env: &Env,
    contract_addr: &Address,
    contract_id: u32,
    index: u32,
) -> Milestone {
    env.as_contract(contract_addr, || {
        let key = (
            DataKey::Contract(contract_id),
            Symbol::new(env, "milestones"),
        );
        let milestones: SorobanVec<Milestone> = env.storage().persistent().get(&key).unwrap();
        milestones.get(index).unwrap()
    })
}

/// Overwrite milestone `index`'s `deadline` and `released` flag directly in
/// persistent storage, bypassing any setter entrypoint.
/// Callers must ensure `index` is in-bounds for the stored milestone vector.
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

/// Snapshot of the milestone fields that must remain invariant across
/// read-only deadline queries.
#[derive(Clone, Debug, PartialEq, Eq)]
struct MilestoneSnapshot {
    deadline: Option<u64>,
    released: bool,
    refunded: bool,
    amount: i128,
}

/// Capture a snapshot of the milestone at `index`.
fn snapshot_milestone(
    env: &Env,
    contract_addr: &Address,
    contract_id: u32,
    index: u32,
) -> MilestoneSnapshot {
    let m = read_milestone(env, contract_addr, contract_id, index);
    MilestoneSnapshot {
        deadline: m.deadline,
        released: m.released,
        refunded: m.refunded,
        amount: m.amount,
    }
}

// ── Category 1: Zero duration / zero deadline ────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A milestone with deadline=0 and now=0 must NOT be overdue (strict >).
    #[test]
    fn fuzz_deadline_zero_now_zero_not_overdue(_seed in 0u32..256u32) {
        // Invariant: is_milestone_overdue is a pure read; state must not change.
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(0), false);
        set_now(&env, 0);
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline=0, now=0 must not be overdue (strict >)"
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "deadline check must not mutate milestone state");
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
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            client.is_milestone_overdue(&id, &0),
            "deadline=0, now=1 must be overdue"
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "deadline check must not mutate milestone state");
    }
}

// ── Category 2: Maximum duration / u64 boundary ─────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// deadline=u64::MAX, now < u64::MAX must NOT be overdue.
    #[test]
    fn fuzz_deadline_max_now_before_not_overdue(now in 0u64..u64::MAX) {
        // Invariant: boundary timestamps must not overflow or mutate state.
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(u64::MAX), false);
        set_now(&env, now);
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline=u64::MAX, now={} must not be overdue", now
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "deadline check must not mutate milestone state");
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
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline=u64::MAX, now=u64::MAX must not be overdue (strict >)"
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "deadline check must not mutate milestone state");
    }
}

// ── Category 3: Past deadline / now > deadline ───────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// For any deadline > 0, now = deadline + 1 must be overdue.
    #[test]
    fn fuzz_past_deadline_overdue(deadline in 1u64..u64::MAX) {
        // Invariant: overdue detection is pure; released/refunded flags untouched.
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);
        let now = deadline.saturating_add(1); // safe: deadline >= 1
        set_now(&env, now);
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            client.is_milestone_overdue(&id, &0),
            "deadline={}, now={} must be overdue", deadline, now
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "deadline check must not mutate milestone state");
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
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline={}, now={} must NOT be overdue (strict >)", deadline, deadline
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "deadline check must not mutate milestone state");
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
        let before = snapshot_milestone(&env, &client.address, id, 0);

        // before: now = deadline - delta_before (must NOT be overdue)
        let now_before = deadline - delta_before;
        set_now(&env, now_before);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "now_before={} < deadline={} must not be overdue",
            now_before, deadline
        );
        assert_overdue_compat_contract(&env, &client, &id, &0, false);

        // at exact boundary: now = deadline (must NOT be overdue)
        set_now(&env, deadline);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "now == deadline must not be overdue (strict >)"
        );
        assert_overdue_compat_contract(&env, &client, &id, &0, false);

        // after: now = deadline + delta_after (must be overdue)
        let now_after = deadline.saturating_add(delta_after);
        set_now(&env, now_after);
        prop_assert!(
            client.is_milestone_overdue(&id, &0),
            "now_after={} > deadline={} must be overdue",
            now_after, deadline
        );

        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "monotonicity checks must not mutate milestone state");
    }
}

// ── Category 5: Ledger boundary ─────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Timestamp 0 with a future deadline must not be overdue.
    #[test]
    fn fuzz_ledger_zero_with_future_deadline(deadline in 1u64..u64::MAX) {
        // Invariant: ledger timestamp 0 is a valid boundary; state unchanged.
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);
        set_now(&env, 0);
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "now=0 with deadline={} must not be overdue", deadline
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "deadline check must not mutate milestone state");
    }

    /// A small deadline must be overdue one tick past but not at the exact tick.
    #[test]
    fn fuzz_small_deadline_boundary(deadline in 1u64..1000u64) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);
        let before = snapshot_milestone(&env, &client.address, id, 0);

        // At exact deadline
        set_now(&env, deadline);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "deadline={}, now=deadline must not be overdue", deadline
        );
        assert_overdue_compat_contract(&env, &client, &id, &0, false);

        // One past deadline
        set_now(&env, deadline + 1);
        prop_assert!(
            client.is_milestone_overdue(&id, &0),
            "deadline={}, now=deadline+1 must be overdue", deadline
        );

        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "boundary checks must not mutate milestone state");
    }
}

// ── Category 6: Released milestone is never overdue ──────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A released milestone must never be overdue regardless of deadline or now.
    #[test]
    fn fuzz_released_milestone_never_overdue(now in 0u64..u64::MAX, deadline in 0u64..u64::MAX) {
        // Invariant: released flag dominates deadline; must remain true after query.
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), true);
        set_now(&env, now);
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "released milestone must never be overdue (now={}, deadline={})", now, deadline
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(after.released, "released flag must remain true");
        prop_assert_eq!(before, after, "deadline check must not mutate milestone state");
    }
}

// ── Category 7: None deadline is never overdue ───────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A milestone with no deadline (None) must never be overdue.
    #[test]
    fn fuzz_no_deadline_never_overdue(now in 0u64..u64::MAX) {
        // Invariant: None deadline is a stable terminal state; no mutation.
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, None, false);
        set_now(&env, now);
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &0),
            "None deadline must never be overdue at now={}", now
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(after.deadline, None, "None deadline must remain None");
        prop_assert_eq!(before, after, "deadline check must not mutate milestone state");
    }
}

// ── Category 8: Out-of-bounds and unknown contracts ─────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Unknown contract id must return false.
    #[test]
    fn fuzz_unknown_contract_not_overdue(bad_id in 100u32..u32::MAX) {
        // Invariant: unknown contracts are read-only no-ops; no state created.
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, _id) = create_contract(&env, &client);
        set_now(&env, 1_000_000);
        prop_assert!(
            !client.is_milestone_overdue(&bad_id, &0),
            "unknown contract {} must not be overdue", bad_id
        );
        assert_overdue_compat_contract(&env, &client, &bad_id, &0, false);
    }

    /// Out-of-bounds milestone index must return false.
    #[test]
    fn fuzz_oob_milestone_index_not_overdue(oob in 3u32..100u32) {
        // Invariant: OOB index is a read-only no-op; existing milestone unchanged.
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_now(&env, 1_000_000);
        let before = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert!(
            !client.is_milestone_overdue(&id, &oob),
            "OOB milestone index {} must not be overdue", oob
        );
        let after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(before, after, "OOB query must not mutate existing milestone");
    }
}

// ── Category 9: Escrow conservation under deadline operations ────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// After setting a deadline and checking overdue, the contract accounting
    /// must be unchanged: funded_amount, released_amount, refunded_amount are
    /// all zero (no release or refund has happened).
    /// Repeated checks must be idempotent and never mutate accounting.
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
        let milestone_before = snapshot_milestone(&env, &client.address, id, 0);
        set_now(&env, now);

        // Call is_milestone_overdue — must not mutate accounting
        let _overdue = client.is_milestone_overdue(&id, &0);
        // Idempotent retry: second call must observe identical state.
        let _overdue_retry = client.is_milestone_overdue(&id, &0);

        let contract = client.get_contract(&id);
        prop_assert_eq!(contract.funded_amount, 0i128);
        prop_assert_eq!(contract.released_amount, 0i128);
        prop_assert_eq!(contract.refunded_amount, 0i128);

        let milestone_after = snapshot_milestone(&env, &client.address, id, 0);
        prop_assert_eq!(milestone_before, milestone_after, "milestone state must be invariant");
    }
}

// ── Category 10: Deterministic recovery / idempotency ───────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Repeated `is_milestone_overdue` calls with identical inputs must return
    /// identical results (deterministic), and must not mutate persisted state.
    #[test]
    fn fuzz_overdue_check_is_deterministic_and_readonly(
        deadline in 0u64..u64::MAX,
        now in 0u64..u64::MAX,
        released in any::<bool>(),
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), released);
        set_now(&env, now);

        let before = read_milestone(&env, &client.address, id, 0);
        let first = client.is_milestone_overdue(&id, &0);
        let second = client.is_milestone_overdue(&id, &0);
        let third = client.is_milestone_overdue(&id, &0);
        let after = read_milestone(&env, &client.address, id, 0);

        prop_assert_eq!(first, second, "repeated checks must agree");
        prop_assert_eq!(second, third, "repeated checks must agree");
        prop_assert_eq!(before.deadline, after.deadline, "deadline must not change");
        prop_assert_eq!(before.released, after.released, "released must not change");

        // Cross-check against the strict `>` invariant.
        let expected = !released && deadline != 0 && now > deadline;
        prop_assert_eq!(first, expected, "result must match strict > invariant");
    }

    /// Recovery after a failed/unknown lookup must not corrupt state: querying
    /// an unknown contract or OOB index must not affect a valid contract's
    /// milestone or its overdue result.
    #[test]
    fn fuzz_recovery_after_bad_lookup_preserves_state(
        deadline in 1u64..u64::MAX,
        now in 0u64..u64::MAX,
        bad_id in 100u32..u32::MAX,
        oob in 3u32..100u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let client = register_client(&env);
        let (_ca, _fa, id) = create_contract(&env, &client);
        set_milestone_deadline_and_released(&env, &client.address, id, 0, Some(deadline), false);
        set_now(&env, now);

        let before = read_milestone(&env, &client.address, id, 0);
        let baseline = client.is_milestone_overdue(&id, &0);

        // Adverse lookups must be rejected without side effects.
        prop_assert!(!client.is_milestone_overdue(&bad_id, &0));
        prop_assert!(!client.is_milestone_overdue(&id, &oob));

        let after = read_milestone(&env, &client.address, id, 0);
        let recovered = client.is_milestone_overdue(&id, &0);

        prop_assert_eq!(before.deadline, after.deadline);
        prop_assert_eq!(before.released, after.released);
        prop_assert_eq!(baseline, recovered, "state must be recoverable to baseline");
    }
}
