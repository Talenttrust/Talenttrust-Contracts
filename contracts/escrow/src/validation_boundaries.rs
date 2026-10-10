//! Centralized validation boundaries for all escrow entrypoints.
//!
//! This module re-exports every protocol limit constant that is enforced at
//! runtime across the public entrypoints of the escrow contract, adds
//! documentation cross-referencing the enforcement site, and provides
//! deterministic boundary-check helpers that can be used by both runtime code
//! and off-chain tooling.
//!
//! ## Decision boundary index
//!
//! | Boundary | Constant | Entrypoint enforced | Error on violation |
//! |---|---|---|---|
//! | Max milestones per contract | [`MAX_MILESTONES`] | `create_contract` | `TooManyMilestones` |
//! | Min milestones per contract | 1 | `create_contract` | `EmptyMilestones` |
//! | Min milestone amount | [`MIN_MILESTONE_AMOUNT_STROOPS`] | `create_contract` | `InvalidMilestoneAmount` |
//! | Max single milestone amount | [`MAX_SINGLE_AMOUNT_STROOPS`] | `create_contract` | `InvalidMilestoneAmount` |
//! | Max total escrow per contract | [`MAX_TOTAL_ESCROW_STROOPS`] | `create_contract` | `InvalidMilestoneAmount` |
//! | Min deposit amount | [`MIN_MILESTONE_AMOUNT_STROOPS`] | `deposit_funds` | `AmountMustBePositive` |
//! | Max deposit (contract cap) | contract total | `deposit_funds` | `InvalidMilestoneAmount` |
//! | Release index | 0..milestones.len() | `release_milestone` | `IndexOutOfBounds` |
//! | Approve milestone index | 0..milestones.len() | `approve_milestone_release` | `IndexOutOfBounds` |
//! | Min reputation rating | [`MIN_RATING`] | `issue_reputation` | `InvalidRating` |
//! | Max reputation rating | [`MAX_RATING`] | `issue_reputation` | `InvalidRating` |
//! | Min comment length | [`MIN_COMMENT_BYTES`] | `issue_reputation` | `EmptyComment` |
//! | Max comment length | [`MAX_COMMENT_BYTES`] | `issue_reputation` | `CommentTooLong` |
//! | Min protocol fee bps | [`MIN_FEE_BPS`] | `set_protocol_fee_bps` | `InvalidProtocolParameters` |
//! | Max protocol fee bps | [`MAX_FEE_BPS`] | `set_protocol_fee_bps` | `InvalidProtocolParameters` |
//! | Max work evidence length | [`MAX_WORK_EVIDENCE_BYTES`] | `submit_work_evidence` | `EvidenceTooLong` |
//! | Min work evidence length | [`MIN_WORK_EVIDENCE_BYTES`] | `submit_work_evidence` | `EmptyEvidence` |
//! | Max page size | [`PAGE_CEILING`] | paginated read views (clamped, not rejected) | — |
//! | Min admin-configurable milestones | [`MIN_MAX_MILESTONES`] | `set_max_milestones` | `LimitOutOfRange` |
//! | Max admin-configurable milestones | [`MAX_MAX_MILESTONES`] | `set_max_milestones` | `LimitOutOfRange` |
//!
//! ## Invariants
//!
//! 1. **Determinism**: Every boundary check is a pure numeric comparison
//!    against a compile-time constant. There is no floating-point arithmetic,
//!    no external oracle, and no clock dependency in boundary enforcement.
//!
//! 2. **Fail-closed**: Violated boundaries always produce a typed
//!    `panic_with_error`, never a silent truncation or default.
//!
//! 3. **Check order**: Inside `create_contract` the sequence is:
//!    milestone-count check → per-amount positivity check → total-cap check.
//!    This means `TooManyMilestones` fires before `InvalidMilestoneAmount` when
//!    both violations are present simultaneously.
//!
//! 4. **Checked arithmetic**: All summation over milestone amounts uses
//!    [`safe_add_amounts`] / `checked_add`, which converts an i128 overflow
//!    into `PotentialOverflow` rather than wrapping silently.
//!
//! 5. **Reputation gating**: `issue_reputation` requires the contract to have
//!    reached `ContractStatus::Completed` *and* have a positive pending-credit
//!    balance before accepting a rating. A contract that was fully refunded
//!    never accrues a credit and therefore permanently prevents reputation
//!    issuance for that contract.

// ── Re-exports from authoritative source modules ─────────────────────────────

