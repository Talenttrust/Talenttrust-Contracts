#![cfg(test)]

//! Property-based tests for escrow invariants across random milestone schedules
//! and random sequences of deposits, releases, refunds, and approvals.
//!
//! Determinism:
//! - Default 256 cases per property; override via `PROPTEST_CASES` env var at
//!   build time (proptest reads it via `option_env!`).
//! - Seed reproduction: `PROPTEST_SEED=<hex> cargo test -p escrow proptest::...`.
//! - Failing counter-examples auto-persist to `contracts/escrow/proptest-regressions/`.

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::vec::Vec as StdVec;

use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, vec as sorovec, Address, Env, Vec as SorobanVec};

use crate::{ContractStatus, Escrow, EscrowClient, Milestone};

// ---------------------------------------------------------------------------
// Strategy helpers
// ---------------------------------------------------------------------------

const MAX_MILESTONES: usize = 8;
const MAX_AMOUNT: i128 = 1_000_000_000_000; // 10^12 stroops — well below i128 overflow on any realistic sum.
const MAX_OPS: usize = 24;

fn milestone_amounts_strategy() -> impl Strategy<Value = StdVec<i128>> {
    prop::collection::vec(1i128..=MAX_AMOUNT, 1..=MAX_MILESTONES)
}

#[derive(Clone, Debug)]
enum Op {
    Deposit(i128),
    Release(u32),
    Refund(StdVec<u32>),
}

fn op_strategy(n_milestones: usize, total: i128) -> impl Strategy<Value = Op> {
    let n = n_milestones as u32;
    let overshoot_cap = total.saturating_mul(2).max(1);
    prop_oneof![
        (1i128..=overshoot_cap).prop_map(Op::Deposit),
        // Allow n (one past the end) to exercise out-of-bounds panic.
        (0u32..=n).prop_map(Op::Release),
        prop::collection::vec(0u32..=n, 1..=MAX_MILESTONES).prop_map(Op::Refund),
    ]
}

fn op_sequence_strategy(n_milestones: usize, total: i128) -> impl Strategy<Value = StdVec<Op>> {
    prop::collection::vec(op_strategy(n_milestones, total), 0..=MAX_OPS)
}

// ---------------------------------------------------------------------------
// Shadow model — mirrors the contract's decision logic to decide whether each
// op should succeed, without depending on the contract's output.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Shadow {
    total_amount: i128,
    funded_amount: i128,
    released_amount: i128,
    refunded_amount: i128,
    released: StdVec<bool>,
    refunded: StdVec<bool>,
    status: ContractStatus,
}

impl Shadow {
    fn new(amounts: &[i128]) -> Self {
        Self {
            total_amount: amounts.iter().copied().sum(),
            funded_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
            released: std::vec![false; amounts.len()],
            refunded: std::vec![false; amounts.len()],
            status: ContractStatus::Created,
        }
    }

    fn available(&self) -> i128 {
        self.funded_amount - self.released_amount - self.refunded_amount
    }

    fn is_open(&self) -> bool {
        matches!(
            self.status,
            ContractStatus::Created | ContractStatus::Funded
        )
    }

    fn recompute_status(&mut self) {
        let mut any_refunded = false;
        let mut all_settled = true;
        for i in 0..self.released.len() {
            if self.refunded[i] {
                any_refunded = true;
            }
            if !self.released[i] && !self.refunded[i] {
                all_settled = false;
            }
        }
        self.status = if all_settled {
            if any_refunded {
                ContractStatus::Refunded
            } else {
                ContractStatus::Completed
            }
        } else if self.funded_amount > 0 {
            ContractStatus::Funded
        } else {
            ContractStatus::Created
        };
    }

