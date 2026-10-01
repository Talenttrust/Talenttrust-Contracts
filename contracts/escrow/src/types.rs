use soroban_sdk::{contracterror, contracttype, Address, BytesN, String, Vec};

// ── Indexer summary types ────────────────────────────────────────────────────

#[allow(dead_code)]
pub const CONTRACT_SUMMARY_SCHEMA_VERSION: u32 = 1;

/// Current on-ledger layout version for per-contract dispute metadata.
///
/// Bump this when introducing a new `DisputeMetadata` layout. Older layouts are
/// upgraded on read by `dispute::load_dispute_metadata`.
pub const DISPUTE_STORAGE_VERSION: u32 = 1;

/// Current on-ledger layout version for reputation storage.
///
/// v1 = legacy layout (only `DataKey::Reputation` present, no version marker).
/// v2 = current layout (`DataKey::Reputation` + `DataKey::ReputationStorageVersion`).
/// State writes belong exclusively in `migrate_reputation_storage` and
/// `issue_reputation`; getters must stay read-only.
pub const REPUTATION_STORAGE_VERSION: u32 = 2;

/// Legacy (v0) dispute metadata layout without an embedded schema version.
///
/// Retained solely so migrate-on-read can decode pre-versioned records and
/// rewrite them as [`DisputeMetadata`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeMetadataV0 {
    pub raised_by: Address,
    pub reason_hash: BytesN<32>,
    pub raised_at: u64,
}

/// Versioned dispute metadata stored under [`DataKey::Dispute`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeMetadata {
    /// Must equal [`DISPUTE_STORAGE_VERSION`] after a successful write/migration.
    pub schema_version: u32,
    pub raised_by: Address,
    pub reason_hash: BytesN<32>,
    pub raised_at: u64,
}

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

// ── Pause scope types ────────────────────────────────────────────────────────

/// Determines which entrypoints are blocked when a pause is active.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PauseTarget {
    /// Block payout operations (release, refund, cancel).
    Payout = 1,
    /// Block dispute operations (raise, resolve, rollback).
    Dispute = 2,
    /// Block all mutating entrypoints (default legacy behavior).
    Global = 3,
}

/// Scoped pause state stored under [`DataKey::PauseScope`].
///
/// Replaces the bare `bool` previously stored under `DataKey::Paused`.
/// The `None` variant (absent storage key) means unpaused.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseScope {
    pub target: PauseTarget,
    /// Human-readable reason for the pause (e.g. "security incident").
    pub reason: String,
    /// Ledger sequence when the pause was activated.
    pub paused_at: u64,
}

