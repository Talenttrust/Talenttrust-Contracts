//! Comprehensive tests for protocol fee computation and bps bounds enforcement.
//!
//! Covers:
//! - `calculate_protocol_fee`: rounding, overflow guard, zero bps, edge cases
//! - `set_protocol_fee_bps`: valid range, boundary rejection at >= 10_000, auth
//! - `get_protocol_fee_bps` / `get_accumulated_protocol_fees`: default and post-set reads
//! - Fee accrual through `release_milestone`: single and multi-milestone flows
//! - Fee cap invariant: fee never exceeds milestone amount

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{DataKey, Error, Escrow, EscrowClient, ReleaseAuthorization};

// ── Helper: stand-alone unit-test environment (no SAC, no contract) ──────────

fn bare_env() -> Env {
    Env::default()
}

// ── Helper: initialized escrow with mocked auth (no SAC) ─────────────────────

fn init_escrow(env: &Env) -> (EscrowClient<'_>, Address) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths_allowing_non_root_auth();
    client.initialize(&admin);
    (client, admin)
}

// ── Helper: full integration fixture with SAC ─────────────────────────────────

/// Returns (escrow_client, sac_token_address, admin_address).
/// Uses `mock_all_auths_allowing_non_root_auth` so SAC `transfer` works.
fn setup_with_sac(env: &Env) -> (EscrowClient<'_>, Address, Address) {
    let contract_id = env.register(Escrow, ());
    let escrow = EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    escrow.bind_settlement_token(&admin, &token);
    (escrow, token, admin)
}

// ─────────────────────────────────────────────────────────────────────────────
// Section A: unit tests for `calculate_protocol_fee` (pure arithmetic)
// ─────────────────────────────────────────────────────────────────────────────

/// Zero bps → always 0, no multiplication executed.
#[test]
fn fee_zero_bps_returns_zero() {
    let env = bare_env();
    assert_eq!(Escrow::calculate_protocol_fee(&env, 0, 0), 0);
    assert_eq!(Escrow::calculate_protocol_fee(&env, 1_000_000, 0), 0);
    assert_eq!(Escrow::calculate_protocol_fee(&env, i128::MAX / 2, 0), 0);
}

/// Exact division: 1_000_000 × 250 / 10_000 = 25_000 with no remainder.
#[test]
fn fee_250_bps_round_amount_exact() {
    let env = bare_env();
    let fee = Escrow::calculate_protocol_fee(&env, 1_000_000, 250);
    assert_eq!(fee, 25_000);
}

/// Floor rounding: 1_001 × 250 = 250_250; floor(250_250 / 10_000) = 25.
#[test]
fn fee_floor_rounds_down_on_indivisible_product() {
    let env = bare_env();
    let fee = Escrow::calculate_protocol_fee(&env, 1_001, 250);
    assert_eq!(fee, 25, "indivisible product must floor, not round up");
}

/// Sub-threshold amount: 9 × 1_000 = 9_000; floor(9_000 / 10_000) = 0.
#[test]
fn fee_sub_threshold_floors_to_zero() {
    let env = bare_env();
    assert_eq!(Escrow::calculate_protocol_fee(&env, 9, 1_000), 0);
    assert_eq!(Escrow::calculate_protocol_fee(&env, 1, 9_999), 0);
}

/// Single stroop at maximum allowed bps (9_999): floor(1 × 9_999 / 10_000) = 0.
#[test]
fn fee_one_stroop_at_max_bps_floors_to_zero() {
    let env = bare_env();
    let fee = Escrow::calculate_protocol_fee(&env, 1, 9_999);
    assert_eq!(fee, 0);
    // net payout must not go negative
    assert!(1_i128 - fee >= 0);
}

/// 10_000 stroops at 9_999 bps: floor(10_000 × 9_999 / 10_000) = 9_999.
/// fee (9_999) < amount (10_000), so net payout = 1.
#[test]
fn fee_at_max_bps_9999_is_strictly_less_than_amount() {
    let env = bare_env();
    let amount: i128 = 10_000;
    let fee = Escrow::calculate_protocol_fee(&env, amount, 9_999);
    assert_eq!(fee, 9_999);
    assert!(fee < amount, "fee must be strictly less than amount");
    assert!(amount - fee >= 1);
}