    /// Returns true if the op should succeed against this model.
    fn apply(&mut self, op: &Op, amounts: &[i128]) -> bool {
        if !self.is_open() {
            return false;
        }
        match op {
            Op::Deposit(amount) => {
                if *amount <= 0 {
                    return false;
                }
                let new_funded = self.funded_amount + *amount;
                if new_funded > self.total_amount {
                    return false;
                }
                self.funded_amount = new_funded;
                self.recompute_status();
                true
            }
            Op::Release(idx) => {
                let idx = *idx as usize;
                if idx >= amounts.len() {
                    return false;
                }
                if self.released[idx] || self.refunded[idx] {
                    return false;
                }
                if self.available() < amounts[idx] {
                    return false;
                }
                self.released[idx] = true;
                self.released_amount += amounts[idx];
                self.recompute_status();
                true
            }
            Op::Refund(ids) => {
                if ids.is_empty() {
                    return false;
                }
                let mut seen: StdVec<u32> = StdVec::new();
                let mut sum: i128 = 0;
                for id in ids {
                    if seen.contains(id) {
                        return false;
                    }
                    seen.push(*id);
                    let idx = *id as usize;
                    if idx >= amounts.len() {
                        return false;
                    }
                    if self.released[idx] || self.refunded[idx] {
                        return false;
                    }
                    sum += amounts[idx];
                }
                if self.available() < sum {
                    return false;
                }
                for id in &seen {
                    self.refunded[*id as usize] = true;
                }
                self.refunded_amount += sum;
                self.recompute_status();
                true
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Harness helpers
// ---------------------------------------------------------------------------

struct Harness<'a> {
    env: Env,
    client: EscrowClient<'a>,
    client_addr: Address,
    freelancer_addr: Address,
}

fn fresh_harness<'a>() -> Harness<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &addr);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    Harness {
        env,
        client,
        client_addr,
        freelancer_addr,
    }
}

fn ids_to_sorovec(env: &Env, v: &[u32]) -> SorobanVec<u32> {
    let mut out = SorobanVec::new(env);
    for x in v {
        out.push_back(*x);
    }
    out
}

fn do_deposit(h: &Harness, id: u32, amount: i128) -> Result<bool, ()> {
    catch_unwind(AssertUnwindSafe(|| h.client.deposit_funds(&id, &amount))).map_err(|_| ())
}

fn do_release(h: &Harness, id: u32, idx: u32) -> Result<bool, ()> {
    catch_unwind(AssertUnwindSafe(|| h.client.release_milestone(&id, &idx))).map_err(|_| ())
}

fn do_refund(h: &Harness, id: u32, ids: &[u32]) -> Result<i128, ()> {
    let env_ids = ids_to_sorovec(&h.env, ids);
    catch_unwind(AssertUnwindSafe(|| {
        h.client.refund_unreleased_milestones(&id, &env_ids)
    }))
    .map_err(|_| ())
}

fn sum_vec(amounts: &[i128]) -> i128 {
    amounts.iter().copied().sum()
}

fn amounts_sorovec(env: &Env, amounts: &[i128]) -> SorobanVec<i128> {
    let mut out = sorovec![env];
    for a in amounts {
        out.push_back(*a);
    }
    out
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

const DEFAULT_CASES: u32 = match option_env!("PROPTEST_CASES") {
    Some(s) => parse_u32_const(s),
    None => 256,
};

// `option_env!` returns a `&'static str`; we need a `const fn` parse because
// `ProptestConfig` expects a `u32` value we can bake into the proptest! macro.
const fn parse_u32_const(s: &str) -> u32 {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut acc: u32 = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < b'0' || b > b'9' {
            return 256;
        }
        acc = acc * 10 + (b - b'0') as u32;
        i += 1;
    }
    if acc == 0 {
        256
    } else {
        acc
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: DEFAULT_CASES,
        .. ProptestConfig::default()
    })]

    #[test]
    fn prop_creation_invariants(amounts in milestone_amounts_strategy()) {
        let h = fresh_harness();
        let ms = amounts_sorovec(&h.env, &amounts);
        let id = h.client.create_contract(&h.client_addr, &h.freelancer_addr, &ms);
        prop_assert_eq!(id, 0);

        let data = h.client.get_contract(&id);
        prop_assert_eq!(data.total_amount, sum_vec(&amounts));
        prop_assert_eq!(data.funded_amount, 0);
        prop_assert_eq!(data.released_amount, 0);
        prop_assert_eq!(data.refunded_amount, 0);
        prop_assert_eq!(data.status, ContractStatus::Created);

        let ms_on_chain: SorobanVec<Milestone> = h.client.get_milestones(&id);
        prop_assert_eq!(ms_on_chain.len() as usize, amounts.len());
        for (i, m) in ms_on_chain.iter().enumerate() {
            prop_assert_eq!(m.amount, amounts[i]);
            prop_assert!(!m.released);
            prop_assert!(!m.refunded);
        }
    }

    #[test]
    fn prop_id_monotonicity_across_multiple_contracts(
        schedules in prop::collection::vec(milestone_amounts_strategy(), 1..=6)
    ) {
        let h = fresh_harness();
        for (expected_id, amounts) in schedules.iter().enumerate() {
            let ms = amounts_sorovec(&h.env, amounts);
            let id = h.client.create_contract(&h.client_addr, &h.freelancer_addr, &ms);
            prop_assert_eq!(id, expected_id as u32);

            let data = h.client.get_contract(&id);
            prop_assert_eq!(data.total_amount, sum_vec(amounts));
            prop_assert_eq!(data.funded_amount, 0);
            prop_assert_eq!(data.status, ContractStatus::Created);
        }
    }

    #[test]
    fn prop_balance_and_status_invariant_under_random_ops(
        (amounts, ops) in milestone_amounts_strategy().prop_flat_map(|amounts| {
            let total = sum_vec(&amounts);
            let n = amounts.len();
            (Just(amounts), op_sequence_strategy(n, total))
        })
    ) {
        let h = fresh_harness();
        let ms = amounts_sorovec(&h.env, &amounts);
        let id = h.client.create_contract(&h.client_addr, &h.freelancer_addr, &ms);

        let mut shadow = Shadow::new(&amounts);
        let mut prev_status = shadow.status;

        for op in &ops {
            let expected_ok = {
                let mut fork = shadow.clone();
                fork.apply(op, &amounts)
            };
            let actual_ok = match op {
                Op::Deposit(a) => do_deposit(&h, id, *a).is_ok(),
                Op::Release(i) => do_release(&h, id, *i).is_ok(),
                Op::Refund(ids) => do_refund(&h, id, ids).is_ok(),
            };
            prop_assert_eq!(
                actual_ok, expected_ok,
                "shadow/contract disagree on op={:?}", op
            );
            if actual_ok {
                shadow.apply(op, &amounts);
            }

            // Invariants on the live contract state.
            let data = h.client.get_contract(&id);
            let ms_chain: SorobanVec<Milestone> = h.client.get_milestones(&id);

            prop_assert!(data.funded_amount >= 0);
            prop_assert!(data.released_amount >= 0);
            prop_assert!(data.refunded_amount >= 0);
            prop_assert!(data.funded_amount <= data.total_amount);
            prop_assert!(
                data.released_amount + data.refunded_amount <= data.funded_amount,
                "negative available balance"
            );

            let mut sum_released: i128 = 0;
            let mut sum_refunded: i128 = 0;
            for (i, m) in ms_chain.iter().enumerate() {
                prop_assert!(
                    !(m.released && m.refunded),
                    "milestone {} is both released and refunded", i
                );
                if m.released { sum_released += m.amount; }
                if m.refunded { sum_refunded += m.amount; }
            }
            prop_assert_eq!(sum_released, data.released_amount);
            prop_assert_eq!(sum_refunded, data.refunded_amount);

            // Status transitions: never go backwards.
            let ok_transition = match (prev_status, data.status) {
                (a, b) if a == b => true,
                (ContractStatus::Created, ContractStatus::Funded) => true,
                (ContractStatus::Funded, ContractStatus::Completed) => true,
                (ContractStatus::Funded, ContractStatus::Refunded) => true,
                _ => false,
            };
            prop_assert!(
                ok_transition,
                "illegal status transition {:?} -> {:?}", prev_status, data.status
            );
            prev_status = data.status;
        }
    }

    #[test]
    fn prop_release_then_refund_exclusivity(
        amounts in milestone_amounts_strategy(),
        target_raw in 0u32..MAX_MILESTONES as u32,
    ) {
        let n = amounts.len() as u32;
        prop_assume!(n > 0);
        let target = target_raw % n;
        let h = fresh_harness();
        let ms = amounts_sorovec(&h.env, &amounts);
        let id = h.client.create_contract(&h.client_addr, &h.freelancer_addr, &ms);
        h.client.deposit_funds(&id, &sum_vec(&amounts));
        h.client.release_milestone(&id, &target);

        let before = h.client.get_contract(&id);
        let refund_res = do_refund(&h, id, &[target]);
        prop_assert!(refund_res.is_err(), "refund of already-released milestone must panic");
        let after = h.client.get_contract(&id);
        prop_assert_eq!(before, after);
    }

    #[test]
    fn prop_refund_then_release_exclusivity(
        amounts in milestone_amounts_strategy(),
        target_raw in 0u32..MAX_MILESTONES as u32,
    ) {
        let n = amounts.len() as u32;
        prop_assume!(n > 0);
        let target = target_raw % n;
        let h = fresh_harness();
        let ms = amounts_sorovec(&h.env, &amounts);
        let id = h.client.create_contract(&h.client_addr, &h.freelancer_addr, &ms);
        h.client.deposit_funds(&id, &sum_vec(&amounts));
        h.client.refund_unreleased_milestones(&id, &ids_to_sorovec(&h.env, &[target]));

        let before = h.client.get_contract(&id);
        let release_res = do_release(&h, id, target);
        prop_assert!(release_res.is_err(), "release of already-refunded milestone must panic");
        let after = h.client.get_contract(&id);
        prop_assert_eq!(before, after);
    }

    #[test]
    fn prop_total_balance_conservation(
        (amounts, ops) in milestone_amounts_strategy().prop_flat_map(|amounts| {
            let total = sum_vec(&amounts);
            let n = amounts.len();
            (Just(amounts), op_sequence_strategy(n, total))
        })
    ) {
        let h = fresh_harness();
        let ms = amounts_sorovec(&h.env, &amounts);
        let id = h.client.create_contract(&h.client_addr, &h.freelancer_addr, &ms);

        for op in &ops {
            let _ = match op {
                Op::Deposit(a) => do_deposit(&h, id, *a).ok().map(|_| ()),
                Op::Release(i) => do_release(&h, id, *i).ok().map(|_| ()),
                Op::Refund(ids) => do_refund(&h, id, ids).ok().map(|_| ()),
            };

            let data = h.client.get_contract(&id);
            let balance = data.funded_amount - data.released_amount - data.refunded_amount;
            prop_assert!(balance >= 0, "escrow balance went negative");
            prop_assert_eq!(
                data.released_amount + data.refunded_amount + balance,
                data.funded_amount,
                "conservation violated"
            );
        }
    }
}
