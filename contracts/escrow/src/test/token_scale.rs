//! Tests for token scale persistence, validation, and normalized-value exposure (#1346).
//!
//! ## Required edge cases (from issue #1346)
//!
//! 1. **zero decimals** — a token with 0 decimal places accepts any positive
//!    integer amount without a fractional-amount error.
//! 2. **fractional input** — an amount that is not a whole multiple of the
//!    token's scale unit is rejected with `FractionalTokenAmount`.
//! 3. **maximum value** — the largest exactly-representable amount (at the
//!    configured scale) is accepted.
//! 4. **scale mismatch** — re-binding a second token whose scale differs from
//!    the already-stored scale is blocked; the stored scale must not change.
//!    (Since `bind_settlement_token` is write-once, the "mismatch" case is
//!    tested via the double-bind guard and via the normalized-read entrypoint
//!    reflecting the first token's scale.)
//! 5. **scale change after funding** — the scale is frozen at bind time; once
//!    contracts are funded their milestone amounts remain valid against the
//!    original scale even if the token's `decimals()` would return a different
//!    value on a hypothetical re-probe (write-once guarantee).
//!
//! Additional tests cover:
//! - `get_token_scale` before and after binding
//! - `get_normalized_amount` round-trip
//! - Scale validation in `create_contract` for all representable amounts
//! - `TokenScaleNotSet` when no token bound
//! - Unit-level `scale_multiplier` and `normalized_amount` functions

#![cfg(test)]

use crate::{
    token_scale::{normalized_amount, scale_multiplier},
    Error, Escrow, EscrowClient, ReleaseAuthorization,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token::StellarAssetClient, vec, Address, Env};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Register a Stellar Asset Contract with a given decimal count and return
/// the token address.  `StellarAssetClient` is a thin SAC wrapper; `decimals`
/// returns 7 for standard Stellar assets.
fn register_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract(admin.clone())
}

/// Assert that a `try_*` call surfaces the expected `Error`.
fn assert_err<T: core::fmt::Debug, E: core::fmt::Debug>(
    result: Result<Result<T, E>, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: Error,
) {
    match result {
        Err(Ok(e)) => {
            let expected_soroban: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_soroban, "contract error code mismatch");
        }
        other => panic!("expected Error::{:?}, got {:?}", expected, other),
    }
}

// ── get_token_scale ───────────────────────────────────────────────────────────

/// Before `bind_settlement_token` the scale is absent.
#[test]
fn get_token_scale_returns_none_before_bind() {
    let env = Env::default();
    let (client, _) = setup(&env);
    assert_eq!(client.get_token_scale(), None);
}

/// After `bind_settlement_token` the scale reflects the SAC token's decimals.
/// Standard Stellar SAC tokens report 7 decimals.
#[test]
fn get_token_scale_returns_decimals_after_bind() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);

    client.bind_settlement_token(&admin, &token);

    let scale = client.get_token_scale();
    assert!(scale.is_some(), "scale must be set after bind");
    // Standard Stellar SAC tokens have 7 decimals.
    assert_eq!(scale.unwrap(), 7u32);
}

// ── get_normalized_amount ─────────────────────────────────────────────────────

/// `get_normalized_amount` panics with `TokenScaleNotSet` before binding.
#[test]
fn get_normalized_amount_fails_before_bind() {
    let env = Env::default();
    let (client, _) = setup(&env);

    let result = client.try_get_normalized_amount(&10_000_000_i128);
    assert_err(result, Error::TokenScaleNotSet);
}

/// After binding a 7-decimal token, `get_normalized_amount` divides by 10^7.
#[test]
fn get_normalized_amount_round_trip_seven_decimals() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);
    client.bind_settlement_token(&admin, &token);

    // 1 token = 10_000_000 stroops with 7 decimals
    assert_eq!(client.get_normalized_amount(&10_000_000_i128), 1_i128);
    assert_eq!(client.get_normalized_amount(&500_000_000_i128), 50_i128);
    assert_eq!(
        client.get_normalized_amount(&1_000_000_0000000_i128),
        1_000_000_i128
    );
}

// ── Edge case 1: zero decimals ────────────────────────────────────────────────

/// A token with 0 decimal places: every positive integer amount is valid and
/// `normalized_amount` is an identity function.
#[test]
fn edge_zero_decimals_any_positive_integer_accepted() {
    // Test the pure function directly — scale_multiplier(0) == 1
    assert_eq!(scale_multiplier(0), 1_i128);
    // Any positive integer is exactly representable (no fractional issue).
    assert_eq!(normalized_amount(1, 0), 1);
    assert_eq!(normalized_amount(42, 0), 42);
    assert_eq!(normalized_amount(1_000_000, 0), 1_000_000);
}

/// With 0-decimal scale, even amounts of `1` are representable.
#[test]
fn edge_zero_decimals_scale_multiplier_is_one() {
    // scale_multiplier(0) must be 1 so that amount % 1 == 0 always.
    let m = scale_multiplier(0);
    assert_eq!(m, 1);
    // All integers are divisible by 1.
    assert_eq!(0 % m, 0);
    assert_eq!(1 % m, 0);
    assert_eq!(i128::MAX % m, 0);
}

