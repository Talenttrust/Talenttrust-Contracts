#![cfg(test)]

use crate::{Escrow, EscrowClient};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

// ── Unit tests for calculate_protocol_fee floor-division rounding ─────────

/// Verifies that `fee_bps == 0` returns `0` immediately, bypassing multiplication.
#[test]
fn test_calculate_protocol_fee_zero_bps_returns_zero() {
    let env = Env::default();
    let fee = Escrow::calculate_protocol_fee(&env, 1_000_000, 0);
    assert_eq!(fee, 0, "zero fee_bps must return 0 without multiplication");
}

/// Verifies exact floor-division: 250 bps of 1_000_000 == 25_000.
#[test]
fn test_calculate_protocol_fee_250_bps_of_round_amount() {
    let env = Env::default();
    // 1_000_000 * 250 / 10_000 = 25_000 exactly
    let fee = Escrow::calculate_protocol_fee(&env, 1_000_000, 250);
    assert_eq!(fee, 25_000);
    // Net payout must never be negative
    assert!(1_000_000 - fee >= 0);
}

/// Verifies floor rounding: an indivisible product rounds DOWN, never up.
///
/// 1_001 * 250 = 250_250; 250_250 / 10_000 = 25 remainder 250 → floor == 25.
#[test]
fn test_calculate_protocol_fee_floor_rounds_down_on_indivisible_product() {
    let env = Env::default();
    let fee = Escrow::calculate_protocol_fee(&env, 1_001, 250);
    assert_eq!(fee, 25, "indivisible product must round down (floor division)");
    assert!(1_001 - fee >= 0);
}

/// Verifies that a sub-threshold amount produces a zero fee (amount * bps < 10_000).
///
/// 9 * 1_000 = 9_000; 9_000 / 10_000 = 0 (floors to zero).
#[test]
fn test_calculate_protocol_fee_sub_threshold_amount_rounds_to_zero() {
    let env = Env::default();
    let fee = Escrow::calculate_protocol_fee(&env, 9, 1_000);
    assert_eq!(fee, 0, "sub-threshold amount must yield zero fee");
}

/// Verifies that the overflow guard panics with `PotentialOverflow` (error #28)
/// when `amount * fee_bps` would overflow `i128`.
#[test]
#[should_panic(expected = "HostError: Error(Contract, #28)")]
fn test_calculate_protocol_fee_overflow_guard_fires() {
    let env = Env::default();
    // i128::MAX * 1 already cannot be multiplied by any fee_bps > 1 safely;
    // using i128::MAX with fee_bps = 2 guarantees overflow.
    Escrow::calculate_protocol_fee(&env, i128::MAX, 2);
}

/// Verifies that the net payout (gross − fee) is never negative for a range of
/// representative valid inputs.
#[test]
fn test_net_payout_never_negative_for_valid_inputs() {
    let env = Env::default();
    let cases: &[(i128, u32)] = &[
        (1, 10_000),       // maximum fee rate, minimal amount
        (10_000, 10_000),  // 100% fee rate
        (50_000, 500),     // 5% fee rate
        (3_333, 1_000),    // 10% fee rate, indivisible
        (1, 1),            // near-zero fee
    ];
    for &(amount, bps) in cases {
        let fee = Escrow::calculate_protocol_fee(&env, amount, bps);
        assert!(
            fee <= amount,
            "fee ({fee}) must not exceed gross amount ({amount}) for bps={bps}"
        );
        assert!(amount - fee >= 0, "net payout must be non-negative");
    }
}

fn create_token_contract(e: &Env, admin: &Address) -> Address {
    e.register_stellar_asset_contract_v2(admin.clone())
        .address()
}

#[test]
fn test_fee_accrual_and_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = create_token_contract(&env, &token_admin);
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);

    // Initialize with 1000 bps (10%)
    client.initialize(&admin, &1000u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Milestones: 1000, 2500, 3333
    let milestones = vec![&env, 1000_i128, 2500_i128, 3333_i128];

    // Note: create_contract has different arguments depending on the current iteration of the code.
    // Based on lib.rs line 145: pub fn create_contract(env: Env, client: Address, freelancer: Address, arbiter: Option<Address>, milestones: Vec<i128>, terms_hash: Option<Bytes>, grace_period_seconds: Option<u64>)
    // Wait, let's use the actual create_contract signature from lib.rs.
    // Looking at lib.rs, create_contract in test.rs uses:
    // client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &None,
        &None,
    );

    client.deposit_funds(&id, &6833_i128); // 1000 + 2500 + 3333 = 6833

    // Release milestone 0 (1000)
    // Fee: (1000 * 1000 + 9999) / 10000 = (1000000 + 9999) / 10000 = 1009999 / 10000 = 100
    assert!(client.release_milestone(&id, &0));

    // Release milestone 1 (2500)
    // Fee: (2500 * 1000 + 9999) / 10000 = (2500000 + 9999) / 10000 = 2509999 / 10000 = 250
    assert!(client.release_milestone(&id, &1));

    // Release milestone 2 (3333)
    // Fee: (3333 * 1000 + 9999) / 10000 = (3333000 + 9999) / 10000 = 3342999 / 10000 = 334
    assert!(client.release_milestone(&id, &2));

    // Total accumulated fees: 100 + 250 + 334 = 684

    // Mint tokens to the contract so it has funds to transfer out
    token_admin_client.mint(&contract_id, &684);

    let destination = Address::generate(&env);

    // Admin withdraws protocol fees
    let success = client.withdraw_protocol_fees(&admin, &destination, &684_i128, &token);
    assert!(success);

    assert_eq!(token_client.balance(&destination), 684);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #6)")] // UnauthorizedRole
fn test_unauthorized_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &1000u32);

    let fake_admin = Address::generate(&env);
    let destination = Address::generate(&env);
    let token = Address::generate(&env);

    // This should panic
    client.withdraw_protocol_fees(&fake_admin, &destination, &100_i128, &token);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #13)")] // InsufficientAccumulatedFees
fn test_over_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &1000u32);

    let destination = Address::generate(&env);
    let token = Address::generate(&env);

    // Withdraw more than 0
    client.withdraw_protocol_fees(&admin, &destination, &100_i128, &token);
}
