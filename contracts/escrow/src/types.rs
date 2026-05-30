use soroban_sdk::{contracterror, contracttype, Address, String, Vec};

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
    // Client migration
    PendingClientMigration(u32),
    // Protocol / governance
    ProtocolFeeBps,
    AccumulatedProtocolFees,
    ReadinessChecklist,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    UnauthorizedRole = 6,
    InvalidStatusTransition = 7,
    AlreadyCancelled = 8,
    ContractNotFound = 9,
    MilestonesAlreadyReleased = 10,
    TooManyMilestones = 11,
    NotCompleted = 12,
    InvalidRating = 13,
    DuplicateRating = 14,
    AlreadyFinalized = 15,
    NotReadyForFinalization = 16,
    AlreadyReleased = 17,
    InsufficientFunds = 18,
    SelfRating = 19,
    CommentTooLong = 20,
    EmptyComment = 21,
    AmountMustBePositive = 22,
    FundingExceedsRequired = 23,
    InvalidState = 24,
    InsufficientEscrowBalance = 25,
    MilestoneNotFound = 26,
    AlreadyApproved = 27,
    ReputationAlreadyIssued = 28,
    // Pause / emergency controls
    ContractPaused = 29,
    EmergencyActive = 30,
    NotInitialized = 31,
    AlreadyInitialized = 32,
    // Additional errors referenced in tests
    FreelancerMismatch = 33,
    EmptyRefundRequest = 34,
    DuplicateMilestoneInRefund = 35,
    PotentialOverflow = 36,
    NonPositiveAmount = 37,
    AmountExceedsMaximum = 38,
    InvalidStroopPrecision = 39,
    ExceedsContractMaximum = 40,
    ExactDepositRequired = 41,
    DepositWouldExceedTotal = 42,
    AccountingInvariantViolated = 43,
}

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

// ─── Indexer summary types ────────────────────────────────────────────────────

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

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepositMode {
    ExactTotal = 0,
    Incremental = 1,
}
