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
    // Finalization
    Finalization(u32),
    // Settlement token
    SettlementToken,
}

/// Canonical contract error type for all entrypoint-facing errors.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// The specified milestone index is out of bounds.
    IndexOutOfBounds = 3,
    /// The milestone has already been released.
    AlreadyReleased = 4,
    /// The refund request is empty.
    EmptyRefundRequest = 6,
    /// Duplicate milestone indices specified in the refund request.
    DuplicateMilestoneInRefund = 7,
    /// The milestone has already been refunded.
    AlreadyRefunded = 8,
    /// Insufficient funds available to perform the operation.
    InsufficientFunds = 9,
    /// The requested contract was not found.
    ContractNotFound = 10,
    /// The caller is not authorized for this operation.
    UnauthorizedRole = 11,
    /// The contract requires an arbiter address but none was provided.
    MissingArbiter = 12,
    /// The provided arbiter address is invalid (e.g. same as client or freelancer).
    InvalidArbiter = 13,
    /// The client and freelancer addresses are identical or invalid.
    InvalidParticipants = 14,
    /// The amount must be strictly greater than zero.
    AmountMustBePositive = 15,
    /// The contract is in an invalid state for this operation.
    InvalidState = 16,
    /// The milestone has already been released.
    MilestoneAlreadyReleased = 17,
    /// The milestone has already been approved.
    AlreadyApproved = 18,
    /// The milestone has not received sufficient approvals to release.
    InsufficientApprovals = 20,
    /// The freelancer address does not match the stored freelancer.
    FreelancerMismatch = 21,
    /// The rating value is outside the allowed range (1 to 5).
    InvalidRating = 22,
    /// Reputation has already been issued for this contract.
    ReputationAlreadyIssued = 23,
    /// The milestone list cannot be empty.
    EmptyMilestones = 25,
    /// The milestone amount is invalid.
    InvalidMilestoneAmount = 26,
    /// A contract with the specified ID already exists.
    ContractIdCollision = 27,
    /// The contract ID has overflowed the maximum limit.
    ContractIdOverflow = 28,
    /// The comment string is empty.
    EmptyComment = 29,
    /// The comment string exceeds the maximum length limit.
    CommentTooLong = 30,
    /// The participant address is invalid.
    InvalidParticipant = 31,
    /// The deposit amount is invalid.
    InvalidDepositAmount = 32,
    /// The milestone configuration is invalid.
    InvalidMilestone = 33,
    /// The contract has already been initialized.
    AlreadyInitialized = 34,
    /// Insufficient accumulated fees available for extraction.
    InsufficientAccumulatedFees = 35,
    /// The contract has not been initialized.
    NotInitialized = 36,
    /// The contract is currently paused.
    ContractPaused = 37,
    /// Emergency mode is currently active.
    EmergencyActive = 38,
    /// Self-rating is not allowed.
    SelfRating = 39,
    /// The contract has not been completed.
    NotCompleted = 40,
    /// The requested contract status transition is invalid.
    InvalidStatusTransition = 41,
    /// An arbiter is required for this operation.
    ArbiterRequired = 42,
    /// The dispute split percentage is invalid.
    InvalidDisputeSplit = 43,
    /// The operation would violate the core accounting invariant.
    AccountingInvariantViolated = 44,
    /// Checked arithmetic operation resulted in an overflow.
    PotentialOverflow = 45,
    /// The contract has already been finalized.
    AlreadyFinalized = 46,
    /// The contract has already been cancelled.
    AlreadyCancelled = 50,
    /// The work evidence string exceeds the maximum length limit.
    EvidenceTooLong = 47,
    /// The governance admin rotation timelock has not elapsed.
    TimelockNotElapsed = 48,
    /// The provided protocol parameters are invalid.
    InvalidProtocolParameters = 49,
    /// The escrow cap would be exceeded by this operation.
    EscrowCapExceeded = 51,
    /// No settlement token has been bound for custody transfers.
    SettlementTokenNotConfigured = 52,
    /// The milestone deadline has not yet passed.
    MilestoneNotOverdue = 53,
    /// The contract ID is out of valid bounds.
    InvalidContractId = 54,
    /// Arithmetic overflow occurred during reputation calculation.
    ArithmeticOverflow = 55,
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
