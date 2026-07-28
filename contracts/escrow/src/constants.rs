/// Minimum valid reputation rating (inclusive).
pub const MIN_RATING: u32 = 1;

/// Maximum valid reputation rating (inclusive).
pub const MAX_RATING: u32 = 5;

/// Max byte length of a reputation feedback comment.
pub const MAX_COMMENT_BYTES: u32 = 200;

/// Unit increment for pending reputation credits.
pub const REPUTATION_CREDIT_INCREMENT: i128 = 1;

/// Basis-point scaling factor for `get_average_rating` (×10_000 preserves four decimal places).
pub const SCALE: i128 = 10_000;

/// Upper bound on the `limit` parameter of paginated read views.
///
/// Keeps per-call storage reads bounded and prevents callers from requesting
/// unbounded scans in a single invocation.
pub const PAGE_CEILING: u32 = 50;