// ── Storage keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    // Admin / pause / emergency
    Initialized,
    Admin,
    Paused,
    /// Scoped pause state (PauseScope struct). Replaces bare bool.
    PauseScope,
    /// Monotonic admin nonce for replay protection.
    AdminNonce,
    /// Monotonic admin-rotation revision, kept with the live contract instance.
    AdminRotationRevision,
    Emergency,
    // Contract storage
    Contract(u32),
    /// Per-record schema version written alongside each `Contract` entry.
    ///
    /// Absent means the record was written before versioning existed (schema v1).
    /// When present, the value must equal [`crate::migration::CONTRACT_STORAGE_SCHEMA_VERSION`]
    /// for the record to be considered current.
    ContractSchemaVersion(u32),
    NextContractId,
    MilestoneReleased(u32, u32),
    MilestoneApprovals(u32, u32),
    // Events / Indexing
    Event(u32),
    NextEventId,
    // Reputation
    ReputationIssued(u32),
    PendingReputationCredits(Address),
    Reputation(Address),
    ReputationComment(u32),
    /// Schema version marker for `Reputation(address)` (stored as u32).
    /// Absent = v1 legacy layout. Present and equal to
    /// [`REPUTATION_STORAGE_VERSION`](crate::REPUTATION_STORAGE_VERSION) = v2 current.
    ReputationStorageVersion(Address),
    /// Index of addresses that have reputation records. Used by paginated readers.
    ReputationIndex,
    // Client migration
    PendingClientMigration(u32),
    // Protocol / governance
    ProtocolParameters,
    ProtocolFeeBps,
    // Two-step admin transfer: pending admin stored here while proposal awaits acceptance
    PendingAdmin,
    AccumulatedProtocolFees,
    GovernedParameters,
    ReadinessChecklist,
    // Configurable limits
    MaxMilestones,
    MaxEscrowStroops,
    MaxArbiters,
    ContractsParameters,
    MaxSettlement,
    // Finalization
    Finalization(u32),
    // Settlement token
    SettlementToken,
    // Dispute / arbiter configuration
    DisputeRollback(u32),
    DisputeConfigKey,
    Dispute(u32),
    // Reputation configuration
    ReputationConfigKey,
    ClientContracts(Address),
    FreelancerContracts(Address),
    // Milestone transition versioning and audit trail (Issue #1340)
    /// Version number for a milestone, incremented on each successful transition.
    /// Used for optimistic concurrency control to detect concurrent modifications.
    MilestoneVersion(u32, u32), // (contract_id, milestone_index) -> u32
    /// The address of the party that last successfully transitioned this milestone.
    /// Used for audit trail and accountability.
    MilestoneLastModifiedBy(u32, u32), // (contract_id, milestone_index) -> Address
    // Fee withdrawal rate-limiting
    /// Maximum fraction of accumulated fees that can be withdrawn in one call,
    /// expressed in basis points (10 000 = 100 %). Default: 5 000 = 50 %.
    FeeWithdrawalCap,
    /// Minimum number of ledgers that must elapse between successful
    /// protocol-fee withdrawals. Stored as `u32`.
    FeeWithdrawalCooldownLedgers,
    /// Ledger sequence number of the last successful protocol-fee withdrawal.
    LastFeeWithdrawalLedger,
    /// Storage layout / schema version for the escrow contract (stored as u32).
    SchemaVersion,
    // Two-step governance proposals for high-impact overrides (#1221)
    /// A pending governance override proposal, keyed by a monotonic u64 proposal ID.
    GovernanceProposal(u64),
    /// Monotonic counter used to generate unique governance proposal IDs.
    NextGovernanceProposalId,
    // Token scale (#1346)
    /// Number of decimal places for the bound settlement token (stored as u32).
    ///
    /// Captured once at `bind_settlement_token` time from `token::Client::decimals()`.
    /// All milestone amounts must be exactly representable at this scale (i.e.
    /// `amount % 10^decimals == 0` when interpreted as a human-visible value).
    TokenScale,
    // Concurrent-execution hardening (#1535)
    /// Per-contract mutation lock marking that a storage-mutating entrypoint is
    /// currently in flight.  Written before the first state change and removed
    /// when the guarded scope ends; never present outside an active call, so an
    /// unexpired entry here means a mutation is in progress.  Stored as `bool`.
    ContractMutationLock(u32),
    /// Ledger-scoped guard used by `create_contract_guard` to prevent concurrent
    /// contract creation within the same ledger.  Stored as a `u32` ledger sequence.
    ContractCreationGuard,
    /// Admin-initiated key-failure recovery record for a specific contract.
    KeyRecovery(u32),
}

// ── Two-step Governance Proposal (Issue #1221) ───────────────────────────────

/// Identifies which high-impact parameter the proposal targets.
///
/// Each variant carries the new value that would be applied on acceptance, so
/// the approver can inspect what they are authorising before signing.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GovernanceProposalKind {
    /// Proposal to change the protocol fee in basis points.
    SetProtocolFeeBps(u32),
    /// Proposal to atomically change both governed parameters.
    SetGovernedParams(GovernedParameters),
    /// Proposal to change the fee-withdrawal cap in basis points.
    SetFeeWithdrawalCap(u32),
    /// Proposal to change the fee-withdrawal cooldown in ledgers.
    SetFeeWithdrawalCooldown(u32),
    /// Proposal to change the maximum milestones per contract.
    SetMaxMilestones(u32),
}

