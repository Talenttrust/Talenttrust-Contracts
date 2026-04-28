use soroban_sdk::{contracterror, contracttype, Address, Bytes, String, Vec};

#[contracttype]
pub enum DataKey {
    Client,
    Freelancer,
    Milestones,
    Initialized,
    MilestoneFunded(u32),
    Admin,
    ProtocolFeeBps,
    AccumulatedProtocolFees,
    ReadinessChecklist,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    IndexOutOfBounds = 3,
    AlreadyReleased = 4,
    InvalidStatusTransition = 5,
    InsufficientMilestoneFunding = 6,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
    Cancelled = 4,
    Refunded = 5,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
    pub refunded: bool,
    pub work_evidence: Option<String>,
    pub funded_amount: i128,
    /// Amount refunded for this specific milestone (≤ amount).
    pub refunded_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneFunding {
    pub contract_id: u32,
    pub milestone_idx: u32,
    pub funded_amount: i128,
}

/// Readiness checklist stored under [`DataKey::ReadinessChecklist`].
///
/// Tracks whether key one-time lifecycle operations have been executed.
/// All fields default to `false` on a fresh contract.
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

// ─── Indexer summary types ────────────────────────────────────────────────────

/// Schema version stamped on every [`ContractSummary`].
///
/// Increment this constant whenever [`ContractSummary`] changes in a
/// backwards-incompatible way so that consumers can gate on the version field.
pub const CONTRACT_SUMMARY_SCHEMA_VERSION: u32 = 1;

/// Compact, per-milestone state included in a [`ContractSummary`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneSummary {
    /// Zero-based position of this milestone in the contract.
    pub index: u32,
    /// Agreed value of this milestone in stroops.
    pub amount: i128,
    /// `true` once the client has released this milestone to the freelancer.
    pub released: bool,
    /// `true` once this milestone has been refunded back to the client.
    pub refunded: bool,
}

/// A self-contained, single-read snapshot of an escrow contract for indexers.
///
/// Returned by `Escrow::get_contract_summary`. Combines contract roles,
/// lifecycle status, financial totals, and per-milestone state into one
/// atomic call so that indexing pipelines do not need multiple round-trips.
///
/// # Stability / versioning
///
/// `schema_version` is set to [`CONTRACT_SUMMARY_SCHEMA_VERSION`] (`1`) in
/// this release. If the struct layout changes in a breaking way the constant
/// is incremented. Consumers should reject or re-fetch summaries whose
/// `schema_version` they do not recognise.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractSummary {
    // ── Versioning ────────────────────────────────────────────────────────────
    /// Monotonically increasing layout version.
    /// See [`CONTRACT_SUMMARY_SCHEMA_VERSION`].
    pub schema_version: u32,

    // ── Roles ─────────────────────────────────────────────────────────────────
    /// Address of the client who funds the contract.
    pub client: Address,
    /// Address of the freelancer who performs the work.
    pub freelancer: Address,
    /// Optional third-party arbiter for dispute resolution.
    pub arbiter: Option<Address>,

    // ── Lifecycle ─────────────────────────────────────────────────────────────
    /// Current contract lifecycle status.
    pub status: ContractStatus,
    /// Whether a reputation score has already been issued for this contract.
    pub reputation_issued: bool,

    // ── Financial totals ──────────────────────────────────────────────────────
    /// Sum of all milestone amounts (agreed contract value), in stroops.
    pub total_amount: i128,
    /// Cumulative amount deposited by the client so far, in stroops.
    pub funded_amount: i128,
    /// Cumulative amount released to the freelancer so far, in stroops.
    pub released_amount: i128,
    /// Balance not yet released or refunded, in stroops.
    pub refundable_balance: i128,

    // ── Milestones ────────────────────────────────────────────────────────────
    /// Number of milestones that have been released to the freelancer.
    pub released_milestone_count: u32,
    /// Per-milestone breakdown (index, amount, `released`, `refunded`).
    pub milestones: Vec<MilestoneSummary>,
}