/// Maximum number of milestones in a single contract.
///
/// Enforced in `create_contract`. Exceeding this limit raises
/// `EscrowError::TooManyMilestones`.
pub use crate::milestones_consts::MAX_MILESTONES;

/// Maximum number of milestones that can be released in one batch call.
pub use crate::milestones_consts::MAX_BATCH_MILESTONES;

/// Basis-point denominator for all fee calculations (10 000 = 100 %).
pub use crate::milestones_consts::PROTOCOL_FEE_BPS_DENOMINATOR;

/// Minimum protocol fee in basis points (0 = fee disabled).
///
/// Enforced in `set_protocol_fee_bps` and `set_governed_params` — values below
/// this are accepted because 0 is explicitly allowed (disables fee collection).
pub use crate::milestones_consts::MIN_FEE_BPS;

/// Maximum protocol fee in basis points (10 000 = 100 %).
///
/// Enforced in `set_protocol_fee_bps` and `set_governed_params`.  Values
/// strictly greater than this raise `Error::InvalidProtocolParameters`.
pub use crate::milestones_consts::MAX_FEE_BPS;

/// Minimum valid reputation rating (inclusive, default 1).
///
/// Enforced in `issue_reputation`. A rating below this raises
/// `Error::InvalidRating`.
pub use crate::milestones_consts::MIN_RATING;

/// Maximum valid reputation rating (inclusive, default 5).
///
/// Enforced in `issue_reputation`. A rating above this raises
/// `Error::InvalidRating`.
pub use crate::milestones_consts::MAX_RATING;

/// Maximum byte length for a reputation comment (200 bytes, inclusive).
///
/// Enforced in `issue_reputation`. A comment exceeding this length raises
/// `Error::CommentTooLong`.
pub use crate::milestones_consts::MAX_COMMENT_BYTES;

/// Minimum byte length for a reputation comment (1 byte, inclusive).
///
/// Enforced in `issue_reputation`. An empty comment raises `Error::EmptyComment`.
pub use crate::milestones_consts::MIN_COMMENT_BYTES;

/// Maximum byte length for a work evidence string (1 000 bytes, inclusive).
pub use crate::milestones_consts::MAX_WORK_EVIDENCE_BYTES;

/// Minimum byte length for a work evidence string (1 byte, inclusive).
pub use crate::milestones_consts::MIN_WORK_EVIDENCE_BYTES;

/// Maximum individual milestone amount, in stroops (1 × 10¹³ stroops = 1 M tokens).
///
/// Enforced per-milestone inside `create_contract` and `deposit_funds`.
/// Values strictly greater than this raise `Error::InvalidMilestoneAmount`.
pub use crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS;

/// Maximum page size for paginated read-only views.
///
/// `limit` parameters for paginated accessors are *clamped* to this value
/// rather than rejected — callers that pass a larger limit will simply receive
/// at most `PAGE_CEILING` entries without error.
pub use crate::constants::PAGE_CEILING;

/// Absolute minimum value for the admin-configurable max-milestones cap.
///
/// `set_max_milestones` rejects values below this with `Error::LimitOutOfRange`.
pub use crate::MIN_MAX_MILESTONES;

/// Absolute maximum value for the admin-configurable max-milestones cap.
///
/// `set_max_milestones` rejects values above this with `Error::LimitOutOfRange`.
pub use crate::MAX_MAX_MILESTONES;

/// Absolute minimum for the admin-configurable max-escrow-stroops cap.
///
/// Equivalent to 0.01 XLM (1 000 000 stroops). `set_max_escrow_stroops` rejects
/// values strictly below this with `Error::LimitOutOfRange`.
pub use crate::MIN_MAX_ESCROW_STROOPS;

/// Absolute minimum for the admin-configurable batch-settlement limit.
pub use crate::MIN_MAX_BATCH_SETTLEMENT;

/// Absolute maximum for the admin-configurable batch-settlement limit.
pub use crate::MAX_MAX_BATCH_SETTLEMENT;

// ── Derived boundary helpers ──────────────────────────────────────────────────

/// The minimum valid milestone amount, in stroops (1 stroop).
///
/// Any milestone with `amount <= 0` is rejected by `create_contract` with
/// `Error::InvalidMilestoneAmount`.
pub const MIN_MILESTONE_AMOUNT_STROOPS: i128 = 1;

