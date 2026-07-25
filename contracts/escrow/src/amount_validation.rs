//! Amount validation and sanitization module
//!
//! Provides centralized validation for all money-like values in the escrow contract.
//! Ensures positivity, max bounds, and proper stroop precision handling.
//!
//! Storage ownership: none. This module is deliberately stateless; callers use
//! these helpers before writing validated values to contract and milestone
//! storage.

/// Maximum number of decimal places for stroop precision (7 decimal places for Stellar)
pub const STROOP_PRECISION: u8 = 7;

/// Maximum individual amount allowed per operation to prevent overflow
pub const MAX_SINGLE_AMOUNT_STROOPS: i128 = 1_000_000_0000000; // 1M tokens

/// Minimum positive amount (1 stroop)
pub const MIN_POSITIVE_AMOUNT: i128 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum AmountValidationError {
    NonPositiveAmount,
    AmountExceedsMaximum,
    ExceedsContractMaximum,
}

/// Validates a single amount for positivity and bounds
///
/// # Arguments
/// * `amount` - The amount to validate (in stroops)
///
/// # Returns
/// `Ok(())` if valid, `Err(AmountValidationError)` if invalid
pub fn validate_single_amount(amount: i128) -> Result<(), crate::EscrowError> {
    // Check positivity
    if amount <= MIN_POSITIVE_AMOUNT - 1 {
        return Err(crate::EscrowError::AmountMustBePositive);
    }

    // Check maximum bounds
    if amount > MAX_SINGLE_AMOUNT_STROOPS {
        // Map large amounts to generic invalid milestone amount
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }

    // Check stroop precision (must be integer, which i128 already guarantees)
    // In Stellar, stroop is the smallest unit, so any integer is valid
    // This check is more for documentation and future-proofing

    Ok(())
}

/// Validates an amount array/vector for positivity and bounds
///
/// # Arguments
/// * `amounts` - Slice of amounts to validate (in stroops)
///
/// # Returns
/// `Ok(total)` with sum of all amounts if valid, `Err(AmountValidationError)` if invalid
pub fn validate_amount_array(amounts: &[i128]) -> Result<i128, crate::EscrowError> {
    let mut total: i128 = 0;

    for &amount in amounts.iter() {
        // Validate individual amount
        validate_single_amount(amount)?;

        // Check for potential overflow in addition
        if let Some(new_total) = total.checked_add(amount) {
            total = new_total;
        } else {
            return Err(crate::EscrowError::PotentialOverflow);
        }
    }

    Ok(total)
}

/// Validates total amount against contract maximum
///
/// # Arguments
/// * `total_amount` - The total amount to validate
/// * `max_contract_total` - Maximum allowed per contract (in stroops)
///
/// # Returns
/// `Ok(())` if valid, `Err(AmountValidationError)` if invalid
pub fn validate_contract_total(
    total_amount: i128,
    max_contract_total: i128,
) -> Result<(), crate::EscrowError> {
    if total_amount > max_contract_total {
        // Map to InvalidMilestoneAmount for contract total overflow
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }
    Ok(())
}

/// Comprehensive validation for milestone amounts
///
/// # Arguments
/// * `milestone_amounts` - Array of milestone amounts (in stroops)
/// * `max_contract_total` - Maximum allowed per contract (in stroops)
///
/// # Returns
/// `Ok(total)` with sum of all milestones if valid, `Err(AmountValidationError)` if invalid
pub fn validate_milestone_amounts(
    milestone_amounts: &[i128],
    max_contract_total: i128,
) -> Result<i128, crate::EscrowError> {
    // Validate each milestone amount and calculate total
    let total = validate_amount_array(milestone_amounts)?;

    // Validate total against contract maximum
    validate_contract_total(total, max_contract_total)?;

    Ok(total)
}

