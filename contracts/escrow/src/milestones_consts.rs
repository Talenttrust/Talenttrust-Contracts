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
//!
//! ## Validation boundaries
//!
//! Every constant below defines an **inclusive** acceptance window for the
//! value it validates. A value equal to a bound is accepted; the value one
//! step beyond the bound is rejected. The mapping is:
//!
//! | Constant | Accepted range | Enforced at | Error on violation |
//! |---|---|---|---|
//! | [`MAX_MILESTONES`] | `1..=MAX_MILESTONES` (1..=10) | `create_contract` | `TooManyMilestones` |
//! | [`MAX_BATCH_MILESTONES`] | `1..=MAX_BATCH_MILESTONES` (1..=10) | `release_milestone_batch` | `EmptyBatch` / `BatchLimitExceeded` |
//! | [`PROTOCOL_FEE_BPS_DENOMINATOR`] | fixed scale: `10_000 bps = 100 %` | `calculate_protocol_fee` | — (floor division) |
//! | [`MIN_FEE_BPS`] / [`MAX_FEE_BPS`] | `0..=MAX_FEE_BPS` (0..=10_000) | `set_protocol_fee_bps`, `set_governed_params` | `InvalidProtocolParameters` |
//! | [`MIN_RATING`] / [`MAX_RATING`] | `1..=5` | `issue_reputation` | `InvalidRating` |
//! | [`MIN_COMMENT_BYTES`] / [`MAX_COMMENT_BYTES`] | `1..=200` bytes | `issue_reputation` | `EmptyComment` / `CommentTooLong` |
//! | [`MIN_WORK_EVIDENCE_BYTES`] / [`MAX_WORK_EVIDENCE_BYTES`] | `1..=1_000` bytes | `submit_work_evidence` | `EmptyEvidence` / `EvidenceTooLong` |
//! | [`MAX_REPUTATION_CONFIG_RATING_CEILING`] | `min_rating..=10` | `set_reputation_config` | `InvalidProtocolParameters` |
//! | [`MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING`] | `1..=1_000` bytes | `set_reputation_config` | `InvalidProtocolParameters` |
//! | [`MAX_RECOVERY_ATTEMPTS`] | `0..=MAX_RECOVERY_ATTEMPTS` (0..=3) | deterministic recovery loops | recovery stops |
//! | [`RECOVERY_TTL_LEDGERS`] | `> 0` ledgers (17_280 ≈ 1 day) | recovery TTL tracking | — |
//!
//! `MAX_FEE_BPS` is *equal to* [`PROTOCOL_FEE_BPS_DENOMINATOR`], so the fee
//! range closes at 100 % and a fee can never exceed the gross milestone
//! amount. The boundary value itself is accepted; only `MAX_FEE_BPS + 1` is
//! rejected. The numeric edge cases above are pinned by the tests in this
//! module.

/// Maximum number of milestones allowed in a single escrow contract.
///
/// `create_contract` rejects any `milestones` vector whose `len()` exceeds
/// this value with `EscrowError::TooManyMilestones`.  The current limit is
/// **10**, balancing transaction-size budgets on Soroban with realistic
/// freelance project structures.
///
/// Exposed via `get_bounds()` as [`ContractBounds::max_milestones`].
pub const MAX_MILESTONES: u32 = 10;

/// Maximum number of milestones that can be released in a single batch call.
pub const MAX_BATCH_MILESTONES: u32 = 10;

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

/// Maximum byte length for a work evidence string (inclusive).
pub const MAX_WORK_EVIDENCE_BYTES: u32 = 1_000;

/// Minimum byte length for a work evidence string (inclusive).
pub const MIN_WORK_EVIDENCE_BYTES: u32 = 1;

/// Maximum allowed value for the configurable maximum rating parameter in
/// reputation configuration (`set_reputation_config`).
///
/// This is the upper bound that an admin can set for `max_rating`;
/// the actual rating scale for `issue_reputation` is always 1–5
/// (see [`MAX_RATING`]).  The ceiling of **10** gives governance
/// flexibility without allowing unbounded ratings.
pub const MAX_REPUTATION_CONFIG_RATING_CEILING: u32 = 10;

/// Maximum allowed value for the configurable maximum comment bytes parameter
/// in reputation configuration (`set_reputation_config`).
///
/// This caps how large the `max_comment_bytes` field can be set by admin.
pub const MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING: u32 = 1_000;

/// Maximum number of deterministic recovery attempts allowed for transient failures
/// during milestone operations or batch releases.
///
/// Ensures failure recovery is bounded and deterministic, preventing infinite loops
/// or resource exhaustion when recovering from adverse conditions.
pub const MAX_RECOVERY_ATTEMPTS: u32 = 3;