/// The maximum total escrow value for a single contract, in stroops.
///
/// Equal to [`MAX_SINGLE_AMOUNT_STROOPS`] (1 000 000 tokens at 7 decimals).
/// This cap applies to the *sum* of all milestone amounts.  If the total
/// exceeds this value, `create_contract` raises `Error::InvalidMilestoneAmount`.
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = MAX_SINGLE_AMOUNT_STROOPS;

// ── Stateless boundary predicates ────────────────────────────────────────────

/// Returns `true` when `count` is a valid milestone count for `create_contract`.
///
/// Valid range: `1..=MAX_MILESTONES`. This matches the runtime check exactly:
/// - zero milestones → `EmptyMilestones`
/// - `count > MAX_MILESTONES` → `TooManyMilestones`
#[inline]
pub fn is_valid_milestone_count(count: u32) -> bool {
    count >= 1 && count <= MAX_MILESTONES
}

/// Returns `true` when `amount` is a valid individual milestone amount.
///
/// Valid range: `MIN_MILESTONE_AMOUNT_STROOPS..=MAX_SINGLE_AMOUNT_STROOPS`.
/// Zero and negative values are explicitly excluded.
#[inline]
pub fn is_valid_milestone_amount(amount: i128) -> bool {
    amount >= MIN_MILESTONE_AMOUNT_STROOPS && amount <= MAX_SINGLE_AMOUNT_STROOPS
}

/// Returns `true` when `rating` falls within the default rating scale.
///
/// Default scale is `MIN_RATING..=MAX_RATING` (1..=5). For runtime
/// enforcement the actual check uses the stored `ReputationConfig`, which
/// may be overridden by admin.  Use this helper only for tests and
/// documentation that target the default configuration.
#[inline]
pub fn is_valid_default_rating(rating: u32) -> bool {
    rating >= MIN_RATING && rating <= MAX_RATING
}

/// Returns `true` when `byte_len` is a valid reputation comment length.
///
/// Valid range: `MIN_COMMENT_BYTES..=MAX_COMMENT_BYTES` (1..=200).
#[inline]
pub fn is_valid_comment_length(byte_len: u32) -> bool {
    byte_len >= MIN_COMMENT_BYTES && byte_len <= MAX_COMMENT_BYTES
}