/// 100% rate at 9_999 bps for large amount rounds correctly.
#[test]
fn fee_large_amount_typical_rate_floor_rounding() {
    let env = bare_env();
    // 1_000_000_000 * 300 = 300_000_000_000; / 10_000 = 30_000_000
    let fee = Escrow::calculate_protocol_fee(&env, 1_000_000_000, 300);
    assert_eq!(fee, 30_000_000);
    assert!(fee <= 1_000_000_000);
}

/// Net payout must be non-negative for a comprehensive matrix of inputs.
#[test]
fn fee_net_payout_never_negative_matrix() {
    let env = bare_env();
    let cases: &[(i128, u32)] = &[
        (1, 1),
        (1, 5_000),
        (1, 9_999),
        (10_000, 9_999),
        (50_000, 500),
        (3_333, 1_000),
        (1_000_000_000_000_000, 1),
        (1_000_000_000_000_000, 9_999),
    ];
    for &(amount, bps) in cases {
        let fee = Escrow::calculate_protocol_fee(&env, amount, bps);
        assert!(
            fee <= amount,
            "fee ({fee}) must not exceed amount ({amount}) for bps={bps}"
        );
        assert!(amount - fee >= 0, "net payout must be non-negative");
    }
}

/// Overflow guard fires when amount × fee_bps overflows i128.
/// i128::MAX × 2 overflows.
#[test]
#[should_panic(expected = "HostError: Error(Contract, #45)")]
fn fee_overflow_guard_fires_on_i128_max_times_2() {
    let env = bare_env();
    Escrow::calculate_protocol_fee(&env, i128::MAX, 2);
}

/// Overflow guard fires for a more realistic oversized amount.
/// 10^37 × 1 still overflows because i128 max is ~1.7×10^38, but
/// 10^36 × 10_000 overflows.
#[test]
#[should_panic(expected = "HostError: Error(Contract, #45)")]
fn fee_overflow_guard_fires_on_large_amount_high_bps() {
    let env = bare_env();
    // 10^36 * 9_999 > i128::MAX
    let huge: i128 = 1_000_000_000_000_000_000_000_000_000_000_000_000_i128; // 10^36
    Escrow::calculate_protocol_fee(&env, huge, 9_999);
}

// ─────────────────────────────────────────────────────────────────────────────
// Section B: `set_protocol_fee_bps` bounds and rejection
// ─────────────────────────────────────────────────────────────────────────────

/// Freshly initialized contract has 0 bps by default.
#[test]
fn bps_default_is_zero_after_init() {
    let env = Env::default();
    let (client, _admin) = init_escrow(&env);
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// 0 bps is accepted (fee collection disabled).
#[test]
fn bps_accepts_zero() {
    let env = Env::default();
    let (client, _admin) = init_escrow(&env);
    assert!(client.set_protocol_fee_bps(&0u32));
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// Arbitrary valid value in 1..9_999 is accepted.
#[test]
fn bps_accepts_mid_range_values() {
    let env = Env::default();
    let (client, _admin) = init_escrow(&env);

    for &bps in &[1u32, 100, 500, 1_000, 5_000, 9_998, 9_999] {
        assert!(client.set_protocol_fee_bps(&bps), "should accept bps={bps}");
        assert_eq!(client.get_protocol_fee_bps(), bps);
    }
}

/// 9_999 bps is the maximum accepted value (last value below 10_000).
#[test]
fn bps_accepts_9999_boundary() {
    let env = Env::default();
    let (client, _admin) = init_escrow(&env);
    assert!(client.set_protocol_fee_bps(&9_999u32));
    assert_eq!(client.get_protocol_fee_bps(), 9_999);
}

/// 10_000 bps is rejected with InvalidProtocolParameters (code 49).
#[test]
fn bps_rejects_10000_with_typed_error() {
    let env = Env::default();
    let (client, _admin) = init_escrow(&env);

    let result = client.try_set_protocol_fee_bps(&10_000u32);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);
    // value must remain at default 0 after a rejected call
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// Values > 10_000 are also rejected with the same typed error.
#[test]
fn bps_rejects_above_10000() {
    let env = Env::default();
    let (client, _admin) = init_escrow(&env);

    for &bad in &[10_001u32, 20_000, u32::MAX] {
        let result = client.try_set_protocol_fee_bps(&bad);
        super::assert_contract_error(result, Error::InvalidProtocolParameters);
    }
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// After a failed set attempt, the previously stored value is unchanged.
#[test]
fn bps_prior_value_preserved_after_rejection() {
    let env = Env::default();
    let (client, _admin) = init_escrow(&env);

    // Set a valid value first
    assert!(client.set_protocol_fee_bps(&500u32));
    assert_eq!(client.get_protocol_fee_bps(), 500);

    // Attempt rejected update
    let result = client.try_set_protocol_fee_bps(&10_000u32);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);

    // Previous value must be intact
    assert_eq!(client.get_protocol_fee_bps(), 500);
}

/// Updating bps multiple times is idempotent: last write wins.
#[test]
fn bps_multiple_valid_updates() {
    let env = Env::default();
    let (client, _admin) = init_escrow(&env);

    client.set_protocol_fee_bps(&100u32);
    assert_eq!(client.get_protocol_fee_bps(), 100);

    client.set_protocol_fee_bps(&500u32);
    assert_eq!(client.get_protocol_fee_bps(), 500);

    client.set_protocol_fee_bps(&0u32);
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// `set_protocol_fee_bps` requires initialization; panics without it.
#[test]
#[should_panic(expected = "HostError: Error(Contract, #36)")]
fn bps_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    // no initialize() call
    client.set_protocol_fee_bps(&100u32);
}

// ─────────────────────────────────────────────────────────────────────────────
// Section C: fee accrual through `release_milestone`
// ─────────────────────────────────────────────────────────────────────────────

/// No fees accumulate when bps = 0 (default).
#[test]
fn accrual_zero_bps_no_fees_accumulate() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_sac(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 1_000_i128];

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&client_addr, &1_000);
    escrow.deposit_funds(&id, &client_addr, &1_000_i128);
    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);

    assert_eq!(escrow.get_accumulated_protocol_fees(), 0);
}

