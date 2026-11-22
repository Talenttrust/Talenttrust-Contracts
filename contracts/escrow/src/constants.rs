//! Contract constants.
//! Public re-exported from the parent `lib.rs` module.

/// Maximum number of milestones per escrow.
pub const MAX_MILESTONES: u32 = 20;
/// Maximum number of ids accepted by `get_escrow_summaries`.
pub const MAX_SUMMARY_BATCH: u32 = 25;
/// Maximum number of escrow ids accepted by a single `migrate_escrows` call.
pub const MAX_MIGRATE_BATCH: u32 = 50;
/// Maximum number of escrow ids stored in a single per-party index entry and
/// the maximum page size returned by the paginated index read APIs.
pub const ESCROW_INDEX_PAGE_SIZE: u32 = 50;
/// Basis-point denominator for all split calculations.
pub const BPS_DENOMINATOR: i128 = 10_000;
/// 48-hour time-lock delay before a scheduled WASM upgrade can be executed.
pub const UPGRADE_DELAY_SECS: u64 = 172_800;
/// Pause categories (bitmask) for `set_pause_flags`.
pub const PAUSE_CREATE: u32 = 1;
pub const PAUSE_DEPOSIT: u32 = 2;
pub const PAUSE_RELEASE: u32 = 4;
pub const PAUSE_DISPUTE: u32 = 8;
pub const PAUSE_CANCEL: u32 = 16;
pub const PAUSE_REPUTATION: u32 = 32;
pub const PAUSE_ALL: u32 = 63;
/// Maximum protocol fee: 500 bps (5%).
pub const MAX_FEE_BPS: u32 = 500;
/// Minimum continuous pause before either party can force an emergency exit (30 days).
pub const EMERGENCY_EXIT_DELAY_SECS: u64 = 30 * 86_400;
/// Minimum review window after a milestone submission: 1 hour.
pub const MIN_REVIEW_WINDOW_SECS: u64 = 3_600;
/// Maximum review window after a milestone submission: 30 days.
pub const MAX_REVIEW_WINDOW_SECS: u64 = 30 * 86_400;
/// Review window used by `create_contract` / `create_contract_sequential` /
/// `create_and_fund` (7 days); `create_contract_with_review` sets a custom one.
pub const DEFAULT_REVIEW_WINDOW_SECS: u64 = 7 * 86_400;
/// Funding window after creation (14 days); afterwards an escrow still in
/// `Created` can no longer be funded and may be expired by anyone.
pub const FUNDING_WINDOW_SECS: u64 = 14 * 86_400;
/// Minimum amount for a single milestone: 0.1 token unit at 7 decimal places.
pub const MIN_MILESTONE_AMOUNT: i128 = 1_000_000;
/// Maximum total escrow value: 10^18 stroops.
pub const MAX_ESCROW_TOTAL: i128 = 1_000_000_000_000_000_000;
// Minimum value actually released to the freelancer for a rating to count:
// 1 token unit at 7 decimals (10x `MIN_MILESTONE_AMOUNT`). Sybil resistance
// comes from this value threshold (farming reputation costs real value), not
// from counterparty graph analysis.
pub const MIN_REPUTATION_VALUE: i128 = 10_000_000;
// Escrows with `total_amount >= PANEL_THRESHOLD_AMOUNT` can only be settled
// by the arbiter panel (`vote_resolution`), not by a single `resolve_dispute`
// call. 10^12 base units (~100k tokens on a 7-decimal asset).
pub const PANEL_THRESHOLD_AMOUNT: i128 = 1_000_000_000_000;
// Maximum number of members on the arbiter panel.
pub const PANEL_MAX_MEMBERS: u32 = 7;
// Maximum number of distinct funders per escrow (`deposit_from` / `deposit_funds`).
pub const MAX_FUNDERS: u32 = 10;
// Threshold for dispute outcome accounting (5,000 basis points, or 50%).
/// In dispute resolution, if freelancer_bps >= DISPUTE_WIN_THRESHOLD_BPS,
// the outcome is counted as a win for the freelancer and a loss for the client; otherwise, the reverse.
pub const DISPUTE_WIN_THRESHOLD_BPS: u32 = 5_000;
// Refundable dispute bond rate: 200 bps (2%) of the escrow total, paid by the
// party opening a dispute and returned or forfeited on resolution.
pub const DISPUTE_BOND_BPS: i128 = 200;
// Floor for the dispute bond. Kept at 0, so tiny escrows round down to no bond.
pub const DISPUTE_BOND_MIN: i128 = 0;
/// Persistent storage TTL: ~1 day (in ledger seconds).
const PERSISTENT_TTL_THRESHOLD: u32 = 17_280;
// Persistent storage TTL extend: ~30 days (in ledger seconds).
const PERSISTENT_TTL_EXTEND: u32 = 518_400;
// Grace period (in ledgers, ~30 days at ~5s per ledger) after which an unrated
// terminal escrow may be compacted.
pub const THIRTY_DAYS_IN_LEDGERS: u32 = 518_400;