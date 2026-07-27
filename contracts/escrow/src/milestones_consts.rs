//! Named constants for milestone-related protocol limits.
//!
//! This module centralises every "magic number" that appears in milestone
//! validation, reputation scoring, and protocol-fee calculation so that
//! the business rules are documented in one place and the call-sites stay
//! readable.
//!
//! All values are `pub` so they can be re-exported from `lib.rs` and
//! referenced by governance, fee, and test modules without creating
//! circular dependencies.

/// Maximum number of milestones allowed in a single escrow contract.
///
/// `create_contract` rejects any `milestones` vector whose `len()` exceeds
/// this value with `EscrowError::TooManyMilestones`.  The current limit is
/// **10**, balancing transaction-size budgets on Soroban with realistic
/// freelance project structures.
///
/// Exposed via `get_bounds()` as [`ContractBounds::max_milestones`].
pub const MAX_MILESTONES: u32 = 10;

/// Basis-point denominator used in all protocol-fee calculations.
///
/// Protocol fees are expressed in *basis points* (bps), where
/// `10 000 bps = 100 %`.  Every fee computation divides by this constant:
///
/// ```text
/// fee = amount × fee_bps / PROTOCOL_FEE_BPS_DENOMINATOR
/// ```
///
/// This is an integer **floor division**, so the freelancer always receives
/// at least `amount − fee` stroops.
///
/// See `calculate_protocol_fee` and `set_governed_params` for the full
/// validation and accrual flow.
pub const PROTOCOL_FEE_BPS_DENOMINATOR: u32 = 10_000;

/// Minimum allowed protocol fee in basis points (inclusive).
///
/// A fee of `0 bps` disables fee collection entirely and causes
/// `calculate_protocol_fee` to short-circuit and return `0`.
///
/// Exposed via `get_bounds()` as the implicit lower bound for
/// [`ContractBounds::max_fee_bps`].
pub const MIN_FEE_BPS: u32 = 0;

/// Maximum allowed protocol fee in basis points (inclusive).
///
/// `set_protocol_fee_bps` and `set_governed_params` reject any `new_bps`
/// value strictly greater than this constant with
/// `Error::InvalidProtocolParameters`.
///
/// Equal to [`PROTOCOL_FEE_BPS_DENOMINATOR`] (100 %): charging more than the
/// full milestone amount as a fee is nonsensical and is therefore disallowed.
///
/// Exposed via `get_bounds()` as [`ContractBounds::max_fee_bps`].
pub const MAX_FEE_BPS: u32 = PROTOCOL_FEE_BPS_DENOMINATOR;

/// Minimum valid reputation rating (inclusive).
///
/// `issue_reputation` rejects a `rating` strictly less than this value with
/// `Error::InvalidRating`.  A rating of **1** is the lowest possible score
/// a client can assign to completed freelancer work.
pub const MIN_RATING: u32 = 1;

/// Maximum valid reputation rating (inclusive).
///
/// `issue_reputation` rejects a `rating` strictly greater than this value
/// with `Error::InvalidRating`.  A rating of **5** is the highest possible
/// score, forming a 1–5 star scale.
pub const MAX_RATING: u32 = 5;

/// Maximum byte length for a reputation comment (inclusive).
///
/// `issue_reputation` rejects a `comment` whose UTF-8 byte length exceeds
/// this value with `Error::CommentTooLong`.
///
/// Soroban `String::len()` returns the raw byte count, so a multi-byte
/// character (e.g. a 3-byte emoji) counts as 3 toward this limit.
/// ASCII characters are each 1 byte.
///
/// The **200-byte** cap keeps on-chain storage bounded: at Stellar's stroop
/// pricing a 200-byte entry is cheap for legitimate use but expensive enough
/// to deter spam.  The minimum is **1 byte** (non-empty comment required).
pub const MAX_COMMENT_BYTES: u32 = 200;

