//! Bounds validation for storage entrypoint inputs.
//!
//! This module extracts numeric and length bound checks for storage-mutating
//! entrypoints into a single source of truth. Each function validates one
//! logical parameter and panics with the appropriate typed [`EscrowError`]
//! on rejection.
//!
//! All functions are pure (no side-effects) and intended to be called at the
//! top of the corresponding entrypoint, before any state mutation occurs.

use crate::milestones_consts::{
    MAX_FEE_BPS, MAX_MILESTONES, MAX_RATING, MAX_REPUTATION_CONFIG_RATING_CEILING,
    MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING, MIN_COMMENT_BYTES, MIN_RATING,
};
use crate::{Error, EscrowError};
use soroban_sdk::Env;

/// Validate the governed total escrow cap in stroops.
///
/// # Accepted values
/// * Any `i128` in `(0, i128::MAX]`.
///
/// # Rejected values
/// * `0` — a zero cap would block every contract creation.
/// * Negative values — amounts must be positive.
///
/// # Panics
/// Panics with [`Error::InvalidProtocolParameters`] when the cap is out
/// of range.
pub(crate) fn validate_escrow_total_cap(env: &Env, max_escrow_total_stroops: i128) {
    if max_escrow_total_stroops <= 0 {
        env.panic_with_error(Error::InvalidProtocolParameters);
    }
}

/// Validate reputation configuration parameters.
///
/// # Accepted values
/// * `min_rating` in `[1, 10]`
/// * `max_rating` in `[min_rating, 10]`
/// * `max_comment_bytes` in `[1, 1_000]`
///
/// # Panics
/// Panics with [`Error::InvalidProtocolParameters`] when any bound is violated.
pub(crate) fn validate_reputation_config_params(
    env: &Env,
    min_rating: u32,
    max_rating: u32,
    max_comment_bytes: u32,
) {
    if min_rating < MIN_RATING
        || max_rating < min_rating
        || max_rating > MAX_REPUTATION_CONFIG_RATING_CEILING
        || max_comment_bytes < MIN_COMMENT_BYTES
        || max_comment_bytes > MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING
    {
        env.panic_with_error(Error::InvalidProtocolParameters);
    }
}

/// Validate the number of milestones for a contract creation call.
///
/// # Accepted values
/// * `count` in `[1, MAX_MILESTONES]`
///
/// # Rejected values
/// * `0` — at least one milestone is required.
/// * Values > `MAX_MILESTONES` (10).
///
/// # Panics
/// Panics with [`EscrowError::EmptyMilestones`] when `count == 0` or
/// [`EscrowError::TooManyMilestones`] when `count > MAX_MILESTONES`.
pub(crate) fn validate_milestone_count(env: &Env, count: u32) {
    if count == 0 {
        env.panic_with_error(EscrowError::EmptyMilestones);
    }
    if count > MAX_MILESTONES {
        env.panic_with_error(EscrowError::TooManyMilestones);
    }
}

/// Validate a protocol fee basis-points value.
///
/// # Accepted values
/// * `bps` in `[0, MAX_FEE_BPS]` (0–10 000).
///
/// # Panics
/// Panics with [`Error::InvalidProtocolParameters`] when `bps > MAX_FEE_BPS`.
pub(crate) fn validate_protocol_fee_bps(env: &Env, bps: u32) {
    if bps > MAX_FEE_BPS {
        env.panic_with_error(Error::InvalidProtocolParameters);
    }
}