/// The lifecycle state of a governance proposal.
///
/// Transitions:
/// `Pending` → `Approved` (approver calls `approve_governance_proposal`)
/// `Pending` → `Rejected` (approver calls `reject_governance_proposal`)
/// `Approved` → `Applied` (admin calls `apply_governance_proposal`)
/// `Pending` | `Approved` → expired (TTL elapses; enforced on read)
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GovernanceProposalState {
    /// Proposal has been submitted and is awaiting approver action.
    Pending = 0,
    /// The approver has authorised the proposal; the admin may now apply it.
    Approved = 1,
    /// The approver has explicitly rejected the proposal.
    Rejected = 2,
    /// The proposal has been applied; the parameter change is live.
    Applied = 3,
}

/// A governance override proposal stored under `DataKey::GovernanceProposal(id)`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernanceProposal {
    /// Monotonic ID assigned at request time.
    pub proposal_id: u64,
    /// The admin who submitted the proposal.
    pub requester: Address,
    /// Current lifecycle state.
    pub state: GovernanceProposalState,
    /// The specific parameter change being proposed.
    pub kind: GovernanceProposalKind,
    /// Ledger sequence at which the proposal was created.
    pub proposed_at_ledger: u32,
    /// Ledger sequence after which the proposal expires.
    /// Once `env.ledger().sequence() > expires_at_ledger`, actions are rejected.
    pub expires_at_ledger: u32,
    /// The address of the approver, if an approval (or rejection) has been recorded.
    pub approver: Option<Address>,
}

// ── Event Types ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventEntry {
    pub contract_id: u32,
    pub status: u32,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub total_deposited: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneIndexEvent {
    pub amount: i128,
    pub released: bool,
    pub refunded: bool,
    pub timestamp: u64,
}

// ── Canonical Errors ─────────────────────────────────────────────────────────

