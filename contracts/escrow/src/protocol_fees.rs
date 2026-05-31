//! Protocol fee calculation and management helpers

use soroban_sdk::{Env, Address};
use crate::{DataKey, Error};

/// Maximum allowed protocol fee in basis points (10% = 1000 bps)
pub const MAX_PROTOCOL_FEE_BPS: u32 = 1000;

/// Calculates the protocol fee for a given amount
///
/// # Arguments
/// * `amount` - The amount to calculate the fee for (in stroops)
/// * `fee_bps` - The protocol fee in basis points
///
/// # Returns
/// The protocol fee amount (in stroops), rounded down
pub fn calculate_protocol_fee(amount: i128, fee_bps: u32) -> i128 {
    (amount * fee_bps as i128) / 10_000
}

/// Gets the current protocol fee in basis points from storage
///
/// # Arguments
/// * `env` - The contract environment
///
/// # Returns
/// The current protocol fee in basis points, defaults to 0 if not set
pub fn get_protocol_fee_bps(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::ProtocolFeeBps)
        .unwrap_or(0)
}

/// Gets the accumulated protocol fees from storage
///
/// # Arguments
/// * `env` - The contract environment
///
/// # Returns
/// The accumulated protocol fees in stroops, defaults to 0 if not set
pub fn get_accumulated_protocol_fees(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::AccumulatedProtocolFees)
        .unwrap_or(0)
}

/// Adds an amount to the accumulated protocol fees
///
/// # Arguments
/// * `env` - The contract environment
/// * `fee_amount` - The amount to add to accumulated fees (in stroops)
pub fn add_to_accumulated_fees(env: &Env, fee_amount: i128) {
    let current = get_accumulated_protocol_fees(env);
    env.storage()
        .persistent()
        .set(&DataKey::AccumulatedProtocolFees, &(current + fee_amount));
}

/// Verifies that the caller is the admin
///
/// # Arguments
/// * `env` - The contract environment
/// * `caller` - The address to verify
///
/// # Errors
/// * `Error::UnauthorizedRole` if the caller is not the admin
pub fn require_admin(env: &Env, caller: &Address) {
    let admin: Address = env.storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
    if *caller != admin {
        env.panic_with_error(Error::UnauthorizedRole);
    }
    caller.require_auth();
}