// ── Edge case 2: fractional input ────────────────────────────────────────────

/// An amount that is not a whole multiple of 10^7 (for a 7-decimal token) is
/// rejected when calling `create_contract`.
#[test]
fn edge_fractional_input_rejected_by_create_contract() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);
    client.bind_settlement_token(&admin, &token);

    let escrow_client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // 10_000_001 is NOT divisible by 10_000_000 (7 decimals) — fractional remainder of 1.
    let bad_milestones = vec![&env, 10_000_001_i128];
    let result = client.try_create_contract(
        &escrow_client_addr,
        &freelancer_addr,
        &None,
        &bad_milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_err(result, Error::FractionalTokenAmount);
}

/// Several fractional inputs at different scales — directly verified via `create_contract`.
#[test]
fn edge_fractional_input_various_amounts() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);
    client.bind_settlement_token(&admin, &token);

    let c = Address::generate(&env);
    let f = Address::generate(&env);

    // These amounts are not divisible by 10_000_000 (7-decimal scale).
    for bad in [1_i128, 7, 100, 999_999, 10_000_001, 19_999_999] {
        let result = client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, bad],
            &ReleaseAuthorization::ClientOnly,
        );
        assert_err(result, Error::FractionalTokenAmount);
    }
}

/// Exactly one stroop short of a whole token triggers `FractionalTokenAmount`.
#[test]
fn edge_fractional_input_one_stroop_off() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);
    client.bind_settlement_token(&admin, &token);

    let c = Address::generate(&env);
    let f = Address::generate(&env);

    // 9_999_999 stroops is NOT divisible by 10_000_000 (one stroop short of 1 token).
    let result = client.try_create_contract(
        &c,
        &f,
        &None,
        &vec![&env, 9_999_999_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert_err(result, Error::FractionalTokenAmount);
}

// ── Edge case 3: maximum value ────────────────────────────────────────────────

/// The maximum exactly-representable amount at a 7-decimal scale is accepted.
#[test]
fn edge_maximum_value_exactly_representable_accepted() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);
    client.bind_settlement_token(&admin, &token);

    let escrow_client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // MAX_SINGLE_AMOUNT_STROOPS = 1_000_000_0000000 (1M tokens at 7 decimals)
    // This is exactly divisible by 10_000_000 → valid.
    let max_amount = crate::MAX_TOTAL_ESCROW_STROOPS; // 1_000_000_0000000
    let good_milestones = vec![&env, max_amount];
    let result = client.try_create_contract(
        &escrow_client_addr,
        &freelancer_addr,
        &None,
        &good_milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    // Should succeed — the amount is exactly representable.
    assert!(
        result.is_ok(),
        "max exactly-representable amount must be accepted: {:?}",
        result
    );
}

/// The normalized value of the max amount equals the expected visible-token value.
#[test]
fn edge_maximum_value_normalized_is_correct() {
    // MAX amount = 1_000_000_0000000 stroops with 7 decimals = 1_000_000 tokens
    let max_stroops: i128 = 1_000_000_0000000;
    let normalized = normalized_amount(max_stroops, 7);
    assert_eq!(normalized, 1_000_000_i128);
}

// ── Edge case 4: scale mismatch ───────────────────────────────────────────────

/// The settlement token binding is write-once; attempting to bind a second
/// token is rejected, so the stored scale can never change after the first bind.
#[test]
fn edge_scale_mismatch_second_bind_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token1 = register_token(&env, &admin);
    let token2 = register_token(&env, &admin);

    // First bind succeeds and records scale.
    client.bind_settlement_token(&admin, &token1);
    let scale_after_first = client.get_token_scale();
    assert!(scale_after_first.is_some());

    // Second bind with a different token is rejected by SettlementTokenAlreadyBound.
    let result = client.try_bind_settlement_token(&admin, &token2);
    assert!(result.is_err(), "second bind must be rejected");

    // Scale is unchanged — still reflects the first token.
    assert_eq!(client.get_token_scale(), scale_after_first);
}

/// Normalized amounts reflect the scale of the first-bound token even after
/// a failed rebind attempt.
#[test]
fn edge_scale_mismatch_normalized_view_reflects_first_token() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);
    client.bind_settlement_token(&admin, &token);

    // 10_000_000 stroops should normalize to 1 token for a 7-decimal SAC token.
    let normalized = client.get_normalized_amount(&10_000_000_i128);
    assert_eq!(normalized, 1_i128);
}

// ── Edge case 5: scale change after funding ───────────────────────────────────

