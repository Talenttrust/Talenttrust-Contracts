//! Token scale persistence, validation, and normalized-value helpers (#1346).
//!
//! ## Why token scale matters
//!
//! Soroban SAC tokens carry an administrator-configured `decimals` field that
//! controls how raw on-chain amounts (stroops or equivalent sub-units) map to
//! human-visible values.  For example, a token with `decimals = 7` means that
//! `10_000_000` raw units equal `1.0` visible token.
//!
//! Without capturing and enforcing this scale, two failure modes exist:
//!
//! 1. **Misinterpretation** — a client denominating amounts in visible tokens
//!    instead of raw units creates contracts whose milestone amounts are
//!    `10^decimals` times too small.
//! 2. **Fractional remainder** — an amount that is not a whole multiple of
//!    `10^decimals` cannot be represented precisely as a visible-token value,
//!    so it silently loses the fractional remainder on display.
//!
//! ## Design
//!
//! * The scale (decimal count) is captured **once** at `bind_settlement_token`
//!   time by calling `token::Client::decimals()` and stored under
//!   [`DataKey::TokenScale`].
//! * Every milestone amount submitted to `create_contract` is validated with
//!   [`require_exact_scale`] to ensure it is exactly representable.
//! * [`get_token_scale`] and [`get_normalized_amount`] are read-only contract
//!   entrypoints for off-chain clients.
//!
//! ## Security assumptions
//!
//! * `decimals()` is a read-only probe — it cannot mutate the token contract
//!   or trigger re-entrancy (no transfer path is exercised).
//! * The scale is captured atomically with the token binding; a subsequent
//!   change to the token's `decimals` field on-chain would not affect the
//!   recorded value (it is stored by value, not by reference).
//! * Milestones are always stored and transferred as raw on-chain units.
//!   Normalization is read-only and only affects the view layer.

use crate::{DataKey, Error};
use soroban_sdk::{token, Address, Env};

/// Maximum allowed decimal places.  SAC tokens cap at 18; we use 18 as the
/// upper bound to future-proof against non-SAC tokens while keeping the
/// power-of-ten computation in `i128` range (10^18 < i128::MAX).
pub const MAX_TOKEN_DECIMALS: u32 = 18;

// ── Storage helpers ────────────────────────────────────────────────────────

/// Read the token decimal count from persistent storage.
///
/// Returns `None` when no scale has been recorded yet (i.e.
/// `bind_settlement_token` has not been called or the token does not
/// implement `decimals()`).
pub fn read_token_scale(env: &Env) -> Option<u32> {
    env.storage().persistent().get(&DataKey::TokenScale)
}

/// Persist the token decimal count under [`DataKey::TokenScale`].
///
/// This is called once at bind time and should not be called again.
pub fn write_token_scale(env: &Env, decimals: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::TokenScale, &decimals);
}

/// Return the recorded token scale, panicking with [`Error::TokenScaleNotSet`]
/// when absent.
pub fn require_token_scale(env: &Env) -> u32 {
    read_token_scale(env).unwrap_or_else(|| env.panic_with_error(Error::TokenScaleNotSet))
}

// ── Scale capture ────────────────────────────────────────────────────────────

