use soroban_sdk::{contracterror, contracttype, Address, String, Vec};
// ── Indexer summary types ────────────────────────────────────────────────────

#[allow(dead_code)]
pub const CONTRACT_SUMMARY_SCHEMA_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneSummary {
    pub index: u32,
    pub amount: i128,
    pub released: bool,
    pub refunded: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractSummary {
    pub schema_version: u32,
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub status: ContractStatus,
    pub reputation_issued: bool,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refundable_balance: i128,
    pub released_milestone_count: u32,
    pub milestones: Vec<MilestoneSummary>,
}

/// Returned by `get_contract_participants`.
///
/// Contains only the participant addresses for a contract: the client,
/// freelancer, and optional arbiter.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractParticipants {
    /// The client address that funded the contract.
    pub client: Address,
    /// The freelancer address performing the work.
    pub freelancer: Address,
    /// Optional arbiter address for dispute resolution.
    pub arbiter: Option<Address>,
}

/// Protocol-wide bounds for contract validation.
///
/// This type carries the hard-coded limits used by `create_contract` and other
/// validation paths. It is returned by `get_bounds()` for off-chain indexers
/// and client applications.</｜DSML｜tool>

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractBounds {
    /// Maximum number of milestones per contract.
    pub max_milestones: u32,
    /// Maximum amount allowed for a single milestone (in stroops).
    pub max_single_milestone_stroops: i128,
    /// Maximum total escrow amount for a single contract (in stroops).
    pub max_total_escrow_stroops: i128,
    /// Maximum protocol fee in basis points (10_000 = 100%).
    pub max_fee_bps: u32,
}

// ── Core contract state ──────────────────────────────────────────────────────

// ─── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    // Admin / pause / emergency
    Initialized,
    Admin,
    Paused,
    Emergency,
    // Contract storage
    Contract(u32),
    NextContractId,
    /// DEPRECATED: Was never written or read. Kept as placeholder to preserve discriminant numbering
    /// for storage compatibility. Milestone release state is tracked solely via Milestone.released
    /// in the milestones vector stored under (DataKey::Contract(id), "milestones").
    #[deprecated(since = "0.1.0", note = "Use Milestone.released in the milestones vector instead")]
    _MilestoneReleasedPlaceholder(u32, u32),
    MilestoneApprovals(u32, u32),
    // Reputation
    ReputationIssued(u32),
    PendingReputationCredits(Address),
    Reputation(Address),
    ReputationComment(u32),
    // Client migration
    PendingClientMigration(u32),
    // Protocol / governance
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
    ProtocolFeeBps,
    // Two-step admin transfer: pending admin stored here while proposal awaits acceptance
    PendingAdmin,
    AccumulatedProtocolFees,
    GovernedParameters,
    ReadinessChecklist,
    // Finalization
    Finalization(u32),
    // Settlement token
    SettlementToken,
}

