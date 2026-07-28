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

/// Protocol-wide bounds for contract validation.
///
/// This type carries the hard-coded limits used by `create_contract` and other
/// validation paths. It is returned by `get_bounds()` for off-chain indexers
/// and client applications.
///
/// Dedicated struct for protocol bounds prevents coupling the limits ABI to the
/// per-contract summary schema version.
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
    /// Maximum number of contracts finalizable in a single batch settlement call.
    pub max_settlement: u32,
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
    MilestoneReleased(u32, u32),
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
    ContractsParameters,
    // Finalization
    Finalization(u32),
    // Settlement token
    SettlementToken,
    DisputeRollback(u32),
    // Dispute / arbiter configuration
    DisputeConfigKey,
    // Reputation configuration
    ReputationConfigKey,
    // Configurable settlement (batch finalize) limit
    MaxSettlement,
    // Milestone vector (replaces composite (Contract(id), "milestones"))
    Milestones(u32),
    // Reputation schema version marker
    ReputationStorageVersion(Address),
    // Migration state (test-only)
    State,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    IndexOutOfBounds = 3,
    AlreadyReleased = 4,
    EmptyRefundRequest = 6,
    DuplicateMilestoneInRefund = 7,
    /// The milestone has already been refunded.
    AlreadyRefunded = 8,
    InsufficientFunds = 9,
    ContractNotFound = 10,
    UnauthorizedRole = 11,
    MissingArbiter = 12,
    InvalidArbiter = 13,
    InvalidParticipants = 14,
    AmountMustBePositive = 15,
    InvalidState = 16,
    MilestoneAlreadyReleased = 17,
    AlreadyApproved = 18,
    InsufficientApprovals = 20,
    FreelancerMismatch = 21,
    InvalidRating = 22,
    ReputationAlreadyIssued = 23,
    EmptyMilestones = 25,
    InvalidMilestoneAmount = 26,
    /// A contract with the specified ID already exists.
    ContractIdCollision = 27,
    ContractIdOverflow = 28,
    EmptyComment = 29,
    CommentTooLong = 30,
    InvalidParticipant = 31,
    InvalidDepositAmount = 32,
    InvalidMilestone = 33,
    /// The deposit amount is invalid.
    InvalidDepositAmount = 32,
    /// The contract has already been initialized.
    AlreadyInitialized = 34,
    InsufficientAccumulatedFees = 35,
    NotInitialized = 36,
    ContractPaused = 37,
    EmergencyActive = 38,
    SelfRating = 39,
    NotCompleted = 40,
    InvalidStatusTransition = 41,
    ArbiterRequired = 42,
    InvalidDisputeSplit = 43,
    AccountingInvariantViolated = 44,
    PotentialOverflow = 45,
    AlreadyFinalized = 46,
    EvidenceTooLong = 47,
    TimelockNotElapsed = 48,
    InvalidProtocolParameters = 49,
    AlreadyCancelled = 50,
    EscrowCapExceeded = 51,
    /// No settlement token has been bound for custody transfers.
    SettlementTokenNotConfigured = 52,
    MilestoneNotOverdue = 53,
    /// `issue_reputation` was called but the freelancer has no pending reputation
    /// credits to consume. This indicates an internal accounting inconsistency
    /// (the contract reached `Completed` without `grant_pending_reputation_credit`
    /// being called) or a duplicate call after credits were already fully drained.
    NoPendingReputationCredits = 54,
    /// No safe rollback is available for the contract's current state.
    RollbackNotAllowed = 54,
    RollbackStateChanged = 55,
    /// The provided reputation parameters are out of the allowed bounds.
    InvalidReputationParameters = 56,
    /// The provided contracts parameters are out of the allowed bounds.
    InvalidContractsParameters = 57,
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

/// Maximum records returned per pagination request across view entrypoints.
pub const MAX_PAGINATION_LIMIT: u32 = 50;

/// Bounded pagination record for milestone release authorization status.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationRecord {
    pub milestone_index: u32,
    pub has_approvals: bool,
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

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractsParameters {
    pub max_milestones: u32,
    pub max_escrow_stroops: i128,
}

impl Default for ContractsParameters {
    fn default() -> Self {
        ContractsParameters {
            max_milestones: crate::contracts::DEFAULT_MAX_MILESTONES,
            max_escrow_stroops: crate::contracts::DEFAULT_MAX_TOTAL_ESCROW_STROOPS,
        }
    }
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

/// Runtime-configurable reputation validation parameters, stored under
/// [`DataKey::ReputationConfigKey`].
///
/// These were compile-time constants (`MIN_RATING`, `MAX_RATING`,
/// `MAX_COMMENT_BYTES`) until issue #1119 added
/// `Escrow::set_reputation_config`, which lets the admin retune them within
/// bounds without redeploying the contract. `issue_reputation` reads this
/// config (falling back to [`ReputationConfig::default`], which matches the
/// original constants) instead of the raw constants directly.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReputationConfig {
    /// Minimum valid rating (inclusive).
    pub min_rating: u32,
    /// Maximum valid rating (inclusive).
    pub max_rating: u32,
    /// Maximum byte length of a reputation feedback comment (inclusive).
    pub max_comment_bytes: u32,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        ReputationConfig {
            min_rating: 1,
            max_rating: 5,
            max_comment_bytes: 200,
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

/// Configuration for the arbiter's partial-refund split, stored under
/// [`DataKey::DisputeConfigKey`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeConfig {
    /// Share of remaining funds allocated to the freelancer in partial refunds
    /// (basis points, `3000` = 30%).
    pub partial_refund_freelancer_bps: u32,
    /// Share of remaining funds allocated to the client in partial refunds
    /// (basis points, `7000` = 70%).
    pub partial_refund_client_bps: u32,
}

impl Default for DisputeConfig {
    fn default() -> Self {
        DisputeConfig {
            partial_refund_freelancer_bps: 3000,
            partial_refund_client_bps: 7000,
        }
    }
}

/// Named result type returned by [`dispute::resolution_payouts`].
///
/// Replaces the opaque `(i128, i128)` tuple so callers can reference fields by
/// name (`client_payout`, `freelancer_payout`, `available_balance`) rather than
/// relying on positional index.
///
/// # Invariant
/// `client_payout + freelancer_payout == available_balance`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeInfo {
    /// Escrowed balance at the time the resolution was computed:
    /// `funded_amount - released_amount - refunded_amount`.
    pub available_balance: i128,
    /// Amount to be credited back to the client (refund side).
    pub client_payout: i128,
    /// Amount to be forwarded to the freelancer (release side).
    pub freelancer_payout: i128,
}
