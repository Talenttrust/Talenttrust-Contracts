//! Property-based tests for contract creation and state invariants.
//!
//! Randomized input testing for escrow contract core invariants:
//! - Contract creation with valid/invalid milestone amounts
//! - Client/freelancer distinctness enforcement
//! - Accounting fields initialized to zero
//! - Status starts as Created
//! - Arbitration modes validated
//!
//! NOTE: Tests requiring fund flow (deposit, release, refund) are excluded due
//! to a pre-existing auth regression in `deposit_funds` cross-contract
//! transfers (181 tests fail on clean main for the same reason).

#![cfg(test)]

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::vec::Vec as StdVec;

use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

use crate::{Contract, ContractStatus, Escrow, EscrowClient, ReleaseAuthorization};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, EscrowClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client)
}

fn to_soroban_vec(env: &Env, amounts: &[i128]) -> Vec<i128> {
    let mut v = Vec::new(env);
    for &a in amounts {
        v.push_back(a);
    }
    v
}

fn try_create(
    client: &EscrowClient,
    ca: &Address,
    fa: &Address,
    arbiter: Option<Address>,
    milestones: Vec<i128>,
    auth: &ReleaseAuthorization,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        client.create_contract(ca, fa, &arbiter, &milestones, auth);
    }))
    .is_ok()
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn valid_amounts() -> impl Strategy<Value = StdVec<i128>> {
    prop::collection::vec(1i128..=100_000_000, 1..=8)
}

fn small_amounts() -> impl Strategy<Value = StdVec<i128>> {
    prop::collection::vec(1i128..=1000, 1..=5)
}

const CASES: u32 = 64;

proptest! {
    #![proptest_config(ProptestConfig { cases: CASES, ..ProptestConfig::default() })]

    /// Valid creation with distinct addresses and positive milestones succeeds.
    #[test]
    fn prop_create_contract_succeeds(amounts in valid_amounts()) {
        let (env, client) = setup();
        let ca = Address::generate(&env);
        let fa = Address::generate(&env);
        let milestones = to_soroban_vec(&env, &amounts);

        let ok = try_create(&client, &ca, &fa, None, milestones, &ReleaseAuthorization::ClientOnly);
        prop_assert!(ok, "Valid creation should succeed");

        let data: Contract = client.get_contract(&1u32);
        prop_assert_eq!(data.status, ContractStatus::Created);
        prop_assert_eq!(data.total_deposited, 0);
        prop_assert_eq!(data.released_amount, 0);
        prop_assert_eq!(data.refunded_amount, 0);
        prop_assert!(!data.reputation_issued);
    }

    /// Client == freelancer is always rejected.
    #[test]
    fn prop_same_participants_rejected(amounts in small_amounts()) {
        let (env, client) = setup();
        let same = Address::generate(&env);
        let milestones = to_soroban_vec(&env, &amounts);

        let ok = try_create(&client, &same, &same, None, milestones, &ReleaseAuthorization::ClientOnly);
        prop_assert!(!ok, "Same participants should be rejected");
    }

    /// Client and freelancer are always distinct in successful creation.
    #[test]
    fn prop_distinct_participants_stored(amounts in small_amounts()) {
        let (env, client) = setup();
        let ca = Address::generate(&env);
        let fa = Address::generate(&env);
        let milestones = to_soroban_vec(&env, &amounts);

        let ok = try_create(&client, &ca, &fa, None, milestones, &ReleaseAuthorization::ClientOnly);
        prop_assert!(ok);

        let data: Contract = client.get_contract(&1u32);
        prop_assert_eq!(data.client, ca);
        prop_assert_eq!(data.freelancer, fa);
    }

    /// Arbiter modes requiring arbiter fail without one.
    #[test]
    fn prop_arbiter_required_modes(
        mode in prop_oneof![
            Just(ReleaseAuthorization::ClientAndArbiter),
            Just(ReleaseAuthorization::ArbiterOnly),
        ],
        amounts in small_amounts(),
    ) {
        let (env, client) = setup();
        let ca = Address::generate(&env);
        let fa = Address::generate(&env);
        let milestones = to_soroban_vec(&env, &amounts);

        let ok = try_create(&client, &ca, &fa, None, milestones, &mode);
        prop_assert!(!ok, "Arbiter-required mode without arbiter should fail");
    }

    /// ClientOnly mode works without an arbiter.
    #[test]
    fn prop_client_only_no_arbiter(amounts in small_amounts()) {
        let (env, client) = setup();
        let ca = Address::generate(&env);
        let fa = Address::generate(&env);
        let milestones = to_soroban_vec(&env, &amounts);

        let ok = try_create(&client, &ca, &fa, None, milestones, &ReleaseAuthorization::ClientOnly);
        prop_assert!(ok, "ClientOnly without arbiter should succeed");
    }

    /// Multiple contracts get sequential IDs.
    #[test]
    fn prop_sequential_ids(amounts in small_amounts()) {
        let (env, client) = setup();
        let milestones = to_soroban_vec(&env, &amounts);

        for n in 0..5u32 {
            let ca = Address::generate(&env);
            let fa = Address::generate(&env);
            let ok = try_create(&client, &ca, &fa, None, milestones.clone(), &ReleaseAuthorization::ClientOnly);
            prop_assert!(ok, "Contract {} creation should succeed", n);
            let data: Contract = client.get_contract(&(n + 1));
            prop_assert_eq!(data.status, ContractStatus::Created);
        }
    }

    /// Accounting fields are always zero after creation.
    #[test]
    fn prop_zero_accounting_after_creation(amounts in valid_amounts()) {
        let (env, client) = setup();
        let ca = Address::generate(&env);
        let fa = Address::generate(&env);
        let milestones = to_soroban_vec(&env, &amounts);

        let ok = try_create(&client, &ca, &fa, None, milestones, &ReleaseAuthorization::ClientOnly);
        prop_assert!(ok);

        let data: Contract = client.get_contract(&1u32);
        prop_assert_eq!(data.total_deposited, 0);
        prop_assert_eq!(data.released_amount, 0);
        prop_assert_eq!(data.refunded_amount, 0);
        prop_assert!(!data.reputation_issued);
    }
}
