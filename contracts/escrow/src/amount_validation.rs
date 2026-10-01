//! Amount validation and sanitization module
///
/// Provides centralized validation for all money-like values in the escrow contract.
/// Ensures positivity, max bounds, and proper stroop precision handling.
///
/// Storage ownership: none. This module is deliberately stateless; callers use
/// these helpers before writing validated values to contract and milestone
/// storage.

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

#[derive(Debug, PartialEq, Eq)]
pub enum AmountValidationError {
    NonPositiveAmount,
    AmountExceedsMaximum,
    ExceedsContractMaximum,
}

/// Amount boundary classification returned by [`classify_amount`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountBoundary {
    /// amount < MIN_POSITIVE_AMOUNT
    NonPositive,
    /// MIN_POSITIVE_AMOUNT <= amount <= MAX_SINGLE_AMOUNT_STROOPS
    WithinBounds,
    /// amount > MAX_SINGLE_AMOUNT_STROOPS
    AboveMaximum,
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
/// # Arguments
/// * `amount` - The amount to validate (in stroops)
///
/// # Returns
/// `Ok(())` if valid, `Err(EscrowError)` if invalid
pub fn validate_single_amount(amount: i128) -> Result<(), crate::EscrowError> {
    if amount < MIN_POSITIVE_AMOUNT {
        return Err(crate::EscrowError::AmountMustBePositive);
    }

    if amount > MAX_SINGLE_AMOUNT_STROOPS {
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }

    Ok(())
}

