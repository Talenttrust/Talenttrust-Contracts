use soroban_sdk::{contracterror, contracttype, Address, String, Vec};

// ─── Indexer summary types ────────────────────────────────────────────────────

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

// Removed duplicate Contract definition with missing fields

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
}

/// Canonical contract error type for all entrypoint-facing errors.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    IndexOutOfBounds = 3,
    AlreadyReleased = 4,
    EmptyRefundRequest = 6,
    DuplicateMilestoneInRefund = 7,
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
    ContractIdCollision = 27,
    ContractIdOverflow = 28,
    EmptyComment = 29,
    CommentTooLong = 30,

    // ── Governance validation errors ─────────────────────────────────────────

    /// Returned by `propose_governance_admin` when the caller proposes their
    /// own address as the new admin. Self-transfer of admin rights is a
    /// no-op that would leave the two-step timelock protectionless.
    CannotProposeSelf = 31,

    /// Returned by `cancel_governance_admin_proposal` and
    /// `accept_governance_admin` when no pending proposal exists in storage.
    NoPendingAdminProposal = 32,

    /// Returned by `accept_governance_admin` when the minimum ledger delay
    /// since the corresponding proposal has not yet elapsed.
    TimelockNotElapsed = 33,

    /// Returned by `set_governed_params` when protocol_fee_bps exceeds 10 000
    /// (i.e. 100%) or when max_escrow_total_stroops is not strictly positive.
    InvalidProtocolParameters = 34,
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

// Removed duplicate DataKey

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
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Reputation {
    pub completed_contracts: i128,
    pub total_rating: i128,
    pub last_rating: i128,
}