/// Validates deposit amount against remaining contract capacity
///
/// This function is critical for preventing stuck or overfunded escrows. It validates:
/// 1. The deposit amount itself is positive and within bounds
/// 2. Adding the deposit to current_deposited won't overflow
/// 3. The resulting total won't exceed the contract's maximum capacity
///
/// # Decision Boundaries
///
/// This function operates at three critical boundaries:
/// - **Exactly-remaining**: `deposit + current == max_total` → Success
/// - **One stroop short**: `deposit + current == max_total - 1` → Success
/// - **One stroop over**: `deposit + current == max_total + 1` → Failure (`InvalidMilestoneAmount`)
///
/// # Arguments
/// * `deposit_amount` - Amount to deposit (in stroops, must be positive)
/// * `current_deposited` - Current total deposited amount (in stroops)
/// * `max_contract_total` - Maximum allowed per contract (in stroops)
///
/// # Returns
/// * `Ok(())` - Deposit is valid and won't exceed capacity
/// * `Err(EscrowError::AmountMustBePositive)` - Deposit amount is ≤ 0
/// * `Err(EscrowError::InvalidMilestoneAmount)` - Deposit would exceed capacity or single amount is too large
/// * `Err(EscrowError::PotentialOverflow)` - Adding deposit to current would overflow i128
///
/// # Examples
///
/// ```ignore
/// // Valid: deposit exactly fills remaining capacity
/// assert!(validate_deposit_amount(500, 500, 1000).is_ok());
///
/// // Invalid: deposit exceeds remaining by 1 stroop
/// assert_eq!(
///     validate_deposit_amount(501, 500, 1000),
///     Err(EscrowError::InvalidMilestoneAmount)
/// );
///
/// // Invalid: contract already fully funded
/// assert_eq!(
///     validate_deposit_amount(1, 1000, 1000),
///     Err(EscrowError::InvalidMilestoneAmount)
/// );
/// ```
///
/// # Security
///
/// - Uses checked arithmetic to prevent integer overflow panics
/// - Rejects any deposit when contract is already fully funded
/// - Validates deposit amount bounds before checking capacity
pub fn validate_deposit_amount(
    deposit_amount: i128,
    current_deposited: i128,
    max_contract_total: i128,
) -> Result<(), crate::EscrowError> {
    // Validate deposit amount itself
    validate_single_amount(deposit_amount)?;

    // Check if deposit would exceed contract maximum
    if let Some(new_total) = current_deposited.checked_add(deposit_amount) {
        if new_total > max_contract_total {
            return Err(crate::EscrowError::InvalidMilestoneAmount);
        }
    } else {
        return Err(crate::EscrowError::PotentialOverflow);
    }

    Ok(())
}

/// Utility function to safely add amounts with overflow protection
///
/// # Arguments
/// * `a` - First amount
/// * `b` - Second amount
///
/// # Returns
/// `Some(sum)` if addition succeeds, `None` if overflow would occur
pub fn safe_add_amounts(a: i128, b: i128) -> Option<i128> {
    a.checked_add(b)
}

/// Utility function to safely subtract amounts with underflow protection
///
/// # Arguments
/// * `a` - Minuend
/// * `b` - Subtrahend
///
/// # Returns
/// `Some(difference)` if subtraction succeeds, `None` if underflow would occur
pub fn safe_subtract_amounts(a: i128, b: i128) -> Option<i128> {
    a.checked_sub(b)
}

/// Computes the currently available (unreleased, unrefunded) balance for a
/// contract using checked arithmetic.
///
/// `available = funded_amount - released_amount - refunded_amount`
///
/// This expression is duplicated across `lib.rs`, `finalize.rs`, and
/// `dispute.rs` call sites that read contract accounting state; centralizing
/// it here ensures every reader fails closed the same way instead of each
/// site risking a silent wraparound (in a release build, where
/// `overflow-checks` is off) or an inconsistent panic message.
///
/// # Errors
/// `AccountingInvariantViolated` if either checked subtraction underflows, or
/// if the result would be negative — both signal that `released_amount +
/// refunded_amount` has already exceeded `funded_amount`, i.e. corrupted
/// accounting state rather than an ordinary overflow.
pub fn checked_available_balance(
    funded_amount: i128,
    released_amount: i128,
    refunded_amount: i128,
) -> Result<i128, crate::Error> {
    let available = funded_amount
        .checked_sub(released_amount)
        .and_then(|value| value.checked_sub(refunded_amount))
        .ok_or(crate::Error::AccountingInvariantViolated)?;
    if available < 0 {
        return Err(crate::Error::AccountingInvariantViolated);
    }
    Ok(available)
}