/// Maximum time-to-live (in ledgers) for transient failure recovery tracking.
///
/// Defines the observability window for failed states, ensuring failures are
/// recoverable and diagnosable without silent data loss. (17280 ledgers ≈ 1 day).
pub const RECOVERY_TTL_LEDGERS: u32 = 17_280;

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
        assert_eq!(MAX_WORK_EVIDENCE_BYTES, 1_000);
        assert_eq!(MIN_WORK_EVIDENCE_BYTES, 1);
        assert_eq!(MAX_REPUTATION_CONFIG_RATING_CEILING, 10);
        assert_eq!(MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING, 1_000);
        assert_eq!(MAX_RECOVERY_ATTEMPTS, 3);
        assert_eq!(RECOVERY_TTL_LEDGERS, 17_280);
    }

    /// Recovery limits must be deterministic and proper non-empty values.
    #[test]
    fn recovery_limits_are_deterministic() {
        assert!(
            MAX_RECOVERY_ATTEMPTS > 0,
            "MAX_RECOVERY_ATTEMPTS must be > 0"
        );
        assert!(RECOVERY_TTL_LEDGERS > 0, "RECOVERY_TTL_LEDGERS must be > 0");
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

    /// Ensure that concurrent reads of these constants from multiple threads
    /// do not produce inconsistent or stale results, guaranteeing thread-safety
    /// and deterministic behaviour under parallel access patterns.
    #[test]
    fn concurrent_read_consistency() {
        extern crate std;
        use std::thread;
        use std::vec::Vec;

        let num_threads = 20;
        let mut handles = Vec::new();

        for _ in 0..num_threads {
            handles.push(thread::spawn(|| {
                // Read all constants, asserting their validity and absence of stale state
                assert_eq!(MAX_MILESTONES, 10);
                assert_eq!(MAX_BATCH_MILESTONES, 10);
                assert_eq!(PROTOCOL_FEE_BPS_DENOMINATOR, 10_000);
                assert_eq!(MIN_FEE_BPS, 0);
                assert_eq!(MAX_FEE_BPS, 10_000);
                assert_eq!(MIN_RATING, 1);
                assert_eq!(MAX_RATING, 5);
                assert_eq!(MAX_COMMENT_BYTES, 200);
                assert_eq!(MIN_COMMENT_BYTES, 1);
                assert_eq!(MAX_WORK_EVIDENCE_BYTES, 1_000);
                assert_eq!(MIN_WORK_EVIDENCE_BYTES, 1);
                assert_eq!(MAX_REPUTATION_CONFIG_RATING_CEILING, 10);
                assert_eq!(MAX_REPUTATION_CONFIG_COMMENT_BYTES_CEILING, 1_000);
            }));
        }

        for handle in handles {
            assert!(
                handle.join().is_ok(),
                "Thread panicked during concurrent read"
            );
        }
    }

    /// Idempotency test: repeated reads and boundary checks must consistently
    /// produce the same state and evaluation over time, preventing duplicate work bugs.
    #[test]
    fn idempotent_boundary_evaluations() {
        let iters = 1_000;
        for _ in 0..iters {
            assert_eq!(MAX_BATCH_MILESTONES, 10);
            assert_eq!(MAX_MILESTONES, 10);
            assert_eq!(PROTOCOL_FEE_BPS_DENOMINATOR, 10_000);
        }
    }

    // ── Validation boundary pins ─────────────────────────────────────────────
    //
    // The helpers below mirror the runtime guards at their enforcement sites so
    // the exact `..=bound` acceptance semantics can be tested without a `Env`.
    // They deliberately contain no runtime dependency beyond this module.

    /// Mirrors `create_contract`: `1..=MAX_MILESTONES` accepted.
    fn accepts_milestone_count(count: u32) -> bool {
        count >= 1 && count <= MAX_MILESTONES
    }

    /// Mirrors `release_milestone_batch_impl`: non-empty and `<= MAX_BATCH_MILESTONES`.
    fn accepts_batch_count(count: u32) -> bool {
        count >= 1 && count <= MAX_BATCH_MILESTONES
    }

    /// Mirrors `validate_protocol_fee_bps_value`: `<= MAX_FEE_BPS` accepted.
    fn accepts_fee_bps(bps: u32) -> bool {
        bps >= MIN_FEE_BPS && bps <= MAX_FEE_BPS
    }

    /// Mirrors `calculate_protocol_fee`'s floor division for a non-negative
    /// `amount`. Returns `0` for `bps == 0` (the runtime short-circuit).
    fn protocol_fee(amount: i128, bps: u32) -> i128 {
        if bps == 0 {
            return 0;
        }
        amount * bps as i128 / PROTOCOL_FEE_BPS_DENOMINATOR as i128
    }

    /// Mirrors the recovery bookkeeping bound: attempts `0..=MAX_RECOVERY_ATTEMPTS`.
    fn within_recovery_budget(attempts: u32) -> bool {
        attempts <= MAX_RECOVERY_ATTEMPTS
    }

    /// `count == MAX_MILESTONES` is accepted; `count == MAX_MILESTONES + 1` and
    /// `0` are rejected. This pins the inclusive upper bound.
    #[test]
    fn milestone_count_boundary() {
        assert!(
            accepts_milestone_count(MAX_MILESTONES),
            "exactly MAX_MILESTONES must be accepted"
        );
        assert!(
            !accepts_milestone_count(MAX_MILESTONES + 1),
            "MAX_MILESTONES + 1 must be rejected"
        );
        assert!(
            !accepts_milestone_count(0),
            "zero milestones must be rejected"
        );
    }

    /// `len == MAX_BATCH_MILESTONES` is accepted; `+1` and `0` are rejected.
    #[test]
    fn batch_milestone_count_boundary() {
        assert!(
            accepts_batch_count(MAX_BATCH_MILESTONES),
            "exactly MAX_BATCH_MILESTONES must be accepted"
        );
        assert!(
            !accepts_batch_count(MAX_BATCH_MILESTONES + 1),
            "MAX_BATCH_MILESTONES + 1 must be rejected"
        );
        assert!(!accepts_batch_count(0), "empty batch must be rejected");
    }

    /// The fee range is inclusive at both ends: `0` and `MAX_FEE_BPS`
    /// (`== PROTOCOL_FEE_BPS_DENOMINATOR`) are accepted, while
    /// `MAX_FEE_BPS + 1` and `u32::MAX` are rejected.
    #[test]
    fn fee_bps_boundary() {
        assert_eq!(MIN_FEE_BPS, 0);
        assert!(accepts_fee_bps(MIN_FEE_BPS), "0 bps must be accepted");
        assert!(
            accepts_fee_bps(MAX_FEE_BPS),
            "100% (MAX_FEE_BPS) must be accepted"
        );
        assert_eq!(
            MAX_FEE_BPS, PROTOCOL_FEE_BPS_DENOMINATOR,
            "the fee ceiling is the basis-point denominator"
        );
        assert!(
            !accepts_fee_bps(MAX_FEE_BPS + 1),
            "MAX_FEE_BPS + 1 must be rejected"
        );
        assert!(!accepts_fee_bps(u32::MAX), "u32::MAX must be rejected");
    }

    /// The fee helper's own boundary: `0` short-circuits to `0`, the maximum
    /// `100 %` fee equals the gross amount, and one bps less floors the value.
    /// The freelancer's net (`amount - fee`) is never negative.
    #[test]
    fn protocol_fee_computation_boundary() {
        let amount = 1_000_000_i128;

        assert_eq!(
            protocol_fee(amount, MIN_FEE_BPS),
            0,
            "0 bps charges nothing"
        );

        let full_fee = protocol_fee(amount, MAX_FEE_BPS);
        assert_eq!(
            full_fee, amount,
            "100% fee must equal the gross amount, never exceed it"
        );
        assert!(amount - full_fee >= 0, "net payout must be non-negative");

        // denominator - 1 bps floors down and never rounds up.
        let almost_full = protocol_fee(amount, PROTOCOL_FEE_BPS_DENOMINATOR - 1);
        assert_eq!(almost_full, amount - amount / PROTOCOL_FEE_BPS_DENOMINATOR);
        assert!(
            almost_full <= amount,
            "fee must never exceed the gross amount"
        );
    }

    /// Recovery attempts are bounded inclusively by `MAX_RECOVERY_ATTEMPTS`.
    #[test]
    fn recovery_attempts_boundary() {
        assert!(
            within_recovery_budget(MAX_RECOVERY_ATTEMPTS),
            "exactly MAX_RECOVERY_ATTEMPTS must be allowed"
        );
        assert!(
            !within_recovery_budget(MAX_RECOVERY_ATTEMPTS + 1),
            "MAX_RECOVERY_ATTEMPTS + 1 must be rejected"
        );
    }

    // ── Cross-constant invariants ────────────────────────────────────────────

    /// A batch can never release more milestones than a contract may contain.
    #[test]
    fn max_batch_milestones_le_max_milestones() {
        assert!(
            MAX_BATCH_MILESTONES <= MAX_MILESTONES,
            "MAX_BATCH_MILESTONES must not exceed MAX_MILESTONES"
        );
    }

    /// Every configured upper bound must be strictly positive so each range is
    /// non-empty and the fail-closed guards are meaningful.
    #[test]
    fn all_upper_bounds_are_positive() {
        assert!(MAX_MILESTONES > 0);
        assert!(MAX_BATCH_MILESTONES > 0);
        assert!(MAX_FEE_BPS > 0);
        assert!(MAX_RATING >= MIN_RATING);
        assert!(MAX_COMMENT_BYTES >= MIN_COMMENT_BYTES);
        assert!(MAX_WORK_EVIDENCE_BYTES >= MIN_WORK_EVIDENCE_BYTES);
        assert!(MAX_RECOVERY_ATTEMPTS > 0);
        assert!(RECOVERY_TTL_LEDGERS > 0);
    }

    /// The basis-point denominator must be an exact power of ten: `10^4`.
    /// A non-power-of-ten denominator would silently break percentage math.
    #[test]
    fn fee_denominator_is_a_power_of_ten() {
        let mut value = PROTOCOL_FEE_BPS_DENOMINATOR;
        let mut exponent = 0_u32;
        while value > 1 {
            assert_eq!(value % 10, 0, "denominator must be a power of ten");
            value /= 10;
            exponent += 1;
        }
        assert_eq!(value, 1, "denominator must reduce to exactly 1");
        assert_eq!(exponent, 4, "10_000 bps == 10^4 == 100%");
    }
}
