#![cfg(test)]

use crate::test::{assert_contract_error, create_client, default_milestones};
use crate::{Contract, ContractStatus, EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

// ── Effective defaults (read before any setter is called) ─────────────────────

#[test]
fn effective_max_milestones_returns_default_before_set() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    assert_eq!(client.get_max_milestones(), crate::MAX_MILESTONES);
}

#[test]
fn effective_max_escrow_stroops_returns_default_before_set() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    assert_eq!(
        client.get_max_escrow_stroops(),
        crate::MAX_TOTAL_ESCROW_STROOPS
    );
}

// ── set_max_milestones / get_max_milestones ───────────────────────────────────

#[test]
fn set_max_milestones_persists_value() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    client.set_max_milestones(&25);
    assert_eq!(client.get_max_milestones(), 25);
}

#[test]
fn set_max_milestones_can_set_minimum() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    client.set_max_milestones(&crate::MIN_MAX_MILESTONES);
    assert_eq!(client.get_max_milestones(), crate::MIN_MAX_MILESTONES);
}

#[test]
fn set_max_milestones_can_set_maximum() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    client.set_max_milestones(&crate::MAX_MAX_MILESTONES);
    assert_eq!(client.get_max_milestones(), crate::MAX_MAX_MILESTONES);
}

#[test]
fn set_max_milestones_below_minimum_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_set_max_milestones(&(crate::MIN_MAX_MILESTONES - 1)),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_milestones_above_maximum_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_set_max_milestones(&(crate::MAX_MAX_MILESTONES + 1)),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_milestones_before_init_panics() {
    let env = Env::default();
    let client = create_client(&env);

    assert_contract_error(
        client.try_set_max_milestones(&10),
        crate::Error::NotInitialized,
    );
}

#[test]
fn set_max_milestones_can_overwrite() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    client.set_max_milestones(&15);
    assert_eq!(client.get_max_milestones(), 15);
    client.set_max_milestones(&5);
    assert_eq!(client.get_max_milestones(), 5);
}

#[test]
fn set_max_milestones_returns_true() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert!(client.set_max_milestones(&20));
}

// ── set_max_escrow_stroops / get_max_escrow_stroops ───────────────────────────

#[test]
fn set_max_escrow_stroops_persists_value() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    let new_val: i128 = 500_000_000_000_000;
    client.set_max_escrow_stroops(&new_val);
    assert_eq!(client.get_max_escrow_stroops(), new_val);
}

#[test]
fn set_max_escrow_stroops_can_set_minimum() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    client.set_max_escrow_stroops(&crate::MIN_MAX_ESCROW_STROOPS);
    assert_eq!(
        client.get_max_escrow_stroops(),
        crate::MIN_MAX_ESCROW_STROOPS
    );
}

#[test]
fn set_max_escrow_stroops_can_set_mainnet_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    client.set_max_escrow_stroops(&crate::MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS);
    assert_eq!(
        client.get_max_escrow_stroops(),
        crate::MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
    );
}

#[test]
fn set_max_escrow_stroops_below_minimum_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_set_max_escrow_stroops(&(crate::MIN_MAX_ESCROW_STROOPS - 1)),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_escrow_stroops_above_mainnet_cap_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_set_max_escrow_stroops(
            &(crate::MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS + 1),
        ),
        EscrowError::LimitOutOfRange,
    );
}

#[test]
fn set_max_escrow_stroops_before_init_panics() {
    let env = Env::default();
    let client = create_client(&env);

    assert_contract_error(
        client.try_set_max_escrow_stroops(&1_000_000_000_000),
        crate::Error::NotInitialized,
    );
}

#[test]
fn set_max_escrow_stroops_can_overwrite() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    client.set_max_escrow_stroops(&2_000_000_000_000);
    assert_eq!(client.get_max_escrow_stroops(), 2_000_000_000_000);
    client.set_max_escrow_stroops(&1_000_000_000_000);
    assert_eq!(client.get_max_escrow_stroops(), 1_000_000_000_000);
}

#[test]
fn set_max_escrow_stroops_returns_true() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert!(client.set_max_escrow_stroops(&3_000_000_000_000));
}