/// Returns `true` when `fee_bps` is a valid protocol fee setting.
///
/// Valid range: `MIN_FEE_BPS..=MAX_FEE_BPS` (0..=10_000).
#[inline]
pub fn is_valid_fee_bps(fee_bps: u32) -> bool {
    fee_bps <= MAX_FEE_BPS // MIN_FEE_BPS is 0, always satisfied for u32
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    // ── Constant value pins ───────────────────────────────────────────────────
    // These tests pin every exported constant value so that accidental
    // changes to authoritative source modules are caught immediately.

    #[test]
    fn milestone_count_boundary_constants_are_correct() {
        assert_eq!(MAX_MILESTONES, 10);
        assert_eq!(MIN_MAX_MILESTONES, 1);
        assert_eq!(MAX_MAX_MILESTONES, 100);
    }

    #[test]
    fn amount_boundary_constants_are_correct() {
        assert_eq!(MIN_MILESTONE_AMOUNT_STROOPS, 1);
        assert_eq!(MAX_SINGLE_AMOUNT_STROOPS, 1_000_000_0000000_i128);
        assert_eq!(MAX_TOTAL_ESCROW_STROOPS, MAX_SINGLE_AMOUNT_STROOPS);
    }

    #[test]
    fn fee_bps_constants_are_correct() {
        assert_eq!(MIN_FEE_BPS, 0);
        assert_eq!(MAX_FEE_BPS, 10_000);
        assert_eq!(PROTOCOL_FEE_BPS_DENOMINATOR, 10_000);
        assert_eq!(MAX_FEE_BPS, PROTOCOL_FEE_BPS_DENOMINATOR);
    }

    #[test]
    fn reputation_rating_constants_are_correct() {
        assert_eq!(MIN_RATING, 1);
        assert_eq!(MAX_RATING, 5);
        assert!(MIN_RATING <= MAX_RATING, "rating range must be non-empty");
    }

    #[test]
    fn reputation_comment_constants_are_correct() {
        assert_eq!(MIN_COMMENT_BYTES, 1);
        assert_eq!(MAX_COMMENT_BYTES, 200);
        assert!(
            MIN_COMMENT_BYTES <= MAX_COMMENT_BYTES,
            "comment range must be non-empty"
        );
    }

    #[test]
    fn page_ceiling_is_correct() {
        assert_eq!(PAGE_CEILING, 50);
    }

    #[test]
    fn batch_settlement_constants_are_correct() {
        assert_eq!(MIN_MAX_BATCH_SETTLEMENT, 1);
        assert_eq!(MAX_MAX_BATCH_SETTLEMENT, 100);
    }

    #[test]
    fn escrow_stroops_floor_is_correct() {
        // 1_000_000 stroops == 0.1 XLM (7 decimal token)
        assert_eq!(MIN_MAX_ESCROW_STROOPS, 1_000_000_i128);
    }

    // ── Predicate correctness ─────────────────────────────────────────────────

    #[test]
    fn milestone_count_predicate_accepts_valid_range() {
        assert!(is_valid_milestone_count(1));
        assert!(is_valid_milestone_count(5));
        assert!(is_valid_milestone_count(MAX_MILESTONES));
    }

    #[test]
    fn milestone_count_predicate_rejects_zero_and_above_max() {
        assert!(!is_valid_milestone_count(0));
        assert!(!is_valid_milestone_count(MAX_MILESTONES + 1));
    }

    #[test]
    fn milestone_amount_predicate_accepts_valid_range() {
        assert!(is_valid_milestone_amount(MIN_MILESTONE_AMOUNT_STROOPS));
        assert!(is_valid_milestone_amount(100_0000000));
        assert!(is_valid_milestone_amount(MAX_SINGLE_AMOUNT_STROOPS));
    }

    #[test]
    fn milestone_amount_predicate_rejects_zero_negative_and_above_max() {
        assert!(!is_valid_milestone_amount(0));
        assert!(!is_valid_milestone_amount(-1));
        assert!(!is_valid_milestone_amount(MAX_SINGLE_AMOUNT_STROOPS + 1));
        assert!(!is_valid_milestone_amount(i128::MAX));
    }

    #[test]
    fn rating_predicate_accepts_valid_range() {
        for r in MIN_RATING..=MAX_RATING {
            assert!(is_valid_default_rating(r), "rating {r} should be valid");
        }
    }

    #[test]
    fn rating_predicate_rejects_zero_and_above_max() {
        assert!(!is_valid_default_rating(0));
        assert!(!is_valid_default_rating(MAX_RATING + 1));
    }

    #[test]
    fn comment_length_predicate_accepts_valid_range() {
        assert!(is_valid_comment_length(MIN_COMMENT_BYTES));
        assert!(is_valid_comment_length(100));
        assert!(is_valid_comment_length(MAX_COMMENT_BYTES));
    }

    #[test]
    fn comment_length_predicate_rejects_zero_and_above_max() {
        assert!(!is_valid_comment_length(0));
        assert!(!is_valid_comment_length(MAX_COMMENT_BYTES + 1));
    }

    #[test]
    fn fee_bps_predicate_accepts_valid_range() {
        assert!(is_valid_fee_bps(MIN_FEE_BPS));
        assert!(is_valid_fee_bps(5_000));
        assert!(is_valid_fee_bps(MAX_FEE_BPS));
    }

    #[test]
    fn fee_bps_predicate_rejects_above_max() {
        assert!(!is_valid_fee_bps(MAX_FEE_BPS + 1));
        assert!(!is_valid_fee_bps(u32::MAX));
    }

    // ── Cross-constant invariants ─────────────────────────────────────────────

    #[test]
    fn max_total_escrow_equals_max_single_amount() {
        // The total cap is equal to the single-amount ceiling, which means a
        // single-milestone contract may consume the entire cap.
        assert_eq!(MAX_TOTAL_ESCROW_STROOPS, MAX_SINGLE_AMOUNT_STROOPS);
    }

    #[test]
    fn fee_denominator_matches_max_fee() {
        // MAX_FEE_BPS == PROTOCOL_FEE_BPS_DENOMINATOR guarantees that 100 % is
        // the ceiling and no fee ever exceeds the gross milestone amount.
        assert_eq!(MAX_FEE_BPS, PROTOCOL_FEE_BPS_DENOMINATOR);
    }

    #[test]
    fn max_milestones_le_max_max_milestones() {
        // The runtime default cap must never exceed the admin-configurable ceiling.
        assert!(MAX_MILESTONES <= MAX_MAX_MILESTONES);
    }

    #[test]
    fn min_max_milestones_le_max_milestones() {
        // The admin-configurable floor must not exceed the default cap.
        assert!(MIN_MAX_MILESTONES <= MAX_MILESTONES);
    }
}