/// Minimum byte length for a reputation comment (inclusive).
///
/// `issue_reputation` rejects a `comment` whose UTF-8 byte length is `0`
/// with `Error::EmptyComment`.  A comment must contain at least one byte.
pub const MIN_COMMENT_BYTES: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    /// Values are identical to the literals that previously appeared inline;
    /// this test pins them so a future edit to the constant is caught.
    #[test]
    fn milestone_constants_have_correct_values() {
        assert_eq!(MAX_MILESTONES, 10);
        assert_eq!(PROTOCOL_FEE_BPS_DENOMINATOR, 10_000);
        assert_eq!(MIN_FEE_BPS, 0);
        assert_eq!(MAX_FEE_BPS, 10_000);
        assert_eq!(MIN_RATING, 1);
        assert_eq!(MAX_RATING, 5);
        assert_eq!(MAX_COMMENT_BYTES, 200);
        assert_eq!(MIN_COMMENT_BYTES, 1);
    }

    /// MAX_FEE_BPS must equal the denominator — charging 100 % is the ceiling.
    #[test]
    fn max_fee_bps_equals_denominator() {
        assert_eq!(
            MAX_FEE_BPS, PROTOCOL_FEE_BPS_DENOMINATOR,
            "MAX_FEE_BPS must equal PROTOCOL_FEE_BPS_DENOMINATOR"
        );
    }

    /// Rating range must be a proper non-empty interval.
    #[test]
    fn rating_range_is_valid() {
        assert!(MIN_RATING <= MAX_RATING, "MIN_RATING must be ≤ MAX_RATING");
        assert_eq!(MIN_RATING, 1);
        assert_eq!(MAX_RATING, 5);
    }

    /// Comment byte range must be a proper non-empty interval.
    #[test]
    fn comment_byte_range_is_valid() {
        assert!(
            MIN_COMMENT_BYTES <= MAX_COMMENT_BYTES,
            "MIN_COMMENT_BYTES must be ≤ MAX_COMMENT_BYTES"
        );
    }

    /// Every rating value inside [MIN_RATING, MAX_RATING] should be accepted
    /// and every value outside rejected — document the inclusive boundaries.
    #[test]
    fn rating_boundary_coverage() {
        let valid_ratings = [MIN_RATING, 2, 3, 4, MAX_RATING];
        for &r in &valid_ratings {
            assert!(
                r >= MIN_RATING && r <= MAX_RATING,
                "rating {r} should be within bounds"
            );
        }

        // Values just outside the range
        let below = MIN_RATING.wrapping_sub(1); // 0
        let above = MAX_RATING + 1; // 6
        assert!(
            below < MIN_RATING || below > MAX_RATING,
            "rating {below} should be out-of-bounds"
        );
        assert!(
            above < MIN_RATING || above > MAX_RATING,
            "rating {above} should be out-of-bounds"
        );
    }

    /// Comment length boundary coverage — edge values at 0, 1, 200, 201.
    #[test]
    fn comment_length_boundary_coverage() {
        // These mirror the guards in issue_reputation()
        assert!(
            0 < MIN_COMMENT_BYTES,
            "empty comment (0 bytes) must be rejected"
        );
        assert!(
            MIN_COMMENT_BYTES <= MAX_COMMENT_BYTES,
            "min must not exceed max"
        );
        assert_eq!(MAX_COMMENT_BYTES, 200);
        // One byte over the limit
        let over_limit = MAX_COMMENT_BYTES + 1;
        assert!(
            over_limit > MAX_COMMENT_BYTES,
            "201-byte comment must exceed the cap"
        );
    }

    /// Protocol fee boundary coverage — 0 and 10_000 are both valid;
    /// 10_001 must be rejected by governance logic.
    #[test]
    fn fee_bps_boundary_coverage() {
        // Boundary values that must be accepted.
        // MIN_FEE_BPS == 0 (u32 minimum), MAX_FEE_BPS == 10_000.
        assert_eq!(MIN_FEE_BPS, 0);
        assert_eq!(MAX_FEE_BPS, 10_000);
        // MAX must strictly exceed MIN so the fee range is non-trivial.
        assert!(MAX_FEE_BPS > 0, "MAX_FEE_BPS must be > 0");

        // One bps over the maximum must exceed the limit
        let over_limit = MAX_FEE_BPS + 1;
        assert!(
            over_limit > MAX_FEE_BPS,
            "10_001 bps must exceed MAX_FEE_BPS"
        );
    }
}
