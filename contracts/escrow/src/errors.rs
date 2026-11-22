//! Escrow error definitions.
//! Public re-exported from the parent `lib.rs` module.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    Paused = 4,
    ContractNotFound = 5,
    InvalidParties = 6,
    InvalidMilestones = 7,
    InvalidAmount = 8,
    OverFunded = 9,
    InvalidStatus = 10,
    MilestoneNotFound = 11,
    MilestoneAlreadyReleased = 12,
    InsufficientFunds = 13,
    InvalidSplit = 14,
    InvalidRating = 15,
    ReputationAlreadyIssued = 16,
    ArithmeticOverflow = 17,
    RatingNotFound = 18,
    RatingAlreadyVoided = 19,
    InvalidTransition = 20,
    NoSurplus = 21,
    EscrowFrozen = 22,
    AdminCannotBeParty = 23,
    MilestoneTooSmall = 24,
    NotCompactable = 25,
    EscrowCompacted = 26,
    InvalidSplitConfig = 27,
    OutOfOrder = 28,
    DuplicateIndex = 29,
    BelowReputationThreshold = 30,
    InvalidPanel = 31,
    PanelRequired = 32,
    RoleUnchanged = 33,
    TooManyFunders = 34,
    FundingExpired = 35,
    NotExpired = 36,
    ChangeOrderNotFound = 37,
    InvalidMetadata = 38,
    NoPendingUpgrade = 39,
    UpgradeNotReady = 40,
    InvalidReviewWindow = 41,
    MilestoneNotSubmitted = 42,
    ReviewWindowActive = 43,
    TransferAmountMismatch = 44,
    InvalidAdmin = 45,
    NoPendingAdmin = 46,
    EmergencyExitNotAvailable = 47,
    SettlementMismatch = 48,
    FeeTooHigh = 49,
    InvalidSchedule = 50,
    // NOTE: the Soroban contract spec caps an error enum at 50 cases, so new
    // failure modes reuse the closest existing code (append-only numbering).
}