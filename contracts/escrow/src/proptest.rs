//! Property-based tests for the escrow accounting invariant.
//!
//! Drives randomised sequences of `deposit_funds`, `release_milestone`, and
//! `cancel_contract` against the live Soroban test environment and asserts
//! after every operation that:
//!
//!   `total_deposited == released_amount + refunded_amount + available_balance`
//!   `available_balance >= 0`
//!
//! ## Running
//!
//! ```sh
//! # Default 256 cases per property:
//! cargo test -p escrow proptest
//!
//! # More cases:
//! PROPTEST_CASES=1024 cargo test -p escrow proptest
//!
//! # Reproduce a specific failure:
//! PROPTEST_SEED=<hex> cargo test -p escrow proptest
//! ```
//!
//! Failing seeds are auto-saved to `proptest-regressions/proptest.txt`.

#![cfg(test)]

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::vec::Vec as StdVec;

use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SorobanVec};

use crate::{ContractStatus, DepositMode, Escrow, EscrowClient};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_MS: usize = 8;
const MAX_AMOUNT: i128 = 1_000_000_000; // well below overflow on any realistic sum
const MAX_OPS: usize = 20;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn milestone_amounts() -> impl Strategy<Value = StdVec<i128>> {
    prop::collection::vec(1i128..=MAX_AMOUNT, 1..=MAX_MS)
}

#[derive(Clone, Debug)]
enum Op {
    Deposit(i128),
    Release(u32),
    Cancel,
}

fn op_strategy(n: usize, total: i128) -> impl Strategy<Value = Op> {
    let n32 = n as u32;
    let overshoot = total.saturating_mul(2).max(1);
    prop_oneof![
        (1i128..=overshoot).prop_map(Op::Deposit),
        (0u32..=(n32 + 1)).prop_map(Op::Release), // +1 exercises out-of-bounds
        Just(Op::Cancel),
    ]
}