/// Safely accumulates amounts into a total with overflow protection.
///
/// Iterates through amounts, validating each amount for positivity and bounds,
/// and accumulating the total with checked arithmetic. Returns the total only if
/// all amounts are valid and no overflow occurs.
///
/// This function is intended for use in contexts like `deposit_funds` where an
/// unchecked `.sum()` could panic on overflow, creating a panicking code path
/// reachable by user-supplied milestone data.
///
/// # Arguments
/// * `amounts` - Iterator over amount references (typically milestone amounts)
///
/// # Returns
/// `Ok(total)` if all amounts are valid and accumulation succeeds, `Err(EscrowError)` if any validation fails
pub fn accumulate_amounts<I: IntoIterator<Item = i128>>(
    amounts: I,
) -> Result<i128, crate::EscrowError> {
    let mut total: i128 = 0;

    for amount in amounts.into_iter() {
        // Validate individual amount for positivity and bounds
        validate_single_amount(amount)?;

        // Check for potential overflow in accumulation
        if let Some(new_total) = total.checked_add(amount) {
            total = new_total;
        } else {
            return Err(crate::EscrowError::PotentialOverflow);
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_single_amount() {
        assert!(validate_single_amount(1).is_ok());
        assert!(validate_single_amount(100_0000000).is_ok());
        assert!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS).is_ok());

        assert_eq!(
            validate_single_amount(0),
            Err(crate::EscrowError::AmountMustBePositive)
        );
        assert_eq!(
            validate_single_amount(-1),
            Err(crate::EscrowError::AmountMustBePositive)
        );
        assert_eq!(
            validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS + 1),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_validate_amount_array() {
        let amounts1 = [100_0000000, 200_0000000, 300_0000000];
        assert!(validate_amount_array(&amounts1).is_ok());
        assert_eq!(validate_amount_array(&amounts1).unwrap(), 600_0000000);

        let amounts2 = [100_0000000, 0, 300_0000000];
        assert_eq!(
            validate_amount_array(&amounts2),
            Err(crate::EscrowError::AmountMustBePositive)
        );

        let amounts3 = [100_0000000, -50_0000000, 300_0000000];
        assert_eq!(
            validate_amount_array(&amounts3),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn test_validate_contract_total() {
        let max_total = 1_000_000_0000000;
        assert!(validate_contract_total(100_0000000, max_total).is_ok());
        assert!(validate_contract_total(max_total, max_total).is_ok());
        assert_eq!(
            validate_contract_total(max_total + 1, max_total),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_validate_milestone_amounts() {
        let max_contract_total = 1_000_000_0000000;
        let milestones1 = [100_0000000, 200_0000000, 300_0000000];
        assert!(validate_milestone_amounts(&milestones1, max_contract_total).is_ok());
        let milestones2 = [500_000_0000000, 600_000_0000000];
        assert_eq!(
            validate_milestone_amounts(&milestones2, max_contract_total),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_validate_deposit_amount() {
        struct TestCase {
            name: &'static str,
            deposit_amount: i128,
            current_deposited: i128,
            max_contract_total: i128,
            expected: Result<(), crate::EscrowError>,
        }

        let test_cases = [
            TestCase {
                name: "zero deposit amount should fail with AmountMustBePositive",
                deposit_amount: 0,
                current_deposited: 0,
                max_contract_total: 1000,
                expected: Err(crate::EscrowError::AmountMustBePositive),
            },
            TestCase {
                name: "negative deposit amount should fail with AmountMustBePositive",
                deposit_amount: -1,
                current_deposited: 0,
                max_contract_total: 1000,
                expected: Err(crate::EscrowError::AmountMustBePositive),
            },
            TestCase {
                name: "one stroop under remaining capacity should succeed",
                deposit_amount: 499,
                current_deposited: 500,
                max_contract_total: 1000,
                expected: Ok(()),
            },
            TestCase {
                name: "exactly remaining capacity should succeed",
                deposit_amount: 500,
                current_deposited: 500,
                max_contract_total: 1000,
                expected: Ok(()),
            },
            TestCase {
                name: "one stroop over remaining capacity should fail with InvalidMilestoneAmount",
                deposit_amount: 501,
                current_deposited: 500,
                max_contract_total: 1000,
                expected: Err(crate::EscrowError::InvalidMilestoneAmount),
            },
            TestCase {
                name: "already fully funded contract should reject any further deposit",
                deposit_amount: 1,
                current_deposited: 1000,
                max_contract_total: 1000,
                expected: Err(crate::EscrowError::InvalidMilestoneAmount),
            },
            TestCase {
                name: "deposit exceeding max single amount bound should fail",
                deposit_amount: MAX_SINGLE_AMOUNT_STROOPS + 1,
                current_deposited: 0,
                max_contract_total: MAX_SINGLE_AMOUNT_STROOPS * 2,
                expected: Err(crate::EscrowError::InvalidMilestoneAmount),
            },
            TestCase {
                name: "potential i128 overflow in addition should fail",
                deposit_amount: 1,
                current_deposited: i128::MAX,
                max_contract_total: i128::MAX,
                expected: Err(crate::EscrowError::PotentialOverflow),
            },
        ];

        for tc in test_cases {
            let result = validate_deposit_amount(
                tc.deposit_amount,
                tc.current_deposited,
                tc.max_contract_total,
            );
            assert_eq!(
                result, tc.expected,
                "Test case '{}' failed. Expected: {:?}, Got: {:?}",
                tc.name, tc.expected, result
            );
        }
    }

    #[test]
    fn test_safe_arithmetic() {
        assert_eq!(safe_add_amounts(100, 200), Some(300));
        assert_eq!(safe_add_amounts(i128::MAX, 1), None);
        assert_eq!(safe_subtract_amounts(300, 100), Some(200));
        assert_eq!(safe_subtract_amounts(0, 1), Some(-1));
        assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
    }

    // ── Overflow / saturation boundary tests ────────────────────────────────

    #[test]
    fn validate_single_amount_rejects_i128_max_exceeds_bounds() {
        assert_eq!(
            validate_single_amount(i128::MAX),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_single_amount_rejects_i128_min() {
        assert_eq!(
            validate_single_amount(i128::MIN),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn validate_single_amount_rejects_i128_min_plus_one() {
        assert_eq!(
            validate_single_amount(i128::MIN + 1),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn validate_single_amount_boundary_one() {
        assert!(validate_single_amount(1).is_ok());
    }

    #[test]
    fn validate_single_amount_just_below_max() {
        assert!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS - 1).is_ok());
    }

    #[test]
    fn validate_single_amount_exactly_at_max() {
        assert!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS).is_ok());
    }

    // ── Amount array overflow ──────────────────────────────────────────────

    #[test]
    fn validate_amount_array_sum_overflow_returns_error() {
        let amounts = [i128::MAX, i128::MAX];
        assert_eq!(
            validate_amount_array(&amounts),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_amount_array_sum_near_i128_max() {
        let half = i128::MAX / 2;
        let remainder = i128::MAX - half;
        let amounts = [half, remainder];
        assert_eq!(
            validate_amount_array(&amounts),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_amount_array_sum_one_over_i128_max() {
        let half = i128::MAX / 2;
        let amounts = [half, half + 1];
        assert_eq!(
            validate_amount_array(&amounts),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_amount_array_single_max_amount() {
        let amounts = [MAX_SINGLE_AMOUNT_STROOPS];
        assert_eq!(
            validate_amount_array(&amounts),
            Ok(MAX_SINGLE_AMOUNT_STROOPS)
        );
    }

    #[test]
    fn validate_amount_array_empty() {
        let amounts: [i128; 0] = [];
        assert_eq!(validate_amount_array(&amounts), Ok(0));
    }

    #[test]
    fn validate_amount_array_many_small_values_sum_to_max() {
        let per = MAX_SINGLE_AMOUNT_STROOPS / 100;
        let amounts: [i128; 100] = [per; 100];
        assert_eq!(validate_amount_array(&amounts), Ok(per * 100));
    }

    // ── Deposit amount overflow ────────────────────────────────────────────

    #[test]
    fn validate_deposit_amount_i128_max_current_plus_one() {
        assert_eq!(
            validate_deposit_amount(1, i128::MAX, i128::MAX),
            Err(crate::EscrowError::PotentialOverflow)
        );
    }

    #[test]
    fn validate_deposit_amount_two_large_values_overflow() {
        let a = i128::MAX / 2 + 1;
        let b = i128::MAX / 2 + 1;
        assert_eq!(
            validate_deposit_amount(a, b, i128::MAX),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_deposit_amount_exact_i128_max_current() {
        assert_eq!(
            validate_deposit_amount(i128::MAX, i128::MAX, i128::MAX),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_deposit_amount_zero_current() {
        assert!(validate_deposit_amount(100, 0, 200).is_ok());
    }

    #[test]
    fn validate_deposit_amount_sum_exceeds_max() {
        assert_eq!(
            validate_deposit_amount(600, 500, 1000),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_deposit_amount_exactly_fills_capacity() {
        assert!(validate_deposit_amount(500, 500, 1000).is_ok());
    }

    #[test]
    fn validate_deposit_amount_one_stroop_over() {
        assert_eq!(
            validate_deposit_amount(501, 500, 1000),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    // ── safe_add_amounts / safe_subtract_amounts boundary tests ────────────

    #[test]
    fn safe_add_two_i128_max() {
        assert_eq!(safe_add_amounts(i128::MAX, i128::MAX), None);
    }

    #[test]
    fn safe_add_i128_max_and_zero() {
        assert_eq!(safe_add_amounts(i128::MAX, 0), Some(i128::MAX));
    }

    #[test]
    fn safe_add_i128_min_and_zero() {
        assert_eq!(safe_add_amounts(i128::MIN, 0), Some(i128::MIN));
    }

    #[test]
    fn safe_add_i128_min_and_negative_one() {
        assert_eq!(safe_add_amounts(i128::MIN, -1), None);
    }

    #[test]
    fn safe_add_i128_max_and_one() {
        assert_eq!(safe_add_amounts(i128::MAX, 1), None);
    }

    #[test]
    fn safe_add_negative_values() {
        assert_eq!(safe_add_amounts(-100, -200), Some(-300));
    }

    #[test]
    fn safe_subtract_i128_min_and_one() {
        assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
    }

    #[test]
    fn safe_subtract_i128_max_and_negative_one() {
        assert_eq!(safe_subtract_amounts(i128::MAX, -1), None);
    }

    #[test]
    fn safe_subtract_zero_and_i128_max() {
        assert_eq!(safe_subtract_amounts(0, i128::MAX), Some(i128::MIN + 1));
    }

    #[test]
    fn safe_subtract_same_value_returns_zero() {
        assert_eq!(safe_subtract_amounts(12345, 12345), Some(0));
    }

    #[test]
    fn safe_subtract_zero_and_zero() {
        assert_eq!(safe_subtract_amounts(0, 0), Some(0));
    }

    #[test]
    fn safe_subtract_i128_max_and_zero() {
        assert_eq!(safe_subtract_amounts(i128::MAX, 0), Some(i128::MAX));
    }

    #[test]
    fn safe_subtract_i128_min_and_zero() {
        assert_eq!(safe_subtract_amounts(i128::MIN, 0), Some(i128::MIN));
    }

    // ── accumulate_amounts boundary tests ──────────────────────────────────

    #[test]
    fn accumulate_amounts_empty() {
        assert_eq!(accumulate_amounts([]), Ok(0));
    }

    #[test]
    fn accumulate_amounts_overflow() {
        assert_eq!(
            accumulate_amounts([i128::MAX, 1]),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn accumulate_amounts_rejects_negative() {
        assert_eq!(
            accumulate_amounts([-1]),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn accumulate_amounts_rejects_zero() {
        assert_eq!(
            accumulate_amounts([0]),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn accumulate_amounts_rejects_overbound() {
        assert_eq!(
            accumulate_amounts([MAX_SINGLE_AMOUNT_STROOPS + 1]),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn accumulate_amounts_near_max() {
        let half = MAX_SINGLE_AMOUNT_STROOPS / 2;
        let remainder = MAX_SINGLE_AMOUNT_STROOPS - half;
        assert_eq!(
            accumulate_amounts([half, remainder]),
            Ok(MAX_SINGLE_AMOUNT_STROOPS)
        );
    }

    // ── validate_contract_total boundary tests ─────────────────────────────

    #[test]
    fn validate_contract_total_at_zero() {
        assert!(validate_contract_total(0, 100).is_ok());
    }

    #[test]
    fn validate_contract_total_exceeds_max() {
        assert_eq!(
            validate_contract_total(101, 100),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_contract_total_exactly_at_max() {
        assert!(validate_contract_total(100, 100).is_ok());
    }

    #[test]
    fn validate_contract_total_one_under_max() {
        assert!(validate_contract_total(99, 100).is_ok());
    }

    #[test]
    fn validate_contract_total_i128_max_exceeds_zero() {
        assert_eq!(
            validate_contract_total(i128::MAX, 0),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_contract_total_both_i128_max() {
        assert!(validate_contract_total(i128::MAX, i128::MAX).is_ok());
    }

    // ── validate_milestone_amounts boundary tests ──────────────────────────

    #[test]
    fn validate_milestone_amounts_overflow_in_sum() {
        assert_eq!(
            validate_milestone_amounts(&[i128::MAX, 1], i128::MAX),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_milestone_amounts_empty_array() {
        assert_eq!(validate_milestone_amounts(&[], 100), Ok(0));
    }

    #[test]
    fn validate_milestone_amounts_total_exceeds_contract_max() {
        assert_eq!(
            validate_milestone_amounts(&[60, 60], 100),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn validate_milestone_amounts_total_exactly_at_max() {
        assert_eq!(validate_milestone_amounts(&[50, 50], 100), Ok(100));
    }

    #[test]
    fn validate_milestone_amounts_rejects_negative_element() {
        assert_eq!(
            validate_milestone_amounts(&[100, -1], 200),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn validate_milestone_amounts_single_element() {
        assert_eq!(validate_milestone_amounts(&[42], 100), Ok(42));
    }
}