// ── contract_exists ───────────────────────────────────────────────────────────

#[test]
fn contract_exists_for_existing_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.contract_exists(&id));
}

#[test]
fn contract_exists_for_nonexistent_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert!(!client.contract_exists(&999));
}

#[test]
fn contract_exists_zero_id() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert!(!client.contract_exists(&0));
}

// ── get_bounds ────────────────────────────────────────────────────────────────

#[test]
fn get_bounds_returns_expected_values() {
    let env = Env::default();
    let client = create_client(&env);
    let bounds = client.get_bounds();
    assert_eq!(bounds.max_milestones, crate::MAX_MILESTONES);
    assert_eq!(
        bounds.max_single_milestone_stroops,
        crate::MAX_SINGLE_AMOUNT_STROOPS
    );
    assert_eq!(
        bounds.max_total_escrow_stroops,
        crate::MAX_TOTAL_ESCROW_STROOPS
    );
    assert_eq!(bounds.max_fee_bps, 10_000);
}

#[test]
fn get_bounds_works_before_initialization() {
    let env = Env::default();
    let client = create_client(&env);
    let bounds = client.get_bounds();
    assert!(bounds.max_milestones > 0);
    assert!(bounds.max_total_escrow_stroops > 0);
}

#[test]
fn get_bounds_is_idempotent() {
    let env = Env::default();
    let client = create_client(&env);
    let first = client.get_bounds();
    let second = client.get_bounds();
    assert_eq!(first, second);
}

// ── get_contract ──────────────────────────────────────────────────────────────

#[test]
fn get_contract_returns_created_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let c: Contract = client.get_contract(&id);
    assert_eq!(c.client, client_addr);
    assert_eq!(c.freelancer, freelancer_addr);
    assert_eq!(c.status, ContractStatus::Created);
}

#[test]
fn get_contract_panics_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_get_contract(&999),
        crate::Error::ContractNotFound,
    );
}

// ── get_next_contract_id ─────────────────────────────────────────────────────

#[test]
fn get_next_contract_id_starts_at_one() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    assert_eq!(client.get_next_contract_id(), 1);
}

#[test]
fn get_next_contract_id_increments() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(client.get_next_contract_id(), 2);

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(client.get_next_contract_id(), 3);
}

// ── get_contract_summary ─────────────────────────────────────────────────────

#[test]
fn get_contract_summary_returns_full_summary() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let summary = client.get_contract_summary(&id);
    assert_eq!(
        summary.schema_version,
        crate::CONTRACT_SUMMARY_SCHEMA_VERSION
    );
    assert_eq!(summary.client, client_addr);
    assert_eq!(summary.freelancer, freelancer_addr);
    assert_eq!(summary.status, ContractStatus::Created);
    assert_eq!(summary.milestones.len(), 3);
    assert_eq!(summary.funded_amount, 0);
    assert_eq!(summary.released_amount, 0);
}

#[test]
fn get_contract_summary_panics_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_get_contract_summary(&999),
        EscrowError::ContractNotFound,
    );
}

// ── get_milestones ────────────────────────────────────────────────────────────

#[test]
fn get_milestones_returns_all_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 100_000_000, 200_000_000];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.get_milestones(&id);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get_unchecked(0).amount, 100_000_000);
    assert_eq!(result.get_unchecked(1).amount, 200_000_000);
}

#[test]
fn get_milestones_panics_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_get_milestones(&999),
        EscrowError::ContractNotFound,
    );
}

// ── get_milestone ─────────────────────────────────────────────────────────────

#[test]
fn get_milestone_returns_some_for_valid_index() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let m = client.get_milestone(&id, &0);
    assert!(m.is_some());
    assert_eq!(m.unwrap().amount, crate::test::MILESTONE_ONE);
}

#[test]
fn get_milestone_returns_none_for_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.get_milestone(&id, &100).is_none());
}

#[test]
fn get_milestone_panics_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_get_milestone(&999, &0),
        EscrowError::ContractNotFound,
    );
}

// ── get_refundable_balance ────────────────────────────────────────────────────