fn ops_strategy(n: usize, total: i128) -> impl Strategy<Value = StdVec<Op>> {
    prop::collection::vec(op_strategy(n, total), 0..=MAX_OPS)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_soroban_vec(env: &Env, amounts: &[i128]) -> SorobanVec<i128> {
    let mut v = SorobanVec::new(env);
    for &a in amounts {
        v.push_back(a);
    }
    v
}

fn sum(amounts: &[i128]) -> i128 {
    amounts.iter().copied().sum()
}

struct Harness {
    env: Env,
    client_addr: Address,
    freelancer_addr: Address,
}

impl Harness {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        Harness {
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

fn try_deposit(client: &EscrowClient, id: u32, amount: i128) -> bool {
    catch_unwind(AssertUnwindSafe(|| client.deposit_funds(&id, &amount))).is_ok()
}

fn try_release(client: &EscrowClient, id: u32, idx: u32) -> bool {
    catch_unwind(AssertUnwindSafe(|| client.release_milestone(&id, &idx))).is_ok()
}

fn try_cancel(client: &EscrowClient, id: u32, caller: &Address) -> bool {
    catch_unwind(AssertUnwindSafe(|| client.cancel_contract(&id, caller))).is_ok()
}

// ---------------------------------------------------------------------------
// Invariant checker (mirrors check_accounting_invariant in lib.rs)
// ---------------------------------------------------------------------------

fn assert_invariant(client: &EscrowClient, id: u32) {
    let d = client.get_contract(&id);
    let available = d.total_deposited - d.released_amount - d.refunded_amount;
    assert!(available >= 0, "available_balance < 0: {d:?}");
    assert_eq!(
        d.total_deposited,
        d.released_amount + d.refunded_amount + available,
        "accounting invariant violated: {d:?}"
    );
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

const DEFAULT_CASES: u32 = 256;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: DEFAULT_CASES,
        ..ProptestConfig::default()
    })]

    /// After every operation the accounting invariant must hold and
    /// available_balance must never go negative.
    #[test]
    fn prop_accounting_invariant_holds_under_random_ops(
        (amounts, ops) in milestone_amounts().prop_flat_map(|amounts| {
            let total = sum(&amounts);
            let n = amounts.len();
            (Just(amounts), ops_strategy(n, total))
        })
    ) {
        let h = Harness::new();
        let client = h.escrow_client();
        let ms = to_soroban_vec(&h.env, &amounts);
        let id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &ms,
            &DepositMode::Incremental,
        );

        assert_invariant(&client, id);

        for op in &ops {
            match op {
                Op::Deposit(a) => { let _ = try_deposit(&client, id, *a); }
                Op::Release(i) => { let _ = try_release(&client, id, *i); }
                Op::Cancel => { let _ = try_cancel(&client, id, &h.client_addr); }
            }
            // Invariant must hold regardless of whether the op succeeded.
            assert_invariant(&client, id);
        }
    }

    /// Depositing the exact total then releasing all milestones one-by-one
    /// must always satisfy the invariant and end in Completed status.
    #[test]
    fn prop_full_release_sequence_invariant(amounts in milestone_amounts()) {
        let h = Harness::new();
        let client = h.escrow_client();
        let total = sum(&amounts);
        let ms = to_soroban_vec(&h.env, &amounts);
        let id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &ms,
            &DepositMode::ExactTotal,
        );

        assert!(try_deposit(&client, id, total));
        assert_invariant(&client, id);

        for i in 0..amounts.len() as u32 {
            assert!(try_release(&client, id, i));
            assert_invariant(&client, id);
        }

        let data = client.get_contract(&id);
        prop_assert_eq!(data.status, ContractStatus::Completed);
        prop_assert_eq!(data.released_amount, total);
        prop_assert_eq!(data.refunded_amount, 0);
        prop_assert_eq!(data.total_deposited, total);
    }

    /// Over-release attempts (releasing the same milestone twice) must be
    /// rejected and must not violate the invariant.
    #[test]
    fn prop_double_release_rejected_invariant_preserved(
        amounts in milestone_amounts(),
        target_raw in 0u32..MAX_MS as u32,
    ) {
        let n = amounts.len() as u32;
        prop_assume!(n > 0);
        let target = target_raw % n;

        let h = Harness::new();
        let client = h.escrow_client();
        let total = sum(&amounts);
        let ms = to_soroban_vec(&h.env, &amounts);
        let id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &ms,
            &DepositMode::ExactTotal,
        );

        assert!(try_deposit(&client, id, total));
        assert!(try_release(&client, id, target));
        assert_invariant(&client, id);

        let before = client.get_contract(&id);
        // Second release of the same milestone must fail.
        prop_assert!(!try_release(&client, id, target));
        let after = client.get_contract(&id);
        // State must be unchanged.
        prop_assert_eq!(before.released_amount, after.released_amount);
        prop_assert_eq!(before.total_deposited, after.total_deposited);
        assert_invariant(&client, id);
    }

    /// Incremental deposits that sum to the total must satisfy the invariant
    /// at every step and end in Funded status.
    #[test]
    fn prop_incremental_deposit_invariant(amounts in milestone_amounts()) {
        let h = Harness::new();
        let client = h.escrow_client();
        let ms = to_soroban_vec(&h.env, &amounts);
        let id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &ms,
            &DepositMode::Incremental,
        );

        for &a in &amounts {
            assert!(try_deposit(&client, id, a));
            assert_invariant(&client, id);
        }

        let data = client.get_contract(&id);
        prop_assert_eq!(data.status, ContractStatus::Funded);
        prop_assert_eq!(data.total_deposited, sum(&amounts));
    }

    /// Cancelling a contract must not violate the invariant.
    #[test]
    fn prop_cancel_preserves_invariant(amounts in milestone_amounts()) {
        let h = Harness::new();
        let client = h.escrow_client();
        let ms = to_soroban_vec(&h.env, &amounts);
        let id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &ms,
            &DepositMode::Incremental,
        );

        // Optionally deposit some funds first.
        let partial = amounts[0];
        let _ = try_deposit(&client, id, partial);
        assert_invariant(&client, id);

        assert!(try_cancel(&client, id, &h.client_addr));
        assert_invariant(&client, id);

        let data = client.get_contract(&id);
        prop_assert_eq!(data.status, ContractStatus::Cancelled);
    }

    /// Adversarial: depositing more than the total must be rejected and must
    /// not corrupt the invariant.
    #[test]
    fn prop_overfund_rejected_invariant_preserved(amounts in milestone_amounts()) {
        let h = Harness::new();
        let client = h.escrow_client();
        let total = sum(&amounts);
        let ms = to_soroban_vec(&h.env, &amounts);
        let id = client.create_contract(
            &h.client_addr,
            &h.freelancer_addr,
            &ms,
            &DepositMode::Incremental,
        );

        // Deposit the exact total first.
        assert!(try_deposit(&client, id, total));
        assert_invariant(&client, id);

        // Any further deposit must be rejected.
        prop_assert!(!try_deposit(&client, id, 1));
        assert_invariant(&client, id);
    }
}