/// Canonical error type for contract operations.
///
/// Declared here (in `types.rs`) so the `#[contracterror]` proc-macro from
/// soroban-sdk processes it in a submodule rather than in the crate root
/// alongside `#[contract]` / `#[contractimpl]`.  The crate root re-exports
/// it as both `crate::Error` and `crate::EscrowError` (via
/// `pub use types::Error;` and `pub use types::Error as EscrowError;`)
/// so all existing panic sites and test assertions continue to resolve.
///
/// NOTE: This is the SINGLE canonical `#[contracterror]` enum for the
/// entire escrow crate.  A previous revision had a separate `types::Error`
/// (53 variants) that was later consolidated into `EscrowError` in lib.rs.
/// That dual registration produced 188 host-side "contract error code
/// mismatch" failures.  This enum replaces both — a single registration
/// with all variants that source and test sites reference.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    ContractNotFound = 6,
    EmptyRefundRequest = 7,
    DuplicateMilestoneInRefund = 8,
    AlreadyReleased = 9,
    AlreadyRefunded = 10,
    InsufficientFunds = 11,
    AlreadyInitialized = 12,
    InsufficientAccumulatedFees = 13,
    /// Returned by lifecycle entrypoints when `initialize` has not been called.
    ///
    /// All money-flow operations require initialization so the admin-controlled
    /// safety rails (pause, emergency controls, protocol fees) are always in
    /// scope before any funds can move.
    NotInitialized = 14,
    UnauthorizedRole = 15,
    ContractPaused = 16,
    EmergencyActive = 17,
    InvalidState = 18,
    InvalidRating = 19,
    SelfRating = 20,
    ReputationAlreadyIssued = 21,
    NotCompleted = 22,
    FreelancerMismatch = 23,
    InvalidStatusTransition = 24,
    ArbiterRequired = 25,
    InvalidDisputeSplit = 26,
    AccountingInvariantViolated = 27,
    PotentialOverflow = 28,
    AlreadyFinalized = 29,
    AmountMustBePositive = 30,
    /// No settlement token has been bound for custody transfers.
    SettlementTokenNotConfigured = 31,
    /// A settlement token has already been bound.
    SettlementTokenAlreadyBound = 32,
    /// The sum of milestone amounts exceeded the configured maximum or overflowed.
    TotalCapExceeded = 33,
    /// Too many milestones were provided.
    TooManyMilestones = 34,
    /// An arbiter was required by the release authorization mode but not provided.
    MissingArbiter = 35,
    /// The provided arbiter is invalid (same as client or freelancer).
    InvalidArbiter = 36,
    /// Contract is cancelled and must not accept further value-moving operations.
    ContractCancelled = 37,
    /// Contract has been refunded and is terminal for value-moving operations.
    ContractRefunded = 38,
    /// The address supplied as settlement token is not a valid token contract.
    /// The pre-bind probe called `token::Client::balance` against the escrow
    /// contract address and the call panicked — the address does not implement
    /// the SAC token interface.
    InvalidSettlementToken = 39,
    /// The address supplied as settlement token is the escrow contract itself.
    /// Binding self would create a circular custody reference and brick all
    /// transfer paths.
    SettlementTokenIsSelf = 40,
    /// The address supplied as settlement token is the escrow admin.
    /// Binding the admin as the custody asset conflates governance authority
    /// with the settlement token role.
    SettlementTokenIsAdmin = 41,
    /// Reputation feedback comment was empty.
    EmptyComment = 42,
    /// Reputation feedback comment exceeded the 200-character maximum.
    CommentTooLong = 43,
    /// Returned when `accept_governance_admin` is called before
    /// `ADMIN_ROTATION_MIN_DELAY_LEDGERS` have elapsed since the matching
    /// `propose_governance_admin` call.  Mirrors the canonical
    /// [`crate::Error::TimelockNotElapsed`] variant (types.rs) so off-chain
    /// callers can decode the timelock violation on either enum.  Numeric
    /// value matches for cross-enum `assert_contract_error` comparisons.
    /// Mirrors the legacy [`Error::TimelockNotElapsed`] for stable host error
    /// code semantics.  See `Error` in `types.rs` for the
    /// canonical discriminant reference.
    TimelockNotElapsed = 48,
    /// Specified milestone index is out of bounds.  Mirrors the legacy
    /// [`Error::IndexOutOfBounds`] disc so source sites that panic with the
    /// legacy name produce a stable host error code.
    IndexOutOfBounds = 49,
    /// Per-milestone approval stage.  Mirrors the legacy
    /// [`Error::AlreadyApproved`] disc for the approvals flow.
    AlreadyApproved = 50,
    /// Approval-stage failure: not enough sustained approvals to release.
    /// Mirrors the legacy [`Error::InsufficientApprovals`] disc.
    InsufficientApprovals = 51,
    /// Internal allocator error: a contract id collision was detected.
    /// Mirrors the legacy [`Error::ContractIdCollision`] disc.
    ContractIdCollision = 52,
    /// Internal allocator error: the contract id space overflowed.
    /// Mirrors the legacy [`Error::ContractIdOverflow`] disc.
    ContractIdOverflow = 53,
    /// Work-evidence string exceeded the maximum length.  Mirrors the
    /// legacy [`Error::EvidenceTooLong`] disc.
    EvidenceTooLong = 54,
    /// Governance parameter validation failure.  Mirrors the legacy
    /// [`Error::InvalidProtocolParameters`] disc.
    InvalidProtocolParameters = 55,
    /// A milestone deadline is set but the deadline has not yet expired.
    /// Mirrors the legacy [`Error::MilestoneNotOverdue`] disc.
    MilestoneNotOverdue = 56,
    SettlementTokenNotConfigured = 52,
    /// The milestone deadline has not yet passed.
    MilestoneNotOverdue = 53,
    /// The caller is not a recognized party (client, freelancer, or arbiter) of
    /// this contract.  Returned by the shared `require_party` helper.
    PartyNotAuthorized = 54,
}

/// Contract lifecycle states
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Accepted = 1,
    Funded = 2,
    Completed = 3,
    Disputed = 4,
    Cancelled = 5,
    Refunded = 6,
    PartiallyFunded = 7,
}

/// Main escrow contract state
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contract {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub status: ContractStatus,
    pub total_deposited: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub release_authorization: ReleaseAuthorization,
    pub reputation_issued: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub funded_amount: i128,
    pub released: bool,
    pub refunded: bool,
    pub work_evidence: Option<String>,
    pub refunded_amount: i128,
    /// Optional Unix timestamp (seconds) after which the client may claim
    /// a timeout refund for this milestone without arbiter involvement.
    /// None means no deadline — the milestone never expires.
    pub deadline: Option<u64>,
}

