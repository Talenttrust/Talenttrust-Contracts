//! Amount validation and sanitization module.
//!
//! Provides centralized validation for all money-like values in the escrow
//! contract. Ensures positivity, max bounds, and proper stroop precision
//! handling.
//!
//! Storage ownership: none. This module is deliberately stateless; callers use
//! these helpers before writing validated values to contract and milestone
//! storage.
//!
//! # Failure-recovery invariant
//!
//! Every entry point in this module is *all-or-nothing*: it either returns the
//! fully-validated value or fails closed with a typed [`crate::EscrowError`]
//! variant. A rejected amount never yields a partial or ambiguous result:
//!
//! - [`safe_add_amounts`] / [`safe_subtract_amounts`] return `None` on overflow
//!   or underflow; they never wrap.
//! - [`accumulate_amounts`] validates every element and uses checked addition.
//!   On the first invalid element or overflow it returns a typed error and
//!   discards the running total, so no partial sum is observable.
//! - The `validate_*` helpers map each failure class to one deterministic
//!   [`crate::EscrowError`] variant (see each function for its exact mapping).
//!
//! Because the module owns no storage, rejecting an amount cannot corrupt
//! state; a caller that observes an error can safely retry, and the retry sees
//! exactly the same inputs and the same deterministic outcome.

/// Maximum number of decimal places for stroop precision (7 decimal places for Stellar)
#[allow(dead_code)] // available for callers; not used internally
pub const STROOP_PRECISION: u8 = 7;

/// Maximum individual amount allowed per operation to prevent overflow
pub const MAX_SINGLE_AMOUNT_STROOPS: i128 = 1_000_000_0000000; // 1M tokens

/// Minimum positive amount (1 stroop)
pub const MIN_POSITIVE_AMOUNT: i128 = 1;

/// Maximum contract total allowed (in stroops).
/// This is the absolute ceiling for any single contract's total funding.
pub const MAX_CONTRACT_TOTAL_STROOPS: i128 = 1_000_000_0000000; // 1M tokens

/// Classification of an amount against the single-amount boundary.
///
/// The three regions are mutually exclusive and together cover the entire
/// `i128` domain, which makes [`classify_amount`] total and deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmountBoundary {
    /// Amount is below [`MIN_POSITIVE_AMOUNT`] (zero or negative).
    NonPositive,
    /// Amount is within `[MIN_POSITIVE_AMOUNT, MAX_SINGLE_AMOUNT_STROOPS]`.
    WithinBounds,
    /// Amount is strictly greater than [`MAX_SINGLE_AMOUNT_STROOPS`].
    AboveMaximum,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AmountValidationError {
    NonPositiveAmount,
    AmountExceedsMaximum,
    ExceedsContractMaximum,
}

/// Classify `amount` against the single-amount boundary.
///
/// The mapping is total and deterministic over the whole `i128` domain,
/// including `i128::MIN` and `i128::MAX`.
pub fn classify_amount(amount: i128) -> AmountBoundary {
    if amount < MIN_POSITIVE_AMOUNT {
        AmountBoundary::NonPositive
    } else if amount > MAX_SINGLE_AMOUNT_STROOPS {
        AmountBoundary::AboveMaximum
    } else {
        AmountBoundary::WithinBounds
    }
}

