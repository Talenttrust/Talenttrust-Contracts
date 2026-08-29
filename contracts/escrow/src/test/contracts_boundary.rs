//! Boundary / fuzz-style tests for the contracts module (#1255).
//!
//! Covers min, max, zero, and over-limit inputs for contracts-facing limits and
//! readers, asserting typed [`EscrowError`] codes where guards exist.
//!
//! Bounded proptest runs keep CI time predictable (`PROPTEST_CASES` default 32).
//!
//! ## Unguarded boundaries noted
//! - `validate_contract_id_bounds` (in `contracts.rs`) rejects `contract_id == 0`
//!   with `InvalidContractId`, but the crate-root readers (`get_contract`,
//!   `get_milestones`, `get_milestone`, `get_contract_summary`,
//!   `get_refundable_balance`) do **not** call it — id `0` surfaces as
//!   `ContractNotFound` instead.
//! - `contract_exists(0)` returns `false` and does not panic (intentional).
//! - `get_milestone(id, index)` returns `None` for out-of-range indices rather
//!   than a typed error (by design).
//! - `is_milestone_overdue` / `get_milestone_progress` soft-fail on unknown ids.

#![cfg(test)]

extern crate std;

use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, vec, Address, Env, Vec};

use super::{assert_contract_error, create_client, default_milestones};
use crate::{
    EscrowError, ReleaseAuthorization, MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
    MAX_MAX_BATCH_SETTLEMENT, MAX_MAX_MILESTONES, MAX_MILESTONES, MAX_TOTAL_ESCROW_STROOPS,
    MIN_MAX_BATCH_SETTLEMENT, MIN_MAX_ESCROW_STROOPS, MIN_MAX_MILESTONES,
};

const FUZZ_CASES: u32 = 32;

fn setup_simple() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, id)
}

fn client_of<'a>(env: &'a Env, id: &Address) -> crate::EscrowClient<'a> {
    crate::EscrowClient::new(env, id)
}

// ── set_max_settlement: min / max / zero / over-limit ─────────────────────────

#[test]
fn set_max_settlement_accepts_minimum() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert!(client.set_max_settlement(&MIN_MAX_BATCH_SETTLEMENT));
    assert_eq!(client.get_max_settlement(), MIN_MAX_BATCH_SETTLEMENT);
}

#[test]
fn set_max_settlement_accepts_maximum() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert!(client.set_max_settlement(&MAX_MAX_BATCH_SETTLEMENT));
    assert_eq!(client.get_max_settlement(), MAX_MAX_BATCH_SETTLEMENT);
}

#[test]
fn set_max_settlement_rejects_zero() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_max_settlement(&0),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_settlement_rejects_one_over_maximum() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_max_settlement(&(MAX_MAX_BATCH_SETTLEMENT + 1)),
        EscrowError::LimitOutOfRange,
    );
}

// ── set_max_milestones: min / max / zero / over-limit ─────────────────────────

#[test]
fn set_max_milestones_accepts_minimum() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert!(client.set_max_milestones(&MIN_MAX_MILESTONES));
    assert_eq!(client.get_max_milestones(), MIN_MAX_MILESTONES);
}

#[test]
fn set_max_milestones_accepts_maximum() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert!(client.set_max_milestones(&MAX_MAX_MILESTONES));
    assert_eq!(client.get_max_milestones(), MAX_MAX_MILESTONES);
}

#[test]
fn set_max_milestones_rejects_zero() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_max_milestones(&0),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_milestones_rejects_one_over_maximum() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_max_milestones(&(MAX_MAX_MILESTONES + 1)),
        EscrowError::LimitOutOfRange,
    );
}

// ── set_max_escrow_stroops: min / max / zero / over-limit ─────────────────────

#[test]
fn set_max_escrow_stroops_accepts_minimum() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert!(client.set_max_escrow_stroops(&MIN_MAX_ESCROW_STROOPS));
    assert_eq!(client.get_max_escrow_stroops(), MIN_MAX_ESCROW_STROOPS);
}

#[test]
fn set_max_escrow_stroops_accepts_mainnet_cap() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert!(client.set_max_escrow_stroops(&MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS));
    assert_eq!(
        client.get_max_escrow_stroops(),
        MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
    );
}

#[test]
fn set_max_escrow_stroops_rejects_zero() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_max_escrow_stroops(&0),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_escrow_stroops_rejects_one_over_mainnet_cap() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_max_escrow_stroops(&(MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS + 1)),
        EscrowError::LimitOutOfRange,
    );
}

// ── set_contracts_parameters: min / max / zero / over-limit ────────────────────

#[test]
fn set_contracts_parameters_accepts_min_bounds() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert!(client.set_contracts_parameters(&MIN_MAX_MILESTONES, &MIN_MAX_ESCROW_STROOPS));
    let params = client.get_contracts_parameters();
    assert_eq!(params.max_milestones, MIN_MAX_MILESTONES);
    assert_eq!(params.max_escrow_stroops, MIN_MAX_ESCROW_STROOPS);
}

#[test]
fn set_contracts_parameters_accepts_max_bounds() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert!(client.set_contracts_parameters(
        &MAX_MAX_MILESTONES,
        &MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
    ));
    let params = client.get_contracts_parameters();
    assert_eq!(params.max_milestones, MAX_MAX_MILESTONES);
    assert_eq!(
        params.max_escrow_stroops,
        MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
    );
}

