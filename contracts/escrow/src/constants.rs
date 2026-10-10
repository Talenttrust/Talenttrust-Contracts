//! Deterministic policy constants and checked arithmetic for the escrow contract.
//!
//! # Why this module exists
//!
//! A boundary value that decides whether an operation succeeds, fails with a
//! typed error, or is rejected must be defined exactly once. When a limit or an
//! increment is written as a literal at each call site, two paths that are
//! supposed to behave identically can diverge — one wraps on overflow while the
//! other rejects, one accepts a value while the other refuses it. Failure
//! recovery then depends on *which* path happened to run, which is the
//! non-determinism this module removes: every limit and every increment used by
//! the recovery paths below is owned here and shared by every caller.
//!
//! # The pending reputation-credit recovery ledger
//!
//! `DataKey::PendingReputationCredits(freelancer)` is a durable counter of
//! "this freelancer completed a contract and is therefore owed one reputation
//! issuance":
//!
//! * **Accrual** — exactly one credit is added whenever a contract reaches
//!   [`crate::ContractStatus::Completed`], on every terminal path (final
//!   milestone release, batch settlement, partial-refund completion, and
//!   arbiter dispute resolution). A fully `Refunded` contract never accrues.
//! * **Consumption** — exactly one credit is removed by `issue_reputation`, the
//!   only way a completion becomes a stored reputation record.
//!
//! # Invariants
//!
//! | # | Invariant |
//! |---|-----------|
//! | I1 | A stored ledger value is always within `0..=MAX_PENDING_REPUTATION_CREDITS`. |
//! | I2 | An accrual changes a ledger by exactly `REPUTATION_CREDIT_INCREMENT`; a consumption removes exactly the same amount. |
//! | I3 | Accrual past the ceiling and consumption from an empty ledger are *rejected* — never wrapped, saturated, or silently clamped. |
//! | I4 | Rejection is reported as `None` from the pure helpers, so the caller raises a typed contract error (`Error::PotentialOverflow` / `Error::NotCompleted`) and the stored ledger is left unchanged. A retry therefore observes exactly the same state and fails the same way. |
//!
//! The helpers below are the only supported arithmetic for that ledger, and the
//! `const` assertions at the end of this file turn an inconsistent policy into a
//! compile-time failure instead of a runtime surprise.

/// Minimum valid reputation rating (inclusive).
///
/// # Invariant
/// `MIN_RATING >= 1` — zero is not a valid rating because absence of rating
/// is represented by `None`, not by a zero value.  Every reputation-issuing
/// path (`issue_reputation`) must reject `rating < MIN_RATING`.
pub const MIN_RATING: u32 = 1;

/// Maximum valid reputation rating (inclusive).
///
/// # Invariant
/// `MAX_RATING >= MIN_RATING` — the valid rating interval must be non-empty.
/// The current 1–5 scale matches common freelance platforms and is small
/// enough to avoid precision disputes.  `issue_reputation` must reject
/// `rating > MAX_RATING`.
pub const MAX_RATING: u32 = 5;

/// Max byte length of a reputation feedback comment.
///
/// # Invariant
/// `MAX_COMMENT_BYTES >= 1` — a zero-length comment is rejected as empty; use
/// a minimum of 1 byte so the interval `[1, MAX_COMMENT_BYTES]` is non-empty.
/// `issue_reputation` must reject comments whose `len() > MAX_COMMENT_BYTES`.
pub const MAX_COMMENT_BYTES: u32 = 200;

/// Unit increment for pending reputation credits.
///
/// Every accrual adds exactly this amount and every consumption removes exactly
/// this amount, so the ledger is a faithful count of completed contracts that
/// have not yet been rated. It is deliberately `1`: one completed contract
/// yields exactly one issuable reputation.
pub const REPUTATION_CREDIT_INCREMENT: i128 = 1;

/// Deterministic upper bound on a single freelancer's pending-credit ledger.
///
/// The ledger counts completed-but-unrated contracts, so sitting at this value
/// indicates a bug or an attempted accounting attack rather than a legitimate
/// workload. Accruing past it is rejected with a typed error instead of
/// wrapping, which keeps failure recovery deterministic (see
/// [`accrue_pending_credit`]).
///
/// The bound is many orders of magnitude below `i128::MAX`, so checked
/// arithmetic can never be the first thing to fail, and it is far above any
/// realistic number of completed contracts for a single freelancer, so it never
/// acts as a business limit.
pub const MAX_PENDING_REPUTATION_CREDITS: i128 = 1_000_000;

/// Basis-point scaling factor for `get_average_rating` (×10_000 preserves four decimal places).
///
/// # Invariant
/// `SCALE > 0` — the scaling factor must be strictly positive so that the
/// fixed-point arithmetic used in `get_average_rating` never divides by zero
/// and always yields a non-negative result for valid ratings.
pub const SCALE: i128 = 10_000;

/// Upper bound on the `limit` parameter of paginated read views.
///
/// Keeps per-call storage reads bounded and prevents callers from requesting
/// unbounded scans in a single invocation.
///
/// # Invariant
/// `PAGE_CEILING >= 1` — at least one record per page must be returnable;
/// a ceiling of zero would make every paginated read vacuous.
pub const PAGE_CEILING: u32 = 50;

/// Normalize a pagination request without allowing an unbounded storage scan.
/// Zero remains valid and means "return an empty page", preserving the read
/// API's compatibility behavior; oversized requests are safely capped.
pub(crate) const fn normalize_page_limit(limit: u32) -> u32 {
    if limit > PAGE_CEILING {
        PAGE_CEILING
    } else {
        limit
    }
}

/// Decrement one pending reputation credit, rejecting empty or corrupt ledgers.
pub const fn consume_pending_credit(current: i128) -> Option<i128> {
    if current < 1 || current > MAX_PENDING_REPUTATION_CREDITS {
        None
    } else {
        Some(current - 1)
    }
}

/// Add one pending reputation credit without exceeding the configured ceiling.
pub const fn accrue_pending_credit(current: i128) -> Option<i128> {
    if current < 0 || current >= MAX_PENDING_REPUTATION_CREDITS {
        None
    } else {
        Some(current + REPUTATION_CREDIT_INCREMENT)
    }
}

/// Check that a persisted pending-credit value is within protocol bounds.
pub const fn is_valid_pending_credit_ledger(value: i128) -> bool {
    value >= 0 && value <= MAX_PENDING_REPUTATION_CREDITS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_limit_boundaries_are_deterministic() {
        assert_eq!(normalize_page_limit(0), 0);
        assert_eq!(normalize_page_limit(1), 1);
        assert_eq!(normalize_page_limit(PAGE_CEILING), PAGE_CEILING);
        assert_eq!(normalize_page_limit(PAGE_CEILING + 1), PAGE_CEILING);
        assert_eq!(normalize_page_limit(u32::MAX), PAGE_CEILING);
    }
}