/// Validate a single stroop amount for positivity and maximum bounds.
///
/// # Accepted values
/// * `amount` in `(0, MAX_SINGLE_AMOUNT_STROOPS]`.
///
/// # Panics
/// Panics with [`EscrowError::AmountMustBePositive`] when `amount <= 0` or
/// [`EscrowError::InvalidMilestoneAmount`] when the amount exceeds the cap.
pub(crate) fn validate_stroop_amount(env: &Env, amount: i128) {
    if amount <= 0 {
        env.panic_with_error(crate::EscrowError::AmountMustBePositive);
    }
    if amount > crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS {
        env.panic_with_error(crate::EscrowError::InvalidMilestoneAmount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    fn env() -> Env {
        Env::default()
    }

    // ── validate_escrow_total_cap ────────────────────────────────────────────

    #[test]
    fn validate_escrow_total_cap_accepts_1() {
        let e = env();
        validate_escrow_total_cap(&e, 1);
    }

    #[test]
    fn validate_escrow_total_cap_accepts_i128_max() {
        let e = env();
        validate_escrow_total_cap(&e, i128::MAX);
    }

    #[test]
    #[should_panic]
    fn validate_escrow_total_cap_rejects_zero() {
        let e = env();
        validate_escrow_total_cap(&e, 0);
    }

    #[test]
    #[should_panic]
    fn validate_escrow_total_cap_rejects_negative() {
        let e = env();
        validate_escrow_total_cap(&e, -1);
    }

    #[test]
    #[should_panic]
    fn validate_escrow_total_cap_rejects_i128_min() {
        let e = env();
        validate_escrow_total_cap(&e, i128::MIN);
    }

    // ── validate_reputation_config_params ─────────────────────────────────────

    #[test]
    fn validate_reputation_config_params_accepts_default() {
        let e = env();
        validate_reputation_config_params(&e, 1, 5, 200);
    }

    #[test]
    fn validate_reputation_config_params_accepts_min_equal_max_rating() {
        let e = env();
        validate_reputation_config_params(&e, 3, 3, 1);
    }

    #[test]
    fn validate_reputation_config_params_accepts_max_comment_1000() {
        let e = env();
        validate_reputation_config_params(&e, 1, 10, 1_000);
    }

    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_zero_min_rating() {
        let e = env();
        validate_reputation_config_params(&e, 0, 5, 200);
    }

    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_max_below_min() {
        let e = env();
        validate_reputation_config_params(&e, 5, 3, 200);
    }

    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_max_rating_over_10() {
        let e = env();
        validate_reputation_config_params(&e, 1, 11, 200);
    }

    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_zero_comment_bytes() {
        let e = env();
        validate_reputation_config_params(&e, 1, 5, 0);
    }

    #[test]
    #[should_panic]
    fn validate_reputation_config_params_rejects_comment_over_1000() {
        let e = env();
        validate_reputation_config_params(&e, 1, 5, 1_001);
    }

    // ── validate_milestone_count ──────────────────────────────────────────────

    #[test]
    fn validate_milestone_count_accepts_1() {
        let e = env();
        validate_milestone_count(&e, 1);
    }

    #[test]
    fn validate_milestone_count_accepts_max() {
        let e = env();
        validate_milestone_count(&e, MAX_MILESTONES);
    }

    #[test]
    #[should_panic]
    fn validate_milestone_count_rejects_zero() {
        let e = env();
        validate_milestone_count(&e, 0);
    }

    #[test]
    #[should_panic]
    fn validate_milestone_count_rejects_over_max() {
        let e = env();
        validate_milestone_count(&e, MAX_MILESTONES + 1);
    }

    #[test]
    #[should_panic]
    fn validate_milestone_count_rejects_u32_max() {
        let e = env();
        validate_milestone_count(&e, u32::MAX);
    }

    // ── validate_protocol_fee_bps ─────────────────────────────────────────────

    #[test]
    fn validate_protocol_fee_bps_accepts_zero() {
        let e = env();
        validate_protocol_fee_bps(&e, 0);
    }

    #[test]
    fn validate_protocol_fee_bps_accepts_max() {
        let e = env();
        validate_protocol_fee_bps(&e, MAX_FEE_BPS);
    }

    #[test]
    #[should_panic]
    fn validate_protocol_fee_bps_rejects_over_max() {
        let e = env();
        validate_protocol_fee_bps(&e, MAX_FEE_BPS + 1);
    }

    #[test]
    #[should_panic]
    fn validate_protocol_fee_bps_rejects_u32_max() {
        let e = env();
        validate_protocol_fee_bps(&e, u32::MAX);
    }

    // ── validate_stroop_amount ────────────────────────────────────────────────

    #[test]
    fn validate_stroop_amount_accepts_1() {
        let e = env();
        validate_stroop_amount(&e, 1);
    }

    #[test]
    fn validate_stroop_amount_accepts_max() {
        let e = env();
        validate_stroop_amount(&e, crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS);
    }

    #[test]
    #[should_panic]
    fn validate_stroop_amount_rejects_zero() {
        let e = env();
        validate_stroop_amount(&e, 0);
    }

    #[test]
    #[should_panic]
    fn validate_stroop_amount_rejects_negative() {
        let e = env();
        validate_stroop_amount(&e, -1);
    }

    #[test]
    #[should_panic]
    fn validate_stroop_amount_rejects_over_max() {
        let e = env();
        validate_stroop_amount(&e, crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS + 1);
    }
}
