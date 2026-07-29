#![cfg(test)]

//! Overflow and saturation coverage for the events-arithmetic guard rails.
//!
//! `available_balance`, `safe_add_amounts`, and `safe_subtract_amounts`
//! (see `amount_validation.rs`) back every value published on `refunded`,
//! `released`, and `resolved` events. These unit tests exercise them
//! directly at i128 extremes: production entrypoints cannot reach these
//! extremes themselves because `MAX_SINGLE_AMOUNT_STROOPS` /
//! `MAX_TOTAL_ESCROW_STROOPS` already reject any single amount or milestone
//! sum anywhere near i128::MAX before it reaches this arithmetic.

use crate::amount_validation::{available_balance, safe_add_amounts, safe_subtract_amounts};

// --- safe_add_amounts: i128 extremes ---

#[test]
fn add_amounts_at_max_boundary_succeeds() {
    assert_eq!(safe_add_amounts(i128::MAX - 1, 1), Some(i128::MAX));
}

#[test]
fn add_amounts_one_past_max_overflows_to_none() {
    assert_eq!(safe_add_amounts(i128::MAX, 1), None);
}

#[test]
fn add_amounts_sum_near_max_does_not_wrap() {
    // Two large-but-valid-looking amounts whose naive `+` would wrap i128.
    let a = i128::MAX - 10;
    let b = 20;
    assert_eq!(
        safe_add_amounts(a, b),
        None,
        "checked_add must reject, never wrap"
    );
}

#[test]
fn add_amounts_zero_identity() {
    assert_eq!(safe_add_amounts(i128::MAX, 0), Some(i128::MAX));
    assert_eq!(safe_add_amounts(0, 0), Some(0));
}

// --- safe_subtract_amounts: i128 extremes, near-zero ---

#[test]
fn subtract_amounts_at_min_boundary_succeeds() {
    assert_eq!(safe_subtract_amounts(i128::MIN + 1, 1), Some(i128::MIN));
}

#[test]
fn subtract_amounts_one_past_min_underflows_to_none() {
    assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
}

#[test]
fn subtract_amounts_near_zero_exact() {
    assert_eq!(safe_subtract_amounts(5, 5), Some(0));
}

#[test]
fn subtract_amounts_near_zero_would_go_negative_still_succeeds_i128() {
    // i128 subtraction below zero is valid (signed type) as long as it
    // doesn't cross i128::MIN; only true underflow past MIN returns None.
    assert_eq!(safe_subtract_amounts(0, 5), Some(-5));
}

// --- available_balance: the direct events-arithmetic guard ---

#[test]
fn available_balance_normal_case() {
    assert_eq!(available_balance(1_000, 300, 200), Some(500));
}

#[test]
fn available_balance_exact_zero_at_full_drawdown() {
    assert_eq!(available_balance(1_000, 600, 400), Some(0));
}

#[test]
fn available_balance_extreme_funded_no_drawdown() {
    assert_eq!(available_balance(i128::MAX, 0, 0), Some(i128::MAX));
}

#[test]
fn available_balance_first_subtraction_underflow_is_none() {
    // funded - released underflows past i128::MIN on its own.
    assert_eq!(available_balance(i128::MIN, 1, 0), None);
}

#[test]
fn available_balance_second_subtraction_underflow_is_none() {
    // funded - released succeeds, but the result minus refunded underflows.
    assert_eq!(available_balance(i128::MIN + 1, 0, 2), None);
}

#[test]
fn available_balance_inconsistent_state_goes_negative_not_none() {
    // released + refunded exceeding funded produces a valid negative i128
    // (an accounting-invariant bug for callers to catch), not an overflow;
    // only a true i128::MIN crossing should surface as None.
    assert_eq!(available_balance(10, 8, 8), Some(-6));
}