#[test]
fn get_refundable_balance_panics_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_get_refundable_balance(&999),
        EscrowError::ContractNotFound,
    );
}

#[test]
fn get_refundable_balance_is_zero_before_deposit() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert_eq!(client.get_refundable_balance(&id), 0);
}

// ── is_milestone_overdue ──────────────────────────────────────────────────────

#[test]
fn is_milestone_overdue_false_for_unknown_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = create_client(&env);

    assert!(!client.is_milestone_overdue(&999, &0));
}

#[test]
fn is_milestone_overdue_false_for_no_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(!client.is_milestone_overdue(&id, &0));
}

#[test]
fn is_milestone_overdue_false_for_out_of_bounds_index() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(!client.is_milestone_overdue(&id, &100));
}

// ── get_mainnet_readiness_info ────────────────────────────────────────────────

#[test]
fn get_mainnet_readiness_info_fresh_defaults() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    let info = client.get_mainnet_readiness_info();
    assert!(info.initialized);
    assert!(!info.governed_params_set);
    assert!(!info.emergency_controls_enabled);
    assert!(info.caps_set);
    assert_eq!(info.protocol_version, crate::MAINNET_PROTOCOL_VERSION);
    assert_eq!(
        info.max_escrow_total_stroops,
        crate::MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
    );
}

#[test]
fn get_mainnet_readiness_info_before_init() {
    let env = Env::default();
    let client = create_client(&env);

    let info = client.get_mainnet_readiness_info();
    assert!(!info.initialized);
    assert!(!info.governed_params_set);
}

// ── set_arbiter ───────────────────────────────────────────────────────────────

#[test]
fn set_arbiter_updates_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    client.set_arbiter(&id, &admin, &Some(arbiter_addr.clone()));
    let c: Contract = client.get_contract(&id);
    assert_eq!(c.arbiter, Some(arbiter_addr));
}

#[test]
fn set_arbiter_remove_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    client.set_arbiter(&id, &admin, &None);
    let c: Contract = client.get_contract(&id);
    assert_eq!(c.arbiter, None);
}

#[test]
fn set_arbiter_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let result = client.try_set_arbiter(&id, &non_admin, &None);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn set_arbiter_same_as_client_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert_contract_error(
        client.try_set_arbiter(&id, &admin, &Some(client_addr)),
        EscrowError::InvalidArbiter,
    );
}

#[test]
fn set_arbiter_same_as_freelancer_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert_contract_error(
        client.try_set_arbiter(&id, &admin, &Some(freelancer_addr)),
        EscrowError::InvalidArbiter,
    );
}

#[test]
fn set_arbiter_not_found_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_set_arbiter(&999, &admin, &None),
        EscrowError::ContractNotFound,
    );
}

// ── validate_contract_id_bounds (indirect via set_arbiter) ────────────────────

#[test]
fn validate_contract_id_bounds_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = create_client(&env);
    client.initialize(&admin);

    assert_contract_error(
        client.try_set_arbiter(&0, &admin, &None),
        EscrowError::InvalidContractId,
    );
}

// ── Constants consistency ─────────────────────────────────────────────────────

#[test]
fn max_milestones_alias_matches_default() {
    assert_eq!(crate::MAX_MILESTONES, crate::DEFAULT_MAX_MILESTONES);
}

#[test]
fn max_total_escrow_stroops_alias_matches_default() {
    assert_eq!(
        crate::MAX_TOTAL_ESCROW_STROOPS,
        crate::DEFAULT_MAX_TOTAL_ESCROW_STROOPS
    );
}

#[test]
fn mainnet_caps_are_positive() {
    assert!(crate::MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0);
    assert!(crate::MAINNET_PROTOCOL_VERSION > 0);
}

#[test]
fn min_max_bounds_are_consistent() {
    assert!(crate::MIN_MAX_MILESTONES <= crate::MAX_MAX_MILESTONES);
    assert!(crate::MIN_MAX_ESCROW_STROOPS <= crate::DEFAULT_MAX_TOTAL_ESCROW_STROOPS);
    assert!(crate::MIN_MAX_ESCROW_STROOPS <= crate::MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS);
}