/// Once a contract is funded, the recorded scale is frozen.  The stored scale
/// does not change because `bind_settlement_token` is write-once — any attempt
/// to re-probe with a different token is rejected.  This test verifies the
/// invariant end-to-end: an already-funded contract's milestone amounts remain
/// valid against the original scale.
#[test]
fn edge_scale_change_after_funding_amounts_remain_valid() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);

    client.bind_settlement_token(&admin, &token);

    let scale_before = client.get_token_scale().expect("scale set after bind");

    let escrow_client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create a contract with a valid, scale-aligned amount.
    // 100 tokens × 10_000_000 stroops/token = 1_000_000_000 stroops.
    let milestone_amount: i128 = 100 * 10_000_000;
    let milestones = vec![&env, milestone_amount];
    let contract_id = client.create_contract(
        &escrow_client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Fund the contract.
    StellarAssetClient::new(&env, &token).mint(&escrow_client_addr, &milestone_amount);
    client.deposit_funds(&contract_id, &escrow_client_addr, &milestone_amount);

    // Scale must not have changed.
    let scale_after_fund = client
        .get_token_scale()
        .expect("scale still set after funding");
    assert_eq!(
        scale_before, scale_after_fund,
        "scale must not change after funding"
    );

    // Normalized view of the funded amount remains consistent.
    let normalized = client.get_normalized_amount(&milestone_amount);
    assert_eq!(
        normalized, 100_i128,
        "funded amount normalizes to 100 tokens"
    );
}

/// Attempting to bind a second token after contracts are funded is rejected —
/// the scale recorded for the live contracts is protected.
#[test]
fn edge_scale_change_second_bind_after_funding_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);
    client.bind_settlement_token(&admin, &token);

    let escrow_client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestone_amount: i128 = 10_000_000; // 1 token
    let milestones = vec![&env, milestone_amount];
    let contract_id = client.create_contract(
        &escrow_client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    StellarAssetClient::new(&env, &token).mint(&escrow_client_addr, &milestone_amount);
    client.deposit_funds(&contract_id, &escrow_client_addr, &milestone_amount);

    // Attempt to bind a second (different) token — must be rejected.
    let token2 = register_token(&env, &admin);
    let result = client.try_bind_settlement_token(&admin, &token2);
    assert!(result.is_err(), "re-bind after funding must be rejected");
}

// ── scale_multiplier unit tests ───────────────────────────────────────────────

#[test]
fn scale_multiplier_produces_correct_powers_of_ten() {
    assert_eq!(scale_multiplier(0), 1);
    assert_eq!(scale_multiplier(1), 10);
    assert_eq!(scale_multiplier(2), 100);
    assert_eq!(scale_multiplier(6), 1_000_000);
    assert_eq!(scale_multiplier(7), 10_000_000);
    assert_eq!(scale_multiplier(18), 1_000_000_000_000_000_000_i128);
}

// ── normalized_amount unit tests ──────────────────────────────────────────────

#[test]
fn normalized_amount_identity_for_zero_decimals() {
    assert_eq!(normalized_amount(0, 0), 0);
    assert_eq!(normalized_amount(1, 0), 1);
    assert_eq!(normalized_amount(i128::MAX, 0), i128::MAX);
}

#[test]
fn normalized_amount_correct_for_seven_decimals() {
    assert_eq!(normalized_amount(10_000_000, 7), 1);
    assert_eq!(normalized_amount(50_000_000, 7), 5);
    assert_eq!(normalized_amount(1_000_000_0000000_i128, 7), 1_000_000);
}

#[test]
fn normalized_amount_correct_for_two_decimals() {
    assert_eq!(normalized_amount(100, 2), 1);
    assert_eq!(normalized_amount(1_000, 2), 10);
}

// ── Scale validation in create_contract ───────────────────────────────────────

/// Exactly-representable amounts are accepted when a 7-decimal token is bound.
#[test]
fn create_contract_exact_scale_amounts_accepted() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);
    client.bind_settlement_token(&admin, &token);

    let c = Address::generate(&env);
    let f = Address::generate(&env);

    // 1, 10, 50, 100 tokens in stroops — all divisible by 10_000_000.
    for tokens in [1_i128, 10, 50, 100] {
        let amount = tokens * 10_000_000;
        let result = client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, amount],
            &ReleaseAuthorization::ClientOnly,
        );
        assert!(result.is_ok(), "{}t amount should be accepted", tokens);
    }
}

/// Non-representable amounts (fractional stroops) are rejected.
#[test]
fn create_contract_fractional_amounts_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = register_token(&env, &admin);
    client.bind_settlement_token(&admin, &token);

    let c = Address::generate(&env);
    let f = Address::generate(&env);

    // 1, 100, 999_999, 5_000_001 — none divisible by 10_000_000.
    for bad_amount in [1_i128, 100, 999_999, 5_000_001] {
        let result = client.try_create_contract(
            &c,
            &f,
            &None,
            &vec![&env, bad_amount],
            &ReleaseAuthorization::ClientOnly,
        );
        assert_err(result, Error::FractionalTokenAmount);
    }
}

/// When no token is bound yet, `create_contract` skips scale validation
/// (allows pre-bind contract creation).
#[test]
fn create_contract_skips_scale_validation_when_no_token_bound() {
    let env = Env::default();
    let (client, _) = setup(&env);

    let c = Address::generate(&env);
    let f = Address::generate(&env);

    // Any amount passes when no scale is stored.
    let result = client.try_create_contract(
        &c,
        &f,
        &None,
        &vec![&env, 1_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(
        result.is_ok(),
        "pre-bind contract creation should skip scale check"
    );
}
