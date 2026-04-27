//! Amount validation and sanitization module
//! 
//! Provides centralized validation for all money-like values in the escrow contract.
//! Ensures positivity, max bounds, and proper stroop precision handling.

use soroban_sdk::{Env, contracterror};

/// Maximum number of decimal places for stroop precision (7 decimal places for Stellar)
pub const STROOP_PRECISION: u8 = 7;

/// Maximum individual amount allowed per operation to prevent overflow
pub const MAX_SINGLE_AMOUNT_STROOPS: i128 = 1_000_000_0000000; // 1M tokens

/// Minimum positive amount (1 stroop)
pub const MIN_POSITIVE_AMOUNT: i128 = 1;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AmountValidationError {
    /// Amount must be positive (greater than 0)
    NonPositiveAmount = 1000,
    /// Amount exceeds maximum allowed per operation
    AmountExceedsMaximum = 1001,
    /// Amount would cause overflow in calculations
    PotentialOverflow = 1002,
    /// Invalid stroop precision (amount must be multiple of 1 stroop)
    InvalidStroopPrecision = 1003,
    /// Total amount exceeds contract maximum
    ExceedsContractMaximum = 1004,
}

/// Validates a single amount for positivity and bounds
/// 
/// # Arguments
/// * `amount` - The amount to validate (in stroops)
/// 
/// # Returns
/// `Ok(())` if valid, `Err(AmountValidationError)` if invalid
pub fn validate_single_amount(amount: i128) -> Result<(), AmountValidationError> {
    // Check positivity
    if amount <= MIN_POSITIVE_AMOUNT - 1 {
        return Err(AmountValidationError::NonPositiveAmount);
    }

    // Check maximum bounds
    if amount > MAX_SINGLE_AMOUNT_STROOPS {
        return Err(AmountValidationError::AmountExceedsMaximum);
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
pub fn validate_amount_array(amounts: &[i128]) -> Result<i128, AmountValidationError> {
    let mut total: i128 = 0;

    for (index, &amount) in amounts.iter().enumerate() {
        // Validate individual amount
        validate_single_amount(amount)?;

        // Check for potential overflow in addition
        if let Some(new_total) = total.checked_add(amount) {
            total = new_total;
        } else {
            return Err(AmountValidationError::PotentialOverflow);
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
pub fn validate_contract_total(total_amount: i128, max_contract_total: i128) -> Result<(), AmountValidationError> {
    if total_amount > max_contract_total {
        return Err(AmountValidationError::ExceedsContractMaximum);
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
    max_contract_total: i128
) -> Result<i128, AmountValidationError> {
    // Validate each milestone amount and calculate total
    let total = validate_amount_array(milestone_amounts)?;
    
    // Validate total against contract maximum
    validate_contract_total(total, max_contract_total)?;
    
    Ok(total)
}

/// Validates deposit amount against remaining contract capacity
/// 
/// # Arguments
/// * `deposit_amount` - Amount to deposit (in stroops)
/// * `current_deposited` - Current total deposited amount (in stroops)
/// * `max_contract_total` - Maximum allowed per contract (in stroops)
/// 
/// # Returns
/// `Ok(())` if valid, `Err(AmountValidationError)` if invalid
pub fn validate_deposit_amount(
    deposit_amount: i128,
    current_deposited: i128,
    max_contract_total: i128
) -> Result<(), AmountValidationError> {
    // Validate deposit amount itself
    validate_single_amount(deposit_amount)?;

    // Check if deposit would exceed contract maximum
    if let Some(new_total) = current_deposited.checked_add(deposit_amount) {
        if new_total > max_contract_total {
            return Err(AmountValidationError::ExceedsContractMaximum);
        }
    } else {
        return Err(AmountValidationError::PotentialOverflow);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_single_amount() {
        // Valid amounts
        assert!(validate_single_amount(1).is_ok());
        assert!(validate_single_amount(100_0000000).is_ok()); // 1 token
        assert!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS).is_ok());

        // Invalid amounts
        assert_eq!(validate_single_amount(0), Err(AmountValidationError::NonPositiveAmount));
        assert_eq!(validate_single_amount(-1), Err(AmountValidationError::NonPositiveAmount));
        assert_eq!(
            validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS + 1), 
            Err(AmountValidationError::AmountExceedsMaximum)
        );
    }

    #[test]
    fn test_validate_amount_array() {
        // Valid arrays
        let amounts1 = [100_0000000, 200_0000000, 300_0000000];
        assert!(validate_amount_array(&amounts1).is_ok());
        assert_eq!(validate_amount_array(&amounts1).unwrap(), 600_0000000);

        // Arrays with invalid amounts
        let amounts2 = [100_0000000, 0, 300_0000000];
        assert_eq!(validate_amount_array(&amounts2), Err(AmountValidationError::NonPositiveAmount));

        let amounts3 = [100_0000000, -50_0000000, 300_0000000];
        assert_eq!(validate_amount_array(&amounts3), Err(AmountValidationError::NonPositiveAmount));
    }

    #[test]
    fn test_validate_contract_total() {
        let max_total = 1_000_000_0000000; // 1M tokens

        // Valid totals
        assert!(validate_contract_total(100_0000000, max_total).is_ok());
        assert!(validate_contract_total(max_total, max_total).is_ok());

        // Invalid totals
        assert_eq!(
            validate_contract_total(max_total + 1, max_total),
            Err(AmountValidationError::ExceedsContractMaximum)
        );
    }

    #[test]
    fn test_validate_milestone_amounts() {
        let max_contract_total = 1_000_000_0000000;

        // Valid milestone amounts
        let milestones1 = [100_0000000, 200_0000000, 300_0000000];
        assert!(validate_milestone_amounts(&milestones1, max_contract_total).is_ok());

        // Invalid due to contract maximum
        let milestones2 = [500_000_0000000, 600_000_0000000]; // 5M + 6M > 1M max
        assert_eq!(
            validate_milestone_amounts(&milestones2, max_contract_total),
            Err(AmountValidationError::ExceedsContractMaximum)
        );
    }

    #[test]
    fn test_validate_deposit_amount() {
        let max_contract_total = 1_000_000_0000000;

        // Valid deposit
        assert!(validate_deposit_amount(100_0000000, 0, max_contract_total).is_ok());
        assert!(validate_deposit_amount(100_0000000, 500_0000000, max_contract_total).is_ok());

        // Invalid - would exceed maximum
        assert_eq!(
            validate_deposit_amount(600_000_0000000, 500_000_0000000, max_contract_total),
            Err(AmountValidationError::ExceedsContractMaximum)
        );
    }

    #[test]
    fn test_safe_arithmetic() {
        // Safe addition
        assert_eq!(safe_add_amounts(100, 200), Some(300));
        assert_eq!(safe_add_amounts(i128::MAX, 1), None);

        // Safe subtraction
        assert_eq!(safe_subtract_amounts(300, 100), Some(200));
        assert_eq!(safe_subtract_amounts(0, 1), None);
    }
}