/// Canonical contract error type for all entrypoint-facing errors.
#[contracterror(export = false)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    TooManyMilestones = 1,
    LimitOutOfRange = 2,
    IndexOutOfBounds = 3,
    InvalidContractId = 4,
    /// The refund request is empty.
    EmptyRefundRequest = 6,
    DuplicateMilestoneInRefund = 7,
    /// The milestone has already been refunded.
    AlreadyRefunded = 8,
    InsufficientFunds = 9,
    ContractNotFound = 10,
    UnauthorizedRole = 11,
    MissingArbiter = 12,
    InvalidArbiter = 13,
    AmountMustBePositive = 15,
    InvalidState = 16,
    MilestoneAlreadyReleased = 17,
    AlreadyApproved = 18,
    InvalidParticipant = 19,
    InsufficientApprovals = 20,
    InvalidRating = 22,
    ReputationAlreadyIssued = 23,
    EmptyMilestones = 25,
    InvalidMilestoneAmount = 26,
    /// A contract with the specified ID already exists.
    ContractIdCollision = 27,
    ContractIdOverflow = 28,
    EmptyComment = 29,
    CommentTooLong = 30,
    /// The contract has already been initialized.
    AlreadyInitialized = 34,
    InsufficientAccumulatedFees = 35,
    NotInitialized = 36,
    ContractPaused = 37,
    EmergencyActive = 38,
    NotCompleted = 40,
    InvalidStatusTransition = 41,
    ArbiterRequired = 42,
    InvalidDisputeSplit = 43,
    AccountingInvariantViolated = 44,
    PotentialOverflow = 45,
    AlreadyFinalized = 46,
    /// The work evidence string exceeds the maximum length limit.
    EvidenceTooLong = 47,
    TimelockNotElapsed = 48,
    InvalidProtocolParameters = 49,
    /// No settlement token has been bound for custody transfers.
    SettlementTokenNotConfigured = 52,
    MilestoneNotOverdue = 53,
    /// The work evidence string is empty; at least one byte is required.
    EmptyEvidence = 54,
    /// No safe rollback is available for the contract's current state.
    RollbackNotAllowed = 55,
    RoleOverlap = 57,
    /// No dispute record exists for the requested contract.
    DisputeNotFound = 60,
    SettlementTokenAlreadyBound = 61,
    ContractCancelled = 62,
    InvalidDepositAmount = 65,
    /// The requested withdrawal amount exceeds the configured per-withdrawal cap.
    FeeWithdrawalCapExceeded = 66,
    /// A protocol-fee withdrawal was attempted before the cooldown interval elapsed.
    FeeWithdrawalCooldownActive = 67,
    /// A pending admin proposal was not accepted within
    /// `ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS` of being proposed and must be
    /// re-proposed.
    AdminProposalExpired = 68,
    /// `propose_admin` was called with the current admin's own address.
    CannotProposeSelf = 69,
    /// The milestone has pending release approvals; deliverable metadata
    /// cannot be changed after acceptance.
    EvidenceLocked = 70,
    /// The batch of milestone indices is empty.
    EmptyBatch = 71,
    /// The batch exceeds the maximum allowed milestone release count.
    BatchLimitExceeded = 72,
    /// The batch contains duplicate milestone indices.
    DuplicateMilestoneInBatch = 73,
    /// Pause scope guard failed (the operation is not covered by the active pause target).
    PauseScopeActive = 74,
    /// The migration version does not match the expected on-ledger schema version.
    InvalidMigrationVersion = 75,
    /// The admin nonce does not match the expected replay-protection counter.
    StaleNonce = 76,
    // Two-step governance proposal errors (#1221)
    /// The governance proposal was not found (wrong ID or expired and evicted).
    GovernanceProposalNotFound = 77,
    /// The proposal has already been approved, rejected, or applied and cannot
    /// transition further in the current direction.
    GovernanceProposalInvalidState = 78,
    /// The proposal has passed its expiry ledger and can no longer be approved or applied.
    GovernanceProposalExpired = 79,
    /// The approver identity is the same as the requester; self-approval is prohibited.
    GovernanceSelfApproval = 80,
    // Token scale errors (#1346)
    /// The settlement token has not had its scale recorded yet.
    /// Call `bind_settlement_token` before creating contracts.
    TokenScaleNotSet = 81,
    /// The milestone amount is not exactly representable at the token's decimal
    /// scale — it would require fractional token units below the minimum denomination.
    FractionalTokenAmount = 82,
    /// The token bound to this contract has a different decimal scale than the
    /// one recorded at contract-creation time.  Re-binding with a different
    /// token scale is not allowed after contracts exist.
    TokenScaleMismatch = 83,
    /// A mutation was submitted against an obsolete milestone version.
    StaleMilestoneVersion = 84,
    /// The expected-version vector does not match the milestone-index vector.
    InvalidVersionCount = 85,
    // Concurrent-execution hardening (#1535)
    /// A storage-mutating operation for this contract is already in flight.
    ///
    /// Raised when a re-entrant or interleaved call tries to acquire the
    /// per-contract mutation lock while another mutation still holds it.  On
    /// Soroban a transaction is atomic, so seeing this error means a token
    /// callback re-entered the escrow mid-transfer, or a caller issued two
    /// overlapping mutations for the same contract.
    ConcurrentMutation = 86,
    /// The persisted contract record violates a storage invariant.
    ///
    /// Raised when a loaded contract, or the record about to be written, has
    /// negative accounting fields, a released/refunded sum that exceeds the
    /// funded amount, or a milestone in an impossible state.  A healthy
    /// contract can never produce this error, so it always signals corrupted
    /// or stale on-ledger state discovered before it could be propagated.
    StorageInvariantViolated = 87,
}