/// Single milestone release at 1_000 bps (10%) accrues correct fee.
/// floor(1_000 * 1_000 / 10_000) = 100.
#[test]
fn accrual_single_milestone_10_percent() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_sac(&env);

    escrow.set_protocol_fee_bps(&1_000u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 1_000_i128];

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&client_addr, &1_000);
    escrow.deposit_funds(&id, &client_addr, &1_000_i128);
    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);

    assert_eq!(escrow.get_accumulated_protocol_fees(), 100);
}

/// Multi-milestone release accumulates fees correctly, including floor rounding.
/// Milestones: 1_000, 2_500, 3_333 at 1_000 bps.
/// Fees: floor(100) + floor(250) + floor(333.3) = 100 + 250 + 333 = 683.
#[test]
fn accrual_multi_milestone_floor_rounding_sums_correctly() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_sac(&env);

    escrow.set_protocol_fee_bps(&1_000u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 1_000_i128, 2_500_i128, 3_333_i128];
    let total: i128 = 6_833;

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&client_addr, &total);
    escrow.deposit_funds(&id, &client_addr, &total);

    // Milestone 0: fee = 100
    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);
    assert_eq!(escrow.get_accumulated_protocol_fees(), 100);

    // Milestone 1: fee = 250
    escrow.approve_milestone_release(&id, &client_addr, &1);
    escrow.release_milestone(&id, &client_addr, &1);
    assert_eq!(escrow.get_accumulated_protocol_fees(), 350);

    // Milestone 2: fee = floor(3_333 * 1_000 / 10_000) = floor(333.3) = 333
    escrow.approve_milestone_release(&id, &client_addr, &2);
    escrow.release_milestone(&id, &client_addr, &2);
    assert_eq!(escrow.get_accumulated_protocol_fees(), 683);
}

/// Fee is strictly less than milestone amount for max allowed bps (9_999).
#[test]
fn accrual_fee_strictly_less_than_amount_at_max_bps() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_sac(&env);

    escrow.set_protocol_fee_bps(&9_999u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // 10_000 stroops: fee = floor(10_000 * 9_999 / 10_000) = 9_999 < 10_000
    let amount: i128 = 10_000;
    let milestones = soroban_sdk::vec![&env, amount];

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&client_addr, &amount);
    escrow.deposit_funds(&id, &client_addr, &amount);
    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);

    let accumulated = escrow.get_accumulated_protocol_fees();
    assert_eq!(accumulated, 9_999);
    assert!(
        accumulated < amount,
        "fee must be strictly less than amount"
    );
}

