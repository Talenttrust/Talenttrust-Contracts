#![cfg(test)]

//! Property-based tests for escrow invariants across random milestone schedules
//! and random sequences of deposits, releases, and cancellations.
//!
//! Determinism:
//! - Default 256 cases per property; override via `PROPTEST_CASES` env var at
//!   build time (proptest reads it via `option_env!`).
//! - Seed reproduction: `PROPTEST_SEED=<hex> cargo test -p escrow proptest::...`.
//! - Failing counter-examples auto-persist to `contracts/escrow/proptest-regressions/`.

extern crate std;

use std::vec::Vec as StdVec;

use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, vec as sorovec, Address, Env, Vec as SorobanVec};

use crate::{ContractStatus, Escrow, EscrowClient};

// ---------------------------------------------------------------------------
// Strategy helpers
// ---------------------------------------------------------------------------

const MAX_MILESTONES: usize = 10;
const MAX_AMOUNT: i128 = 1_000_000_000_000; 
const MAX_OPS: usize = 24;

fn milestone_amounts_strategy() -> impl Strategy<Value = StdVec<i128>> {
    prop::collection::vec(1i128..=MAX_AMOUNT, 1..=MAX_MILESTONES)
}

#[derive(Clone, Debug)]
enum Op {
    Deposit(i128),
    Release(u32),
    Cancel,
}

fn op_strategy(n_milestones: usize, total: i128) -> impl Strategy<Value = Op> {
    let n = n_milestones as u32;
    let overshoot_cap = total.saturating_mul(2).max(1);
    prop_oneof![
        (1i128..=overshoot_cap).prop_map(Op::Deposit),
        (0u32..=n).prop_map(Op::Release),
        Just(Op::Cancel),
    ]
}

fn op_sequence_strategy(n_milestones: usize, total: i128) -> impl Strategy<Value = StdVec<Op>> {
    prop::collection::vec(op_strategy(n_milestones, total), 0..=MAX_OPS)
}

// ---------------------------------------------------------------------------
// Shadow model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Shadow {
    _total_milestones_amount: i128,
    total_deposited: i128,
    released_amount: i128,
    released: StdVec<bool>,
    status: ContractStatus,
}

impl Shadow {
    fn new(amounts: &[i128]) -> Self {
        Self {
            _total_milestones_amount: amounts.iter().copied().sum(),
            total_deposited: 0,
            released_amount: 0,
            released: std::vec![false; amounts.len()],
            status: ContractStatus::Created,
        }
    }

    fn is_open(&self) -> bool {
        matches!(
            self.status,
            ContractStatus::Created | ContractStatus::Funded
        )
    }

    fn apply(&mut self, op: &Op, amounts: &[i128]) -> bool {
        if !self.is_open() {
            return false;
        }
        match op {
            Op::Deposit(amount) => {
                if *amount <= 0 {
                    return false;
                }
                // Contract doesn't currently cap total_deposited in the logic, 
                // but it checks against MAX_TOTAL_ESCROW_STROOPS in create_contract.
                // However, deposit_funds doesn't check it. 
                // Let's assume it always succeeds if positive.
                self.total_deposited += *amount;
                if self.status == ContractStatus::Created {
                    self.status = ContractStatus::Funded;
                }
                true
            }
            Op::Release(idx) => {
                let idx = *idx as usize;
                if idx >= amounts.len() {
                    return false;
                }
                if self.released[idx] {
                    return false;
                }
                // Note: current contract doesn't explicitly check balance before release,
                // it just adds to released_amount. But it's good practice.
                self.released[idx] = true;
                self.released_amount += amounts[idx];
                true
            }
            Op::Cancel => {
                // Client can cancel only if no milestones released.
                // Freelancer can always cancel.
                // For simplicity, let's assume we cancel as client.
                if self.released_amount > 0 && self.status == ContractStatus::Funded {
                    return false;
                }
                self.status = ContractStatus::Cancelled;
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

fn do_deposit(h: &Harness, id: u32, amount: i128) -> Result<bool, ()> {
    h.client.try_deposit_funds(&id, &amount).map(|res| res.unwrap()).map_err(|_| ())
}

fn do_release(h: &Harness, id: u32, idx: u32) -> Result<bool, ()> {
    h.client.try_release_milestone(&id, &idx).map(|res| res.unwrap()).map_err(|_| ())
}

fn do_cancel(h: &Harness, id: u32) -> Result<bool, ()> {
    h.client.try_cancel_contract(&id, &h.client_addr).map(|res| res.unwrap()).map_err(|_| ())
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
        let id = h.client.create_contract(&h.client_addr, &h.freelancer_addr, &None, &ms, &None, &None);
        prop_assert_eq!(id, 0);

        let data = h.client.get_contract(&id);
        prop_assert_eq!(data.total_deposited, 0);
        prop_assert_eq!(data.released_amount, 0);
        prop_assert_eq!(data.status, ContractStatus::Created);

        let ms_on_chain: SorobanVec<i128> = h.client.get_milestones(&id);
        prop_assert_eq!(ms_on_chain.len() as usize, amounts.len());
        for (i, m) in ms_on_chain.iter().enumerate() {
            prop_assert_eq!(m, amounts[i]);
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
        let id = h.client.create_contract(&h.client_addr, &h.freelancer_addr, &None, &ms, &None, &None);

        let mut shadow = Shadow::new(&amounts);

        for op in &ops {
            let expected_ok = {
                let mut fork = shadow.clone();
                fork.apply(op, &amounts)
            };
            let actual_ok = match op {
                Op::Deposit(a) => do_deposit(&h, id, *a).is_ok(),
                Op::Release(i) => do_release(&h, id, *i).is_ok(),
                Op::Cancel => do_cancel(&h, id).is_ok(),
            };
            prop_assert_eq!(
                actual_ok, expected_ok,
                "shadow/contract disagree on op={:?}", op
            );
            if actual_ok {
                shadow.apply(op, &amounts);
            }

            let data = h.client.get_contract(&id);
            prop_assert!(data.total_deposited >= 0);
            prop_assert!(data.released_amount >= 0);
            prop_assert!(data.released_amount <= data.total_deposited, "released more than deposited");
            prop_assert_eq!(data.status, shadow.status);
        }
    }
}