#[test]
fn set_contracts_parameters_rejects_zero_milestones() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_contracts_parameters(&0, &MIN_MAX_ESCROW_STROOPS),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_contracts_parameters_rejects_over_max_milestones() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_contracts_parameters(&(MAX_MAX_MILESTONES + 1), &MIN_MAX_ESCROW_STROOPS),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_contracts_parameters_rejects_zero_escrow() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_contracts_parameters(&MIN_MAX_MILESTONES, &0),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_contracts_parameters_rejects_over_mainnet_escrow_cap() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_set_contracts_parameters(
            &MIN_MAX_MILESTONES,
            &(MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS + 1),
        ),
        EscrowError::LimitOutOfRange,
    );
}

// ── contract_id == 0 ─────────────────────────────────────────────────────────
// Root readers do not invoke validate_contract_id_bounds; id 0 → ContractNotFound.

#[test]
fn get_contract_zero_id_returns_contract_not_found() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(client.try_get_contract(&0), EscrowError::ContractNotFound);
}

#[test]
fn get_contract_summary_zero_id_returns_contract_not_found() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_get_contract_summary(&0),
        EscrowError::ContractNotFound,
    );
}

#[test]
fn get_milestones_zero_id_returns_contract_not_found() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(client.try_get_milestones(&0), EscrowError::ContractNotFound);
}

#[test]
fn get_milestone_zero_id_returns_contract_not_found() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_get_milestone(&0, &0),
        EscrowError::ContractNotFound,
    );
}

#[test]
fn get_refundable_balance_zero_id_returns_contract_not_found() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert_contract_error(
        client.try_get_refundable_balance(&0),
        EscrowError::ContractNotFound,
    );
}

/// Unguarded: existence probe returns false for id 0 without typed error.
#[test]
fn contract_exists_zero_id_returns_false_unguarded() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    assert!(!client.contract_exists(&0));
}

// ── create_contract amount / length boundaries ───────────────────────────────

#[test]
fn create_contract_rejects_empty_milestones() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let empty: Vec<i128> = Vec::new(&env);
    assert_contract_error(
        client.try_create_contract(&c, &f, &None, &empty, &ReleaseAuthorization::ClientOnly),
        EscrowError::EmptyMilestones,
    );
}

#[test]
fn create_contract_rejects_zero_amount_milestone() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, 0_i128],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

#[test]
fn create_contract_accepts_exactly_max_milestones() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..MAX_MILESTONES {
        amounts.push_back(1_i128);
    }
    let id = client.create_contract(&c, &f, &None, &amounts, &ReleaseAuthorization::ClientOnly);
    assert!(id >= 1);
}

#[test]
fn create_contract_rejects_one_over_max_milestones() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..=MAX_MILESTONES {
        amounts.push_back(1_i128);
    }
    assert_contract_error(
        client.try_create_contract(&c, &f, &None, &amounts, &ReleaseAuthorization::ClientOnly),
        EscrowError::TooManyMilestones,
    );
}

#[test]
fn create_contract_accepts_total_exactly_at_cap() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let id = client.create_contract(
        &c,
        &f,
        &None,
        &vec![&env, MAX_TOTAL_ESCROW_STROOPS],
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(id >= 1);
}

#[test]
fn create_contract_rejects_total_one_over_cap() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, MAX_TOTAL_ESCROW_STROOPS + 1],
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::InvalidMilestoneAmount,
    );
}

// ── get_milestone index boundaries ───────────────────────────────────────────

#[test]
fn get_milestone_accepts_first_and_last_index() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &c,
        &f,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let last = milestones.len() - 1;
    assert!(client.get_milestone(&id, &0).is_some());
    assert!(client.get_milestone(&id, &last).is_some());
}

#[test]
fn get_milestone_returns_none_at_count_and_u32_max() {
    let (env, id) = setup_simple();
    let client = client_of(&env, &id);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = default_milestones(&env);
    let id = client.create_contract(
        &c,
        &f,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.get_milestone(&id, &milestones.len()).is_none());
    assert!(client.get_milestone(&id, &u32::MAX).is_none());
}

// ── Bounded fuzz-style property tests ────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(FUZZ_CASES))]

    /// Any settlement limit outside `[MIN, MAX]` is rejected with LimitOutOfRange.
    #[test]
    fn fuzz_set_max_settlement_out_of_range_rejected(
        bad in prop_oneof![
            Just(0u32),
            (MAX_MAX_BATCH_SETTLEMENT + 1)..=u32::MAX,
        ]
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let client = create_client(&env);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        assert_contract_error(
            client.try_set_max_settlement(&bad),
            EscrowError::LimitOutOfRange,
        );
    }

    /// In-range settlement limits always persist.
    #[test]
    fn fuzz_set_max_settlement_in_range_accepted(
        val in MIN_MAX_BATCH_SETTLEMENT..=MAX_MAX_BATCH_SETTLEMENT
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let client = create_client(&env);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        assert!(client.set_max_settlement(&val));
        assert_eq!(client.get_max_settlement(), val);
    }

    /// Any max_milestones outside `[MIN, MAX]` is rejected.
    #[test]
    fn fuzz_set_max_milestones_out_of_range_rejected(
        bad in prop_oneof![
            Just(0u32),
            (MAX_MAX_MILESTONES + 1)..=u32::MAX,
        ]
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let client = create_client(&env);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        assert_contract_error(
            client.try_set_max_milestones(&bad),
            EscrowError::LimitOutOfRange,
        );
    }

    /// Zero and negative single-milestone amounts are rejected.
    #[test]
    fn fuzz_create_rejects_nonpositive_milestone(bad in i128::MIN..=0i128) {
        let env = Env::default();
        env.mock_all_auths();
        let client = create_client(&env);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        assert_contract_error(
            client.try_create_contract(
                &c,
                &f,
                &None,
                &vec![&env, bad],
                &ReleaseAuthorization::ClientOnly,
            ),
            EscrowError::InvalidMilestoneAmount,
        );
    }
}