/// Accumulated fees are 0 before any releases even when bps is set.
#[test]
fn accrual_no_fees_before_any_release() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_sac(&env);

    escrow.set_protocol_fee_bps(&500u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 1_000_i128];

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&client_addr, &1_000);
    escrow.deposit_funds(&id, &client_addr, &1_000_i128);

    // No release yet
    assert_eq!(escrow.get_accumulated_protocol_fees(), 0);
}

/// Changing bps between milestone releases: each release uses the current bps at release time.
#[test]
fn accrual_bps_change_between_releases_uses_current_rate() {
    let env = Env::default();
    let (escrow, token, _admin) = setup_with_sac(&env);

    // Start at 500 bps (5%)
    escrow.set_protocol_fee_bps(&500u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 1_000_i128, 1_000_i128];

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&client_addr, &2_000);
    escrow.deposit_funds(&id, &client_addr, &2_000_i128);

    // Release 0 at 500 bps: fee = floor(1_000 * 500 / 10_000) = 50
    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);
    assert_eq!(escrow.get_accumulated_protocol_fees(), 50);

    // Change to 1_000 bps (10%)
    escrow.set_protocol_fee_bps(&1_000u32);

    // Release 1 at 1_000 bps: fee = floor(1_000 * 1_000 / 10_000) = 100
    escrow.approve_milestone_release(&id, &client_addr, &1);
    escrow.release_milestone(&id, &client_addr, &1);
    assert_eq!(escrow.get_accumulated_protocol_fees(), 150);
}

// ─────────────────────────────────────────────────────────────────────────────
// Section D: edge cases — zero amount, boundary bps values, reader idempotency
// ─────────────────────────────────────────────────────────────────────────────

/// fee(amount=0, any_bps) = 0.
#[test]
fn fee_zero_amount_any_bps_is_zero() {
    let env = bare_env();
    for &bps in &[0u32, 1, 500, 9_999] {
        assert_eq!(
            Escrow::calculate_protocol_fee(&env, 0, bps),
            0,
            "zero amount must always yield zero fee"
        );
    }
}

/// At exactly 9_999 bps, fee < amount for any amount >= 1.
#[test]
fn fee_9999_bps_always_less_than_amount() {
    let env = bare_env();
    for &amount in &[
        1_i128,
        10,
        100,
        10_000,
        1_000_000,
        1_000_000_000_000_000_i128,
    ] {
        let fee = Escrow::calculate_protocol_fee(&env, amount, 9_999);
        assert!(
            fee < amount,
            "fee ({fee}) must be < amount ({amount}) at 9_999 bps"
        );
    }
}

/// At 1 bps, even large amounts produce a fee that is a tiny fraction of the amount.
#[test]
fn fee_1_bps_tiny_fraction() {
    let env = bare_env();
    // 1_000_000 * 1 / 10_000 = 100
    assert_eq!(Escrow::calculate_protocol_fee(&env, 1_000_000, 1), 100);
    // 9_999 * 1 / 10_000 = 0 (floors to zero)
    assert_eq!(Escrow::calculate_protocol_fee(&env, 9_999, 1), 0);
}

/// `get_accumulated_protocol_fees` returns 0 before any releases.
#[test]
fn reader_accumulated_fees_zero_before_releases() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

/// `get_protocol_fee_bps` returns 0 when no value has been stored.
#[test]
fn reader_fee_bps_zero_when_unset() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// Both readers are idempotent: calling them multiple times returns the same result.
#[test]
fn reader_calls_are_idempotent() {
    let env = Env::default();
    let (client, _admin) = init_escrow(&env);

    client.set_protocol_fee_bps(&250u32);
    env.as_contract(&env.register(Escrow, ()), || {});

    for _ in 0..5 {
        assert_eq!(client.get_protocol_fee_bps(), 250);
    }
}

/// Directly writing to storage and reading back works (no off-by-one in DataKey).
#[test]
fn reader_reflects_direct_storage_write() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBps, &777u32);
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &12_345_i128);
    });

    assert_eq!(client.get_protocol_fee_bps(), 777);
    assert_eq!(client.get_accumulated_protocol_fees(), 12_345);
}