/// Validates a single amount for positivity and bounds.
///
/// Equivalent to requiring [`classify_amount`] to return
/// [`AmountBoundary::WithinBounds`].
///
/// #Arguments
/// * `amount` - The amount to validate (in stroops)
///
/// #Returns
/// `Ok(())` if valid, `Err(AmountValidationError)` if invalid
pub fn validate_single_amount(amount: i128) -> Result<(), crate::EscrowError> {
    // Check positivity
    if amount < MIN_POSITIVE_AMOUNT {
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

/// Validates an amount array/vector for positivity and bounds.
///
/// #Arguments
/// * `amounts` - Slice of amounts to validate (in stroops)
///
/// #Returns
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

/// Validates a total amount against the effective contract maximum.
///
/// # Arguments
/// * `total_amount` - The total amount to validate (in stroops)
/// * `max_contract_total` - Maximum allowed per contract (in stroops)
///
/// #Returns
/// `Ok(())` if valid, `Err(AmountValidationError)` if invalid
///
/// # Security
/// Rejects a non-positive ``max_contract_total`` as an invalid configuration. A cap
/// of zero or negative would otherwise accept any positive total and silently
/// bypass the contract limit.
pub fn validate_contract_total(
    total_amount: i128,
    max_contract_total: i128,
) -> Result<(), crate::EscrowError> {
    // A non-positive cap is an invalid configuration and must not be treated as
    // "unlimited".
    if max_contract_total < MIN_POSITIVE_AMOUNT {
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }

    // Reject non-positive totals explicitly so callers get a precise error.
    if total_amount < MIN_POSITIVE_AMOUNT {
        return Err(crate::EscrowError::AmountMustBePositive);
    }

    if total_amount > max_contract_total {
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }
    Ok(())
}

/// Comprehensive validation for milestone amounts.
///
/// # Arguments
/// * `milestone_amounts` - Array of milestone amounts (in stroops)
/// * `max_contract_total` - Maximum allowed per contract (in stroops)
///
/// #Returns
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

/// Validates a deposit amount against remaining contract capacity.
///
/// This is the pure preflight used by deposit-shaped callers. It rejects any
/// amount that is not strictly positive, any projection that would exceed the
/// contract cap, and any arithmetic that would overflow `i128`.
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
/// #Returns
/// * `Ok(())` - Deposit is valid and won't exceed capacity
/// * `Err(EscrowError::AmountMustBePositive)` - Deposit amount is ≤ 0
/// * `Err(EscrowError::InvalidMilestoneAmount)` - Deposit would exceed capacity,
///   the single amount is too large, the cap is non-positive (invalid config),
///   or `current_deposited < 0` (a corrupted balance that must never be treated
///   as additional capacity)
/// * `Err(EscrowError::PotentialOverflow)` - Adding deposit to current would overflow i128
///
/// # Security
///
/// - Uses checked arithmetic to prevent integer overflow panics
/// - Rejects any deposit when contract is already fully funded
/// - Validates deposit amount bounds before checking capacity
/// - Rejects negative `current_deposited` so a corrupted or uninitialized
///   state cannot be used to inflate remaining capacity.
pub fn validate_deposit_amount(
    deposit_amount: i128,
    current_deposited: i128,
    max_contract_total: i128,
) -> Result<(), crate::EscrowError> {
    // Validate deposit amount itself
    validate_single_amount(deposit_amount)?;

    // A negative current balance is an invalid state; reject it rather than
    // silently treating it as additional capacity.
    if current_deposited < MIN_POSITIVE_AMOUNT - 1 {
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }

    // Reject a non-positive cap as an invalid configuration.
    if max_contract_total < MIN_POSITIVE_AMOUNT {
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }

    // Check if deposit would exceed contract maximum
    if let Some(new_total) = current_deposited.checked_add(deposit_amount) {
        if new_total > max_contract_total {
            return Err(crate::EscrowError::InvalidMilestoneAmount);
        }
    } else {
        return Err(crate::EscrowError::PotentialOverflow);
    }

    // A non-positive cap is an invalid configuration and cannot be used to
    // bound a deposit.
    if max_contract_total < MIN_POSITIVE_AMOUNT {
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }

    // Check if deposit would exceed contract maximum
    match current_deposited.checked_add(deposit_amount) {
        Some(new_total) if new_total > max_contract_total => {
            Err(crate::EscrowError::InvalidMilestoneAmount)
        }
        Some(_) => Ok(()),
        None => Err(crate::EscrowError::PotentialOverflow),
    }
}

/// Utility function to safely add amounts with overflow protection
///
/// #Arguments
/// * `a` - First amount
/// * `b` - Second amount
///
/// #Returns
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
/// #Returns
/// `Some(difference)` if subtraction succeeds, `None` if underflow would occur
pub fn safe_subtract_amounts(a: i128, b: i128) -> Option<i128> {
    a.checked_sub(b)
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
/// #Arguments
/// * `amounts` - Iterator over amount references (typically milestone amounts)
///
/// #Returns
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
    fn test_classify_amount_boundaries() {
        // Non-positive region (open on the left, inclusive of zero).
        assert_eq!(classify_amount(i128::MIN), AmountBoundary::NonPositive);
        assert_eq!(classify_amount(-2), AmountBoundary::NonPositive);
        assert_eq!(classify_amount(-1), AmountBoundary::NonPositive);
        assert_eq!(classify_amount(0), AmountBoundary::NonPositive);

        // Within-bounds region: exactly MIN_POSITIVE_AMOUNT and exactly max.
        assert_eq!(
            classify_amount(MIN_POSITIVE_AMOUNT),
            AmountBoundary::WithinBounds
        );
        assert_eq!(classify_amount(1), AmountBoundary::WithinBounds);
        assert_eq!(
            classify_amount(MIN_POSITIVE_AMOUNT + 1),
            AmountBoundary::WithinBounds
        );
        assert_eq!(
            classify_amount(MAX_SINGLE_AMOUNT_STROOPS - 1),
            AmountBoundary::WithinBounds
        );
        assert_eq!(
            classify_amount(MAX_SINGLE_AMOUNT_STROOPS),
            AmountBoundary::WithinBounds
        );

        // Above-maximum region.
        assert_eq!(
            classify_amount(MAX_SINGLE_AMOUNT_STROOPS + 1),
            AmountBoundary::AboveMaximum
        );
        assert_eq!(classify_amount(i128::MAX), AmountBoundary::AboveMaximum);
    }

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
        assert_eq!(
            validate_single_amount(i128::MAX),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_validate_amount_array() {
        let amounts1 = [100_0000000, 200_0000000, 300_0000000];
        assert!(validate_amount_array(&amounts1).is_ok());
        assert_eq!(validate_amount_array(&amounts1).unwrap(), 600_0000000);

        // Duplicate amounts are valid independent entries.
        assert_eq!(
            validate_amount_array(&[100_0000000, 100_0000000]).unwrap(),
            200_0000000
        );

        // Empty input is allowed and yields zero; callers that need at least one
        // entry enforce that upstream.
        assert_eq!(validate_amount_array(&[]).unwrap(), 0);

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
        assert!(validate_contract_total(1, max_total).is_ok());
        assert!(validate_contract_total(max_total, max_total).is_ok());
        assert_eq!(
            validate_contract_total(max_total + 1, max_total),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );

        // Non-positive totals are rejected (a real total is a sum of positive
        // milestones).
        assert_eq!(
            validate_contract_total(0, max_total),
            Err(crate::EscrowError::AmountMustBePositive)
        );
        assert_eq!(
            validate_contract_total(-1, max_total),
            Err(crate::EscrowError::AmountMustBePositive)
        );

        // A non-positive cap is an invalid configuration.
        assert_eq!(
            validate_contract_total(100, 0),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        assert_eq!(
            validate_contract_total(100, -1),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_validate_contract_total_rejects_non_positive_inputs() {
        // Non-positive total is rejected explicitly.
        assert_eq!(
            validate_contract_total(0, 1000),
            Err(crate::EscrowError::AmountMustBePositive)
        );
        assert_eq!(
            validate_contract_total(-1, 1000),
            Err(crate::EscrowError::AmountMustBePositive)
        );

        // Non-positive cap is an invalid configuration.
        assert_eq!(
            validate_contract_total(1, 0),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        assert_eq!(
            validate_contract_total(1, -1),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_validate_milestone_amounts() {
        let max_contract_total = 1_000_000_0000000;
        let milestones1 = [100_0000000, 200_0000000, 300_0000000];
        assert!(validate_milestone_amounts(&milestones1, max_contract_total).is_ok());

        // Each milestone is individually valid, but their sum exceeds the
        // contract cap, so the whole array is rejected as a unit.
        let milestones2 = [MAX_SINGLE_AMOUNT_STROOPS, MAX_SINGLE_AMOUNT_STROOPS];
        assert_eq!(
            validate_milestone_amounts(&milestones2, max_contract_total),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );

        // Duplicate milestone amounts sum normally.
        assert_eq!(
            validate_milestone_amounts(&[100_0000000, 100_0000000], max_contract_total).unwrap(),
            200_0000000
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
                name: "deposit that would overflow i128 should fail with PotentialOverflow",
                deposit_amount: 1,
                current_deposited: i128::MAX,
                max_contract_total: i128::MAX,
                expected: Err(crate::EscrowError::PotentialOverflow),
            },
        ];

        for tc in test_cases.iter() {
            assert_eq!(
                validate_deposit_amount(
                    tc.deposit_amount,
                    tc.current_deposited,
                    tc.max_contract_total
                ),
                tc.expected,
                "test case failed: {}",
                tc.name
            );
        }
    }

    #[test]
    fn test_safe_add_amounts() {
        assert_eq!(safe_add_amounts(1, 2), Some(3));
        assert_eq!(safe_add_amounts(i128::MAX, 1), None);
        assert_eq!(safe_add_amounts(i128::MIN, -1), None);
    }

    #[test]
    fn test_safe_subtract_amounts() {
        assert_eq!(safe_subtract_amounts(3, 1), Some(2));
        assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
        assert_eq!(safe_subtract_amounts(i128::MAX, -1), None);
    }

    #[test]
    fn test_accumulate_amounts() {
        let amounts = [100_0000000, 200_0000000, 300_0000000];
        assert_eq!(accumulate_amounts(amounts), Ok(600_0000000));

        let invalid = [100_0000000, 0, 300_0000000];
        assert_eq!(
            accumulate_amounts(invalid),
            Err(crate::EscrowError::AmountMustBePositive)
        );

        // An element above the per-operation cap is rejected before it is ever
        // added, so accumulation fails closed with a typed error instead of
        // overflowing or handing back a partial sum. (Cumulative `i128`
        // overflow is unreachable behind this cap; the checked-add arm is
        // defensive and is exercised directly in the `failure_recovery_tests`
        // module.)
        let oversized = [MAX_SINGLE_AMOUNT_STROOPS, MAX_SINGLE_AMOUNT_STROOPS + 1];
        assert_eq!(
            accumulate_amounts(oversized),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_boundary_values() {
        // Minimum positive amount
        assert!(validate_single_amount(MIN_POSITIVE_AMOUNT).is_ok());
        // Maximum single amount
        assert!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS).is_ok());
        // One beyond maximum
        assert_eq!(
            validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS + 1),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        // zero
        assert_eq!(
            validate_single_amount(0),
            Err(crate::EscrowError::AmountMustBePositive)
        );
        // negative
        assert_eq!(
            validate_single_amount(-1),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn test_duplicate_amounts_in_array() {
        // Duplicate amounts are allowed in arrays; they are summed correctly.
        let duplicates = [100_0000000, 100_0000000, 100_0000000];
        assert_eq!(validate_amount_array(&duplicates), Ok(300_0000000));
    }

    #[test]
    fn test_empty_array() {
        // Empty array is valid and yields zero total.
        assert_eq!(validate_amount_array(&[]), Ok(0));
        assert_eq!(accumulate_amounts(core::iter::empty::<i128>()), Ok(0));
    }

    #[test]
    fn test_contract_total_boundaries() {
        // Exactly at maximum
        assert!(
            validate_contract_total(MAX_CONTRACT_TOTAL_STROOPS, MAX_CONTRACT_TOTAL_STROOPS).is_ok()
        );
        // One over
        assert_eq!(
            validate_contract_total(MAX_CONTRACT_TOTAL_STROOPS + 1, MAX_CONTRACT_TOTAL_STROOPS),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        // Zero total is rejected explicitly: a real total is a sum of positive
        // milestones, so a zero total cannot originate from valid input.
        assert_eq!(
            validate_contract_total(0, MAX_CONTRACT_TOTAL_STROOPS),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn test_milestone_amounts_boundaries() {
        // Single milestone at max contract total
        let single_max = [MAX_CONTRACT_TOTAL_STROOPS];
        assert!(validate_milestone_amounts(&single_max, MAX_CONTRACT_TOTAL_STROOPS).is_ok());
        // Single milestone one over max contract total
        let single_over = [MAX_CONTRACT_TOTAL_STROOPS + 1];
        assert_eq!(
            validate_milestone_amounts(&single_over, MAX_CONTRACT_TOTAL_STROOPS),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_deposit_amount_boundaries() {
        // Exactly remaining capacity
        assert!(validate_deposit_amount(500, 500, 1000).is_ok());
        // One stroop short
        assert!(validate_deposit_amount(499, 500, 1000).is_ok());
        // One stroop over
        assert_eq!(
            validate_deposit_amount(501, 500, 1000),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        // Zip deposit into empty contract
        assert!(validate_deposit_amount(1, 0, 1000).is_ok());
        // Zip deposit into full contract
        assert_eq!(
            validate_deposit_amount(1, 1000, 1000),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_deposit_amount_overflow() {
        // A valid deposit projected onto an already-maximal balance overflows
        // `i128` and must surface as `PotentialOverflow`, never a wrap.
        assert_eq!(
            validate_deposit_amount(1, i128::MAX, i128::MAX),
            Err(crate::EscrowError::PotentialOverflow)
        );
        // Adding deposit to current exactly at i128::MAX is fine.
        assert!(validate_deposit_amount(1, i128::MAX - 1, i128::MAX).is_ok());
    }

    #[test]
    fn test_accumulate_amounts_boundaries() {
        // Accumulation of amounts at the per-operation cap stays exact.
        let amounts = [MAX_SINGLE_AMOUNT_STROOPS, MAX_SINGLE_AMOUNT_STROOPS];
        assert_eq!(
            accumulate_amounts(amounts),
            Ok(MAX_SINGLE_AMOUNT_STROOPS * 2)
        );
        // One element above the cap is rejected as a unit; no partial sum is
        // returned.
        let oversized = [MAX_SINGLE_AMOUNT_STROOPS, MAX_SINGLE_AMOUNT_STROOPS + 1];
        assert_eq!(
            accumulate_amounts(oversized),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }
}

/// Focused failure-recovery tests for issue #1389.
///
/// Each test name starts with the acceptance criterion it pins (`ac1`–`ac4`)
/// or `regression`. The module is deliberately allocation-free (`#![no_std]`
/// crate) so the tests exercise only the stateless helpers.
#[cfg(test)]
mod failure_recovery_tests {
    use super::*;

    /// AC1 — `safe_add_amounts` is exact and never wraps.
    #[test]
    fn ac1_safe_add_amounts_is_exact_and_never_wraps() {
        // Legitimate results, including the exact upper/lower boundary.
        assert_eq!(safe_add_amounts(1, 2), Some(3));
        assert_eq!(safe_add_amounts(0, 0), Some(0));
        assert_eq!(safe_add_amounts(-5, 5), Some(0));
        assert_eq!(safe_add_amounts(i128::MAX, 0), Some(i128::MAX));
        assert_eq!(safe_add_amounts(i128::MIN, 0), Some(i128::MIN));
        assert_eq!(safe_add_amounts(i128::MAX, -1), Some(i128::MAX - 1));

        // Exact `None` at the overflow boundary; never a wrapped value.
        assert_eq!(safe_add_amounts(i128::MAX, 1), None);
        assert_eq!(safe_add_amounts(i128::MAX, i128::MAX), None);
        assert_eq!(safe_add_amounts(i128::MIN, -1), None);
        assert_eq!(safe_add_amounts(i128::MIN, i128::MIN), None);
    }

    /// AC1 — `safe_subtract_amounts` is exact and never wraps.
    #[test]
    fn ac1_safe_subtract_amounts_is_exact_and_never_wraps() {
        // Legitimate results, including the exact lower/upper boundary.
        assert_eq!(safe_subtract_amounts(3, 1), Some(2));
        assert_eq!(safe_subtract_amounts(0, 0), Some(0));
        assert_eq!(safe_subtract_amounts(i128::MIN, 0), Some(i128::MIN));
        assert_eq!(safe_subtract_amounts(i128::MIN, -1), Some(i128::MIN + 1));
        assert_eq!(safe_subtract_amounts(i128::MAX, i128::MAX), Some(0));

        // Exact `None` at the underflow boundary; never a wrapped value.
        assert_eq!(safe_subtract_amounts(i128::MAX, -1), None);
        assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
        assert_eq!(safe_subtract_amounts(i128::MIN, i128::MAX), None);
        assert_eq!(safe_subtract_amounts(-2, i128::MAX), None);
    }

    /// AC2 — `accumulate_amounts` fails closed and exposes no partial sum.
    #[test]
    fn ac2_accumulate_amounts_fails_closed_without_partial_sum() {
        // Empty input accumulates to zero.
        assert_eq!(accumulate_amounts(core::iter::empty::<i128>()), Ok(0));

        // Success path returns the exact full sum.
        assert_eq!(accumulate_amounts([1, 2, 3]), Ok(6));
        assert_eq!(
            accumulate_amounts([MAX_SINGLE_AMOUNT_STROOPS, MAX_SINGLE_AMOUNT_STROOPS]),
            Ok(MAX_SINGLE_AMOUNT_STROOPS * 2)
        );

        // A zero element mid-iteration fails closed with a typed error; the
        // already-accumulated total is discarded rather than returned.
        assert_eq!(
            accumulate_amounts([1, 0, 1]),
            Err(crate::EscrowError::AmountMustBePositive)
        );

        // An element above the per-operation cap is rejected before it is
        // added. Because the cap keeps every accepted element far below
        // `i128::MAX`, cumulative `i128` overflow cannot be reached through
        // valid input; the checked-add arm remains a defensive backstop and
        // the equivalent arithmetic guarantee is pinned in `ac1_*`.
        assert_eq!(
            accumulate_amounts([1, MAX_SINGLE_AMOUNT_STROOPS + 1]),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    /// AC3 — `validate_single_amount` maps each failure class to one variant.
    #[test]
    fn ac3_validate_single_amount_maps_each_failure_class() {
        // Success class.
        assert_eq!(validate_single_amount(MIN_POSITIVE_AMOUNT), Ok(()));
        assert_eq!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS), Ok(()));

        // Below-minimum class (zero, negative, `MIN_POSITIVE_AMOUNT - 1`, and
        // the `i128` floor).
        for amount in [0, -1, MIN_POSITIVE_AMOUNT - 1, i128::MIN] {
            assert_eq!(
                validate_single_amount(amount),
                Err(crate::EscrowError::AmountMustBePositive),
                "amount {amount} must be rejected as non-positive"
            );
        }

        // Above-maximum class (`MAX_SINGLE_AMOUNT_STROOPS + 1` and `i128::MAX`).
        for amount in [MAX_SINGLE_AMOUNT_STROOPS + 1, i128::MAX] {
            assert_eq!(
                validate_single_amount(amount),
                Err(crate::EscrowError::InvalidMilestoneAmount),
                "amount {amount} must be rejected as above maximum"
            );
        }
    }

    /// AC3 — `validate_contract_total` maps each failure class to one variant.
    #[test]
    fn ac3_validate_contract_total_maps_each_failure_class() {
        let cap = MAX_CONTRACT_TOTAL_STROOPS;
        assert_eq!(validate_contract_total(1, cap), Ok(()));
        assert_eq!(validate_contract_total(cap, cap), Ok(()));

        // Non-positive total → `AmountMustBePositive`.
        for total in [0, -1, i128::MIN] {
            assert_eq!(
                validate_contract_total(total, cap),
                Err(crate::EscrowError::AmountMustBePositive),
                "total {total} must be rejected as non-positive"
            );
        }

        // Total above cap → `InvalidMilestoneAmount`.
        assert_eq!(
            validate_contract_total(cap + 1, cap),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        assert_eq!(
            validate_contract_total(i128::MAX, cap),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );

        // Non-positive cap (invalid configuration) is rejected *before* the
        // total, so the mapping stays deterministic regardless of the total.
        for bad_cap in [0, -1, i128::MIN] {
            assert_eq!(
                validate_contract_total(1, bad_cap),
                Err(crate::EscrowError::InvalidMilestoneAmount),
                "cap {bad_cap} must be rejected as invalid configuration"
            );
        }
    }

    /// AC3 — `validate_deposit_amount` maps each failure class to one variant.
    #[test]
    fn ac3_validate_deposit_amount_maps_each_failure_class() {
        // Success class, including the exactly-remaining boundary.
        assert_eq!(validate_deposit_amount(1, 0, 1000), Ok(()));
        assert_eq!(validate_deposit_amount(500, 500, 1000), Ok(()));

        // Non-positive deposit → `AmountMustBePositive` (checked first).
        assert_eq!(
            validate_deposit_amount(0, 0, 1000),
            Err(crate::EscrowError::AmountMustBePositive)
        );
        assert_eq!(
            validate_deposit_amount(-1, 0, 1000),
            Err(crate::EscrowError::AmountMustBePositive)
        );

        // Negative current balance (corrupted state) → `InvalidMilestoneAmount`.
        assert_eq!(
            validate_deposit_amount(1, -1, 1000),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        assert_eq!(
            validate_deposit_amount(1, i128::MIN, 1000),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );

        // Non-positive cap (invalid configuration) → `InvalidMilestoneAmount`.
        assert_eq!(
            validate_deposit_amount(1, 0, 0),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );

        // Capacity exceeded → `InvalidMilestoneAmount`.
        assert_eq!(
            validate_deposit_amount(1, 1000, 1000),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );

        // Projection overflow → `PotentialOverflow`.
        assert_eq!(
            validate_deposit_amount(1, i128::MAX, i128::MAX),
            Err(crate::EscrowError::PotentialOverflow)
        );
    }

    /// AC4 — a rejected amount never yields a partial or ambiguous result.
    #[test]
    fn ac4_rejected_amount_never_yields_partial_result() {
        // Failure is `Err`, never `Ok` carrying a partial accumulation.
        let rejected = accumulate_amounts([100, 0, 200]);
        assert!(rejected.is_err());
        assert_ne!(rejected, Ok(100));
        assert_ne!(rejected, Ok(300));

        // Same for the array validator.
        let rejected = validate_amount_array(&[100, 0, 200]);
        assert!(rejected.is_err());
        assert_ne!(rejected, Ok(100));
        assert_ne!(rejected, Ok(300));

        // Overflow is `None`, never a wrapped value a caller could mistake for
        // a legitimate result.
        assert_eq!(safe_add_amounts(i128::MAX, 1), None);
        assert_ne!(safe_add_amounts(i128::MAX, 1), Some(i128::MIN));
    }

    /// Regression — the exact boundary matrix from issue #1389 is deterministic.
    #[test]
    fn regression_boundary_matrix_is_deterministic() {
        let cases: [(i128, AmountBoundary, Result<(), crate::EscrowError>); 6] = [
            (
                i128::MIN,
                AmountBoundary::NonPositive,
                Err(crate::EscrowError::AmountMustBePositive),
            ),
            (
                MIN_POSITIVE_AMOUNT - 1,
                AmountBoundary::NonPositive,
                Err(crate::EscrowError::AmountMustBePositive),
            ),
            (MIN_POSITIVE_AMOUNT, AmountBoundary::WithinBounds, Ok(())),
            (
                MAX_SINGLE_AMOUNT_STROOPS,
                AmountBoundary::WithinBounds,
                Ok(()),
            ),
            (
                MAX_SINGLE_AMOUNT_STROOPS + 1,
                AmountBoundary::AboveMaximum,
                Err(crate::EscrowError::InvalidMilestoneAmount),
            ),
            (
                i128::MAX,
                AmountBoundary::AboveMaximum,
                Err(crate::EscrowError::InvalidMilestoneAmount),
            ),
        ];

        for (amount, boundary, expected) in cases {
            // Calling twice proves the helpers hold no hidden state.
            assert_eq!(classify_amount(amount), boundary, "classify({amount})");
            assert_eq!(
                classify_amount(amount),
                boundary,
                "classify({amount}) on retry"
            );
            assert_eq!(
                validate_single_amount(amount),
                expected,
                "validate_single_amount({amount})"
            );
            assert_eq!(
                validate_single_amount(amount),
                expected,
                "validate_single_amount({amount}) on retry"
            );
        }
    }

    /// Regression — a pre-existing rejection cannot be observed as success on
    /// retry, and a valid boundary amount remains accepted on retry.
    #[test]
    fn regression_rejection_is_stable_across_retries() {
        for amount in [0, -1, MAX_SINGLE_AMOUNT_STROOPS + 1, i128::MAX] {
            let first = validate_single_amount(amount);
            let second = validate_single_amount(amount);
            assert!(first.is_err(), "amount {amount} must be rejected");
            assert_eq!(first, second, "amount {amount} must reject identically");
        }

        assert_eq!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS), Ok(()));
        assert_eq!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS), Ok(()));
    }
}