/// Defines who can approve milestone releases.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    /// Only client can approve.
    ClientOnly = 0,
    /// Either client or arbiter can approve.
    ClientAndArbiter = 1,
    /// Only arbiter can approve.
    ArbiterOnly = 2,
    /// Both client and freelancer must approve; only either of them may release
    /// after both approvals are present.
    MultiSig = 3,
}

/// Tracks approval status for a milestone.
/// Stored in temporary storage with TTL for expiry grace period.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneApprovals {
    pub client_approved: bool,
    pub freelancer_approved: bool,
    pub arbiter_approved: bool,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepositMode {
    ExactTotal = 0,
    Incremental = 1,
}

// ── Governance / readiness ───────────────────────────────────────────────────

/// Readiness checklist stored under [`DataKey::ReadinessChecklist`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadinessChecklist {
    /// `true` after `initialize` has been called successfully.
    pub initialized: bool,
    /// `true` after protocol governance parameters have been set.
    pub governed_params_set: bool,
    /// `true` after an emergency control operation has been invoked.
    pub emergency_controls_enabled: bool,
}

impl Default for ReadinessChecklist {
    fn default() -> Self {
        ReadinessChecklist {
            initialized: false,
            governed_params_set: false,
            emergency_controls_enabled: false,
        }
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedParameters {
    pub protocol_fee_bps: u32,
    pub max_escrow_total_stroops: i128,
}

// ── Protocol state view ─────────────────────────────────────────────────────

/// Unified, read-only snapshot of the escrow protocol's global state.
///
/// Returned by [`Escrow::get_protocol_state`] in a single O(1) storage read.
/// All fields use sensible defaults when the corresponding key has not been
/// written yet, so the function never panics — even on a freshly deployed
/// contract that has not been initialized.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolState {
    /// Whether `initialize` has been called.
    pub initialized: bool,
    /// The current admin address, if set.
    pub admin: Option<Address>,
    /// Whether the contract is paused.
    pub paused: bool,
    /// Whether emergency mode is active.
    pub emergency: bool,
    /// The bound settlement token address, if any.
    pub settlement_token: Option<Address>,
    /// The next auto-incremented contract ID.
    pub next_contract_id: u32,
    /// Protocol fee in basis points.
    pub protocol_fee_bps: u32,
    /// Cumulative protocol fees awaiting withdrawal.
    pub accumulated_protocol_fees: i128,
    /// Maximum total escrow amount (from governed parameters), if set.
    pub max_escrow_total_stroops: Option<i128>,
    /// Deployment readiness checklist.
    pub readiness: ReadinessChecklist,
}

/// Stores a pending governance admin proposal with the proposed address
/// and the ledger sequence when it was proposed.
/// Used for the admin rotation timelock mechanism.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAdminProposal {
    pub proposed: Address,
    pub proposed_at_ledger: u32,
}

// ── Reputation ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Reputation {
    pub completed_contracts: i128,
    pub total_rating: i128,
    pub last_rating: i128,
}

/// A read-only, combined view of a freelancer's reputation state.
///
/// Aggregates all reputation-related fields into a single O(1) read so callers
/// do not need to assemble the picture from multiple storage keys.
///
/// When no reputation record has been written yet every numeric field is `0`
/// and `average_rating_bps` is `0` (rather than panicking or returning `None`).
///
/// # Fields
/// * `completed_contracts`  — total contracts for which reputation was issued
/// * `total_rating`         — sum of all ratings (1–5 per contract)
/// * `last_rating`          — most-recent rating value, or `0` if none
/// * `average_rating_bps`   — `total_rating × 10_000 / completed_contracts`,
///                            or `0` when `completed_contracts == 0`
/// * `pending_credits`      — contracts completed but not yet rated
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationView {
    pub completed_contracts: i128,
    pub total_rating: i128,
    pub last_rating: i128,
    /// Average rating scaled to basis points (×10 000).
    /// `0` when `completed_contracts == 0`.
    pub average_rating_bps: i128,
    pub pending_credits: i128,
}

impl Default for ReputationView {
    fn default() -> Self {
        ReputationView {
            completed_contracts: 0,
            total_rating: 0,
            last_rating: 0,
            average_rating_bps: 0,
            pending_credits: 0,
        }
    }
}

// ── Dispute Resolution ───────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeSplit {
    pub client_amount: i128,
    pub freelancer_amount: i128,
}

pub type SplitAmounts = DisputeSplit;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeResolution {
    FullRefund,
    PartialRefund,
    FullPayout,
    Split(DisputeSplit),
}

impl DisputeResolution {
    pub fn code(&self) -> u32 {
        match self {
            Self::FullRefund => 0,
            Self::PartialRefund => 1,
            Self::FullPayout => 2,
            Self::Split(_) => 3,
        }
    }
}