/// Query the token contract for its decimal count and persist it.
///
/// Called once inside `bind_settlement_token`.  The probe is read-only
/// (`decimals()` does not transfer funds or mutate state on the token) so
/// no re-entrancy risk exists.
///
/// # Security
///
/// `decimals()` is spec'd as a pure read.  If the token does not implement
/// it the host will panic, which is treated the same as an invalid token.
pub fn capture_and_store_token_scale(env: &Env, token: &Address) {
    let client = token::Client::new(env, token);
    let decimals = client.decimals();
    if decimals > MAX_TOKEN_DECIMALS {
        env.panic_with_error(Error::InvalidProtocolParameters);
    }
    write_token_scale(env, decimals);
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Compute `10^decimals` using saturating arithmetic, capped at `i128::MAX`.
///
/// For `decimals == 0` the multiplier is `1` (every integer is representable).
/// For `decimals == 7` (standard SAC stroops) the multiplier is `10_000_000`.
pub fn scale_multiplier(decimals: u32) -> i128 {
    // 10^18 fits in i128 (max ~1.7 * 10^38), so decimals <= MAX_TOKEN_DECIMALS
    // is always safe.
    let mut result: i128 = 1;
    for _ in 0..decimals {
        result = result.saturating_mul(10);
    }
    result
}

/// Validate that `amount` is exactly representable at `decimals` scale.
///
/// An amount is representable when `amount % 10^decimals == 0`, i.e. there is
/// no fractional remainder when the raw amount is converted to the visible-token
/// unit.
///
/// # Arguments
///
/// * `env`      — Soroban environment (for panicking on failure).
/// * `amount`   — Raw on-chain amount (stroops or equivalent sub-units).
/// * `decimals` — Number of decimal places the token uses.
///
/// # Errors
///
/// Panics with [`Error::FractionalTokenAmount`] when `amount % multiplier != 0`.
pub fn require_exact_scale(env: &Env, amount: i128, decimals: u32) {
    if decimals == 0 {
        // Every integer is representable when there are no decimal places.
        return;
    }
    let multiplier = scale_multiplier(decimals);
    if amount % multiplier != 0 {
        env.panic_with_error(Error::FractionalTokenAmount);
    }
}

/// Validate all milestone amounts in a slice for exact representability.
///
/// Iterates over each amount and calls [`require_exact_scale`].  The first
/// non-representable amount causes a panic with [`Error::FractionalTokenAmount`].
///
/// # Arguments
///
/// * `env`      — Soroban environment.
/// * `amounts`  — Iterator of raw on-chain amounts.
/// * `decimals` — Recorded token decimal count.
pub fn require_all_exact_scale<'a, I>(env: &Env, amounts: I, decimals: u32)
where
    I: IntoIterator<Item = &'a i128>,
{
    for &amount in amounts {
        require_exact_scale(env, amount, decimals);
    }
}

// ── Normalization ─────────────────────────────────────────────────────────────

/// Convert a raw on-chain amount to its normalized (human-visible) representation.
///
/// Returns the integer part of `amount / 10^decimals`.  The result is always an
/// integer because we require exact representability before storage (see
/// [`require_exact_scale`]).  Fractional amounts are therefore never stored, so
/// the division is always exact.
///
/// # Arguments
///
/// * `amount`   — Raw on-chain amount (must be exactly representable).
/// * `decimals` — Number of decimal places the token uses.
///
/// # Returns
///
/// `amount / 10^decimals` (integer division; remainder is always zero for
/// amounts that passed [`require_exact_scale`]).
///
/// # Examples
///
/// ```
/// use escrow::token_scale::normalized_amount;
/// // 10_000_000 stroops with 7 decimals → 1 token
/// assert_eq!(normalized_amount(10_000_000, 7), 1);
/// // 500_000_000 stroops → 50 tokens
/// assert_eq!(normalized_amount(500_000_000, 7), 50);
/// // Zero decimals: amount is already the normalized value
/// assert_eq!(normalized_amount(42, 0), 42);
/// ```
pub fn normalized_amount(amount: i128, decimals: u32) -> i128 {
    if decimals == 0 {
        return amount;
    }
    amount / scale_multiplier(decimals)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn scale_multiplier_zero_decimals() {
        assert_eq!(scale_multiplier(0), 1);
    }

    #[test]
    fn scale_multiplier_seven_decimals() {
        assert_eq!(scale_multiplier(7), 10_000_000);
    }

    #[test]
    fn scale_multiplier_max_decimals() {
        // 10^18 must fit in i128 without overflow.
        let m = scale_multiplier(18);
        assert_eq!(m, 1_000_000_000_000_000_000_i128);
    }

    #[test]
    fn normalized_amount_zero_decimals() {
        // With 0 decimals the raw amount is the visible amount.
        assert_eq!(normalized_amount(42, 0), 42);
        assert_eq!(normalized_amount(1, 0), 1);
        assert_eq!(normalized_amount(0, 0), 0);
    }

    #[test]
    fn normalized_amount_seven_decimals() {
        assert_eq!(normalized_amount(10_000_000, 7), 1);
        assert_eq!(normalized_amount(500_000_000, 7), 50);
        assert_eq!(normalized_amount(1_000_000_0000000_i128, 7), 1_000_000);
    }

    #[test]
    fn normalized_amount_two_decimals() {
        // e.g. a cents-based token where 100 raw units = 1 visible token
        assert_eq!(normalized_amount(100, 2), 1);
        assert_eq!(normalized_amount(1_000, 2), 10);
    }
}