// ── Core contract state ──────────────────────────────────────────────────────

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

// ── Simulate / dry-run result types ───────────────────────────────────────────

/// Projected outcome of a `release_milestone` dry-run.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulatedRelease {
    /// Whether the release would succeed (all validation checks pass).
    pub would_succeed: bool,
    /// If `would_succeed` is false, the numeric error code that would be emitted.
    pub error_code: Option<u32>,
    /// The gross milestone amount before any deduction.
    pub gross_amount: i128,
    /// The net amount that would be transferred to the freelancer (gross minus fee).
    pub net_amount: i128,
    /// The protocol fee that would be retained from this release.
    pub protocol_fee: i128,
    /// The projected `released_amount` on the contract after release.
    pub projected_released_amount: i128,
    /// Whether releasing this milestone would complete the contract.
    pub would_complete_contract: bool,
}

/// Projected outcome of a `deposit_funds` dry-run.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulatedDeposit {
    /// The `funded_amount` before the deposit.
    pub current_funded_amount: i128,
    /// The projected `funded_amount` after the deposit.
    pub new_funded_amount: i128,
    /// The projected contract status after the deposit.
    pub projected_status: ContractStatus,
    /// The total value of all milestones (used to determine Funded vs PartiallyFunded).
    pub total_milestone_amount: i128,
}

/// Projected outcome of a `create_contract` dry-run.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulateCreateContractOutcome {
    /// The contract ID that would be assigned.
    pub contract_id: u32,
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub release_authorization: ReleaseAuthorization,
    /// Milestone amounts as submitted.
    pub milestones: Vec<i128>,
    /// The sum of all milestone amounts.
    pub total_amount: i128,
}

/// Projected outcome of a `refund_unreleased_milestones` dry-run.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulatedRefund {
    /// Whether the refund would succeed (all validation checks pass).
    pub would_succeed: bool,
    /// If `would_succeed` is false, the numeric error code that would be emitted.
    pub error_code: Option<u32>,
    /// The total amount that would be refunded to the client.
    pub total_refund_amount: i128,
    /// The projected contract status after the refund.
    pub projected_status: ContractStatus,
    /// The projected `refunded_amount` on the contract after the refund.
    pub projected_refunded_amount: i128,
    /// Whether refunding these milestones would cause all milestones to be
    /// either released or refunded (i.e., the contract would become terminal).
    pub would_complete_contract: bool,
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

// ── Settlement state snapshot ────────────────────────────────────────────────

/// Read-only projection of the settlement layer returned by `get_settlement_state`.
///
/// Aggregates the two independent settlement storage keys into a single,
/// versioned snapshot so callers (indexers, clients, off-chain tooling) can
/// read the full settlement configuration in one call.
///
/// ## Invariants
///
/// * `token` is `Some` after a successful `bind_settlement_token` and `None`
///   before any binding.  Once set, it is immutable — it can never revert to
///   `None`.
/// * `accumulated_protocol_fees` is always `>= 0`.  Negative values indicate
///   an accounting invariant violation and should never occur in production.
/// * This type is **read-only**: `get_settlement_state` never mutates storage.
///
/// ## Compatibility
///
/// New fields may be added (with defaults) in future schema versions. Existing
/// consumers should ignore unknown fields if deserialising from raw ledger XDR.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementState {
    /// The bound SAC token address, or `None` when no token has been bound.
    pub token: Option<Address>,
    /// Total protocol fees accumulated from milestone releases, in stroops.
    ///
    /// Defaults to `0` before any milestone is released. Always non-negative.
    pub accumulated_protocol_fees: i128,
}

impl Default for SettlementState {
    fn default() -> Self {
        SettlementState {
            token: None,
            accumulated_protocol_fees: 0,
        }
    }
}

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