/// Validates an amount array/vector for positivity and bounds.
///
/// # Arguments
/// * `amounts` - Slice of amounts to validate (in stroops)
///
/// # Returns
/// `Ok(total)` with sum of all amounts if valid, `Err(EscrowError)` if invalid
pub fn validate_amount_array(amounts: &[i128]) -> Result<i128, crate::EscrowError> {
    let mut total: i128 = 0;

    for &amount in amounts.iter() {
        validate_single_amount(amount)?;

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
/// # Returns
/// `Ok(())` if valid, `Err(EscrowError)` if invalid
///
/// # Security
/// Rejects a non-positive `max_contract_total` as an invalid configuration. A cap
/// of zero or negative would otherwise accept any positive total and silently
/// bypass the contract limit.
pub fn validate_contract_total(
    total_amount: i128,
    max_contract_total: i128,
) -> Result<(), crate::EscrowError> {
    if max_contract_total < MIN_POSITIVE_AMOUNT {
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }

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
/// # Returns
/// `Ok(total)` with sum of all milestones if valid, `Err(EscrowError)` if invalid
pub fn validate_milestone_amounts(
    milestone_amounts: &[i128],
    max_contract_total: i128,
) -> Result<i128, crate::EscrowError> {
    let total = validate_amount_array(milestone_amounts)?;

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
/// # Returns
/// * `Ok(())` - Deposit is valid and won't exceed capacity
/// * `Err(EscrowError::AmountMustBePositive)` - Deposit amount is ≤ 0
/// * `Err(EscrowError::InvalidMilestoneAmount)` - Deposit would exceed capacity,
///   single amount is too large, or the cap is non-positive (invalid config)
/// * `Err(EscrowError::PotentialOverflow)` - Adding deposit to current would overflow i128
/// * `Err(EscrowError::AccountingInvariantViolated)` - `current_deposited < 0`,
///   which can only arise from corrupted accounting state
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
    validate_single_amount(deposit_amount)?;

    if current_deposited < MIN_POSITIVE_AMOUNT - 1 {
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }

    if max_contract_total < MIN_POSITIVE_AMOUNT {
        return Err(crate::EscrowError::InvalidMilestoneAmount);
    }

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
        validate_single_amount(amount)?;

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
        assert_eq!(classify_amount(i128::MIN), AmountBoundary::NonPositive);
        assert_eq!(classify_amount(-2), AmountBoundary::NonPositive);
        assert_eq!(classify_amount(-1), AmountBoundary::NonPositive);
        assert_eq!(classify_amount(0), AmountBoundary::NonPositive);

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

        assert_eq!(
            validate_amount_array(&[100_0000000, 100_0000000]).unwrap(),
            200_0000000
        );

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

        assert_eq!(
            validate_contract_total(0, max_total),
            Err(crate::EscrowError::AmountMustBePositive)
        );
        assert_eq!(
            validate_contract_total(-1, max_total),
            Err(crate::EscrowError::AmountMustBePositive)
        );

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
        assert_eq!(
            validate_contract_total(0, 1000),
            Err(crate::EscrowError::AmountMustBePositive)
        );
        assert_eq!(
            validate_contract_total(-1, 1000),
            Err(crate::EscrowError::AmountMustBePositive)
        );

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
        let milestones2 = [500_000_0000000, 600_000_0000000];
        assert_eq!(
            validate_milestone_amounts(&milestones2, max_contract_total),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );

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
                deposit_amount: i128::MAX,
                current_deposited: 1,
                max_contract_total: i128::MAX,
                expected: Err(crate::EscrowError::PotentialOverflow),
            },
        ];

        for tc in test_cases.iter() {
            assert_eq!(
                validate_deposit_amount(tc.deposit_amount, tc.current_deposited, tc.max_contract_total),
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
        let amounts = vec![100_0000000, 200_0000000, 300_0000000];
        assert_eq!(accumulate_amounts(amounts), Ok(600_0000000));

        let invalid = vec![100_0000000, 0, 300_0000000];
        assert_eq!(
            accumulate_amounts(invalid),
            Err(crate::EscrowError::AmountMustBePositive)
        );

        let overflow = vec![i128::MAX - 1, 2, 1];
        assert_eq!(
            accumulate_amounts(overflow),
            Err(crate::EscrowError::PotentialOverflow)
        );
    }

    #[test]
    fn test_boundary_values() {
        assert!(validate_single_amount(MIN_POSITIVE_AMOUNT).is_ok());
        assert!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS).is_ok());
        assert_eq!(
            validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS + 1),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        assert_eq!(
            validate_single_amount(0),
            Err(crate::EscrowError::AmountMustBePositive)
        );
        assert_eq!(
            validate_single_amount(-1),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn test_duplicate_amounts_in_array() {
        let duplicates = [100_0000000, 100_0000000, 100_0000000];
        assert_eq!(
            validate_amount_array(&duplicates),
            Ok(300_0000000)
        );
    }

    #[test]
    fn test_empty_array() {
        assert_eq!(validate_amount_array(&[]), Ok(0));
        let empty: Vec<i128> = vec![];
        assert_eq!(accumulate_amounts(empty.into_iter()), Ok(0));
    }

    #[test]
    fn test_contract_total_boundaries() {
        assert!(validate_contract_total(MAX_CONTRACT_TOTAL_STROOPS, MAX_CONTRACT_TOTAL_STROOPS).is_ok());
        assert_eq!(
            validate_contract_total(MAX_CONTRACT_TOTAL_STROOPS + 1, MAX_CONTRACT_TOTAL_STROOPS),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        assert_eq!(
            validate_contract_total(0, MAX_CONTRACT_TOTAL_STROOPS),
            Err(crate::EscrowError::AmountMustBePositive)
        );
    }

    #[test]
    fn test_milestone_amounts_boundaries() {
        let single_max = [MAX_CONTRACT_TOTAL_STROOPS];
        assert!(validate_milestone_amounts(&single_max, MAX_CONTRACT_TOTAL_STROOPS).is_ok());
        let single_over = [MAX_CONTRACT_TOTAL_STROOPS + 1];
        assert_eq!(
            validate_milestone_amounts(&single_over, MAX_CONTRACT_TOTAL_STROOPS),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_deposit_amount_boundaries() {
        assert!(validate_deposit_amount(500, 500, 1000).is_ok());
        assert!(validate_deposit_amount(499, 500, 1000).is_ok());
        assert_eq!(
            validate_deposit_amount(501, 500, 1000),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
        assert!(validate_deposit_amount(1, 0, 1000).is_ok());
        assert_eq!(
            validate_deposit_amount(1, 1000, 1000),
            Err(crate::EscrowError::InvalidMilestoneAmount)
        );
    }

    #[test]
    fn test_deposit_amount_overflow() {
        assert_eq!(
            validate_deposit_amount(i128::MAX - 1, 2, i128::MAX),
            Err(crate::EscrowError::PotentialOverflow)
        );
        assert!(validate_deposit_amount(1, i128::MAX - 1, i128::MAX).is_ok());
    }

    #[test]
    fn test_accumulate_amounts_boundaries() {
        let amounts = vec![i128::MAX - 1, 1];
        assert_eq!(accumulate_amounts(amounts), Ok(i128::MAX));
        let overflow = vec![i128::MAX - 1, 1, 1];
        assert_eq!(
            accumulate_amounts(overflow),
            Err(crate::EscrowError::PotentialOverflow)
        );
    }
}