/// Read-only diagnostic view explaining *why* a milestone is or is not releasable.
///
/// This type exists so that a failure to release is actionable by the caller
/// without guessing. [`AuthorizationRecord`] reports which parties have
/// approved, but it cannot report the contract's
/// [`ReleaseAuthorization`] mode, the milestone's terminal state, or how many
/// approvals are still outstanding. This view resolves all of that in a single
/// read so an integrator can distinguish the four terminal/exhausted outcomes:
///
/// * `released` / `refunded` — the milestone is settled; no further approval is useful.
/// * `release_authorized` — approvals are sufficient right now.
/// * `has_record == false` while the milestone is open — no live approval record
///   exists, which covers both "never approved" and "the temporary record was
///   evicted after `PENDING_APPROVAL_TTL_LEDGERS`". Both require a fresh
///   approval, so callers need not (and cannot) distinguish them.
/// * `approvals_missing > 0` with `has_record == true` — a partially satisfied
///   set is live; the named parties can still approve, or withdraw via
///   `revoke_milestone_approval`.
///
/// The view is derived entirely from stored state and never mutates storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneReleaseReadiness {
    pub milestone_index: u32,
    /// `true` when `check_approvals` would currently succeed for this milestone.
    pub release_authorized: bool,
    /// `true` when a live temporary approval record exists for this milestone.
    pub has_record: bool,
    /// Number of approvals required by the contract's `ReleaseAuthorization` mode.
    pub approvals_required: u32,
    /// Number of required approvals currently present.
    pub approvals_present: u32,
    /// Number of required approvals still outstanding. `0` when none are missing.
    pub approvals_missing: u32,
    /// `true` when the milestone has already been released (terminal).
    pub released: bool,
    /// `true` when the milestone has already been refunded (terminal).
    pub refunded: bool,
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
            max_milestones: crate::DEFAULT_MAX_MILESTONES,
            max_escrow_stroops: crate::DEFAULT_MAX_TOTAL_ESCROW_STROOPS,
        }
    }
}

/// Stores a pending admin proposal with the proposed address
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

/// Lightweight reputation entry returned by the paginated reputations view.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationEntry {
    pub account: Address,
    pub completed_contracts: i128,
    pub total_rating: i128,
    pub last_rating: i128,
}

/// Runtime-configurable reputation validation parameters, stored under
/// [`DataKey::ReputationConfigKey`].
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

/// Projected outcome of a dispute resolution for dry-run simulation.
///
/// This type is returned by `simulate_dispute_resolution`, the read-only
/// dry-run variant of `resolve_dispute`. It carries the projected accounting
/// changes and final status without writing storage or emitting events.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulateDisputeOutcome {
    /// Amount that would be refunded to the client.
    pub client_payout: i128,
    /// Amount that would be released to the freelancer.
    pub freelancer_payout: i128,
    /// Projected final contract status after applying the resolution.
    pub final_status: ContractStatus,
    /// Projected `refunded_amount` after the resolution.
    pub new_refunded_amount: i128,
    /// Projected `released_amount` after the resolution.
    pub new_released_amount: i128,
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

/// Represents the milestone progress of an escrow contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneProgress {
    /// The number of completed (released) milestones.
    pub completed: u32,
    /// The total number of milestones.
    pub total: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeSummary {
    pub contract_id: u32,
    pub status: ContractStatus,
    pub total_deposited: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
}

/// Configuration for the arbiter's partial-refund split, stored under
/// [`DataKey::DisputeConfigKey`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeConfig {
    pub partial_refund_freelancer_bps: u32,
    pub partial_refund_client_bps: u32,
}

impl Default for DisputeConfig {
    fn default() -> Self {
        DisputeConfig {
            partial_refund_freelancer_bps: crate::dispute::DEFAULT_DISPUTE_FREELANCER_BPS,
            partial_refund_client_bps: crate::dispute::DEFAULT_DISPUTE_CLIENT_BPS,
        }
    }
}
