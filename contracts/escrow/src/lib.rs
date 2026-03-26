#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, symbol_short, vec, Address, Env, Symbol, Vec, String};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default minimum milestone amount (1 stroop).
pub const DEFAULT_MIN_MILESTONE_AMOUNT: i128 = 1;

/// Default maximum number of milestones per contract.
pub const DEFAULT_MAX_MILESTONES: u32 = 16;

/// Default minimum reputation rating.
pub const DEFAULT_MIN_REPUTATION_RATING: i128 = 1;

/// Default maximum reputation rating.
pub const DEFAULT_MAX_REPUTATION_RATING: i128 = 5;

/// Default protocol fee in basis points (2.5 %).
pub const DEFAULT_FEE_BASIS_POINTS: u32 = 250;

/// Default timeout duration: 30 days in seconds (30 * 24 * 60 * 60).
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 2_592_000;

/// Minimum timeout duration: 1 day in seconds.
pub const MIN_TIMEOUT_SECONDS: u64 = 86_400;

/// Maximum timeout duration: 365 days in seconds.
pub const MAX_TIMEOUT_SECONDS: u64 = 31_536_000;

// ---------------------------------------------------------------------------
// Schedule metadata constants
// ---------------------------------------------------------------------------

/// Maximum length (in bytes) allowed for a milestone title string.
pub const MAX_SCHEDULE_TITLE_LEN: u32 = 128;

/// Maximum length (in bytes) allowed for a milestone description string.
pub const MAX_SCHEDULE_DESCRIPTION_LEN: u32 = 512;

// ---------------------------------------------------------------------------
// Data keys
// ---------------------------------------------------------------------------

/// Storage keys used throughout the contract.
///
/// Each variant maps to a distinct slot in Soroban persistent storage.
/// Keys that carry a `u32` are per-contract or per-milestone; keys that
/// carry an `Address` are per-participant.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Pause-control administrator address.
    Admin,
    /// Whether the contract is currently paused.
    Paused,
    /// Whether an emergency pause is active (blocks `unpause`).
    EmergencyPaused,
    /// Auto-incrementing counter for escrow contract IDs.
    NextContractId,
    /// Stored [`EscrowContractData`] keyed by contract ID.
    Contract(u32),
    /// Reputation record keyed by freelancer address.
    Reputation(Address),
    /// Number of reputation credits a freelancer may redeem.
    PendingReputationCredits(Address),
    /// Current governance admin.
    GovernanceAdmin,
    /// Pending (not yet accepted) governance admin.
    PendingGovernanceAdmin,
    /// Live protocol parameters.
    ProtocolParameters,
    /// Optional schedule metadata stored per milestone (contract_id, milestone_index).
    MilestoneSchedule(u32, u32),
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Release-authorization scheme for milestone payments.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    /// Only the client may approve a release.
    ClientOnly,
    /// Only the designated arbiter may approve a release.
    ArbiterOnly,
    /// Either the client or the arbiter may approve.
    ClientAndArbiter,
    /// Both the client and at least one other party must approve (multi-sig).
    MultiSig,
}

/// Lifecycle status of an escrow contract.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    /// Contract has been created but not yet funded.
    Created = 0,
    /// Contract has been fully funded.
    Funded = 1,
    /// All milestones have been released; contract is complete.
    Completed = 2,
    /// A dispute has been raised.
    Disputed = 3,
    /// Dispute is actively being adjudicated.
    InDispute = 4,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Canonical error codes returned by the escrow contract.
///
/// Each variant maps to a stable `u32` discriminant that is included in
/// Soroban error events and is visible to off-chain clients.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    /// Supplied contract ID is zero or does not exist.
    InvalidContractId = 1,
    /// Milestone index is out of range.
    InvalidMilestoneId = 2,
    /// An amount field is zero or negative when a positive value is required.
    AmountMustBePositive = 3,
    /// Reputation rating is outside the configured [`min`, `max`] range.
    InvalidRating = 4,
    /// The milestone list supplied to `create_contract` is empty.
    EmptyMilestones = 5,
    /// Client and freelancer resolved to the same address.
    InvalidParticipants = 6,
    /// An individual milestone amount violates the minimum amount rule.
    InvalidMilestoneAmount = 7,
    /// Deposit or release amount is invalid in context.
    InvalidAmount = 8,
    /// The deposit would exceed the total required amount.
    FundingExceedsRequired = 9,
    /// The contract is not in the expected lifecycle state for this operation.
    InvalidState = 10,
    /// The escrow balance is insufficient to release the requested milestone.
    InsufficientEscrowBalance = 11,
    /// The milestone has already been released.
    MilestoneAlreadyReleased = 12,
    /// Requested milestone was not found.
    MilestoneNotFound = 13,
    /// Reputation has already been issued for this contract.
    ReputationAlreadyIssued = 14,
    /// No contract record exists for the given ID.
    ContractNotFound = 15,

    // --- Schedule metadata errors (16–20) ---

    /// A supplied `due_date` timestamp lies in the past.
    ///
    /// Schedule metadata due dates must be strictly in the future at the time
    /// `create_contract` (or `set_milestone_schedule`) is called.
    ScheduleDueDateInPast = 16,

    /// A later milestone has an earlier (or equal) `due_date` than a preceding one.
    ///
    /// Milestone due dates must be strictly monotonically increasing so that
    /// the schedule is self-consistent.
    ScheduleDatesNotMonotonic = 17,

    /// A `title` or `description` string in the schedule metadata exceeds the
    /// maximum allowed length.
    ScheduleStringTooLong = 18,

    /// The caller attempted to overwrite already-released milestone schedule
    /// metadata, which is immutable once the milestone is paid out.
    ScheduleImmutableAfterRelease = 19,

    /// The milestone index supplied to `set_milestone_schedule` is out of range
    /// for the given contract.
    ScheduleInvalidMilestoneIndex = 20,
}

// ---------------------------------------------------------------------------
// Schedule metadata types
// ---------------------------------------------------------------------------

/// Optional scheduling information attached to a single milestone.
///
/// All fields are optional so that callers may supply as much or as little
/// context as needed without being forced to provide a full record.
///
/// # Invariants enforced at write time
///
/// * `due_date`, if present, must be **strictly greater than** the ledger
///   timestamp at the time the metadata is written.
/// * `due_date` values across milestones must be **strictly monotonically
///   increasing** (i.e. milestone N+1's due date > milestone N's due date).
/// * `title` must not exceed [`MAX_SCHEDULE_TITLE_LEN`] bytes.
/// * `description` must not exceed [`MAX_SCHEDULE_DESCRIPTION_LEN`] bytes.
/// * Once a milestone is released (`Milestone::released == true`) its schedule
///   entry is **immutable** — further writes are rejected with
///   [`EscrowError::ScheduleImmutableAfterRelease`].
///
/// # Security notes
///
/// * `due_date` is informational / off-chain signalling only; the contract
///   does **not** automatically release or cancel milestones when a deadline
///   passes.  Enforcement is the responsibility of the calling application.
/// * String fields are length-bounded to prevent storage-exhaustion attacks.
/// * All validations occur inside [`Escrow::validate_schedule_metadata`], which
///   is called before any storage write, so partial writes are impossible.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneSchedule {
    /// Unix timestamp (seconds since epoch) by which this milestone should be
    /// completed.  Must be strictly greater than the ledger timestamp at the
    /// time of writing.
    pub due_date: Option<u64>,

    /// Human-readable short label for the milestone (e.g. "Design mockups").
    /// Maximum [`MAX_SCHEDULE_TITLE_LEN`] bytes.
    pub title: Option<String>,

    /// Extended description of the deliverable for this milestone.
    /// Maximum [`MAX_SCHEDULE_DESCRIPTION_LEN`] bytes.
    pub description: Option<String>,

    /// Ledger timestamp at which this metadata record was last written.
    /// Set automatically by the contract; callers must not supply this field.
    pub updated_at: u64,
}

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// Individual milestone tracked inside an escrow agreement.
///
/// # Invariants
///
/// * `released == true` is irreversible — once set it cannot be unset.
/// * `approved_by` records the address that most recently approved this
///   milestone for release.  In a [`ReleaseAuthorization::MultiSig`] flow the
///   field holds the *last* approver; the full approval set is tracked
///   separately if needed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    /// Payment amount in stroops allocated to this milestone.
    pub amount: i128,
    /// Whether the milestone payment has been released to the freelancer.
    pub released: bool,
    /// Address that most recently approved this milestone for release.
    pub approved_by: Option<Address>,
    /// Ledger timestamp of the most recent approval, if any.
    pub approval_timestamp: Option<u64>,
}

/// Full on-chain state of an escrow agreement.
///
/// Stored once per contract ID in persistent storage under
/// [`DataKey::Contract`].  Schedule metadata for individual milestones is
/// stored separately under [`DataKey::MilestoneSchedule`] to keep this record
/// compact and avoid unbounded growth.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    /// Address of the client who created and funded the escrow.
    pub client: Address,
    /// Address of the freelancer who will receive milestone payments.
    pub freelancer: Address,
    /// Optional third-party arbitration address.
    pub arbiter: Option<Address>,
    /// Ordered list of payment milestones.
    pub milestones: Vec<Milestone>,
    /// Sum of all milestone amounts (stroops).
    pub total_amount: i128,
    /// Amount deposited so far (stroops).
    pub funded_amount: i128,
    /// Amount released to the freelancer so far (stroops).
    pub released_amount: i128,
    /// Number of milestones that have been released.
    pub released_milestones: u32,
    /// Current lifecycle status.
    pub status: ContractStatus,
    /// Authorization scheme governing milestone releases.
    pub release_auth: ReleaseAuthorization,
    /// Whether a reputation credential has been issued for this contract.
    pub reputation_issued: bool,
    /// Ledger timestamp at which this contract was created.
    pub created_at: u64,
}

/// Reputation state derived from completed escrow contracts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    /// Total number of contracts for which reputation has been issued.
    pub completed_contracts: u32,
    /// Sum of all ratings received.
    pub total_rating: i128,
    /// Most recent rating received.
    pub last_rating: i128,
}

/// Governed protocol parameters used throughout escrow validation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolParameters {
    /// Minimum acceptable amount (stroops) for a single milestone.
    pub min_milestone_amount: i128,
    /// Maximum number of milestones allowed per contract.
    pub max_milestones: u32,
    /// Minimum valid reputation rating.
    pub min_reputation_rating: i128,
    /// Maximum valid reputation rating.
    pub max_reputation_rating: i128,
}

// ---------------------------------------------------------------------------
// Contract struct
// ---------------------------------------------------------------------------

#[contract]
pub struct Escrow;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

impl Escrow {
    // --- Pause helpers ---

    fn read_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("Pause controls are not initialized"))
    }

    fn require_admin(env: &Env) {
        let admin = Self::read_admin(env);
        admin.require_auth();
    }

    fn is_paused_internal(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn is_emergency_internal(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::EmergencyPaused)
            .unwrap_or(false)
    }

    fn ensure_not_paused(env: &Env) {
        if Self::is_paused_internal(env) {
            panic!("Contract is paused");
        }
    }

    // --- Contract storage helpers ---

    fn next_contract_id(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    }

    fn load_contract(env: &Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| panic!("contract not found"))
    }

    fn save_contract(env: &Env, contract_id: u32, contract: &EscrowContractData) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), contract);
    }

    // --- Reputation helpers ---

    fn add_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let key = DataKey::PendingReputationCredits(freelancer.clone());
        let current = env.storage().persistent().get::<_, u32>(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + 1));
    }

    // --- Governance helpers ---

    fn governance_admin(env: &Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::GovernanceAdmin)
            .unwrap_or_else(|| panic!("protocol governance is not initialized"))
    }

    fn pending_governance_admin(env: &Env) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingGovernanceAdmin)
    }

    fn protocol_parameters(env: &Env) -> ProtocolParameters {
        env.storage()
            .persistent()
            .get(&DataKey::ProtocolParameters)
            .unwrap_or_else(Self::default_protocol_parameters)
    }

    fn default_protocol_parameters() -> ProtocolParameters {
        ProtocolParameters {
            min_milestone_amount: DEFAULT_MIN_MILESTONE_AMOUNT,
            max_milestones: DEFAULT_MAX_MILESTONES,
            min_reputation_rating: DEFAULT_MIN_REPUTATION_RATING,
            max_reputation_rating: DEFAULT_MAX_REPUTATION_RATING,
        }
    }

    fn validated_protocol_parameters(
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> ProtocolParameters {
        if min_milestone_amount <= 0 {
            panic!("minimum milestone amount must be positive");
        }
        if max_milestones == 0 {
            panic!("maximum milestones must be positive");
        }
        if min_reputation_rating <= 0 {
            panic!("minimum reputation rating must be positive");
        }
        if min_reputation_rating > max_reputation_rating {
            panic!("reputation rating range is invalid");
        }
        ProtocolParameters {
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        }
    }

    fn all_milestones_released(milestones: &Vec<Milestone>) -> bool {
        let mut index = 0_u32;
        while index < milestones.len() {
            let milestone = milestones
                .get(index)
                .unwrap_or_else(|| panic!("missing milestone"));
            if !milestone.released {
                return false;
            }
            index += 1;
        }
        true
    }

    // --- Schedule metadata helpers ---

    /// Validates a single [`MilestoneSchedule`] entry in isolation.
    ///
    /// Checks:
    /// 1. `title` length ≤ [`MAX_SCHEDULE_TITLE_LEN`].
    /// 2. `description` length ≤ [`MAX_SCHEDULE_DESCRIPTION_LEN`].
    /// 3. `due_date`, if present, is strictly in the future relative to the
    ///    current ledger timestamp.
    ///
    /// Does **not** check monotonicity across milestones — use
    /// [`Self::validate_schedule_monotonicity`] for that.
    fn validate_single_schedule(
        env: &Env,
        schedule: &MilestoneSchedule,
    ) -> Result<(), EscrowError> {
        // String-length guards — prevent storage exhaustion.
        if let Some(ref title) = schedule.title {
            if title.len() > MAX_SCHEDULE_TITLE_LEN {
                return Err(EscrowError::ScheduleStringTooLong);
            }
        }
        if let Some(ref desc) = schedule.description {
            if desc.len() > MAX_SCHEDULE_DESCRIPTION_LEN {
                return Err(EscrowError::ScheduleStringTooLong);
            }
        }
        // Due-date must be in the future.
        if let Some(due) = schedule.due_date {
            let now = env.ledger().timestamp();
            if due <= now {
                return Err(EscrowError::ScheduleDueDateInPast);
            }
        }
        Ok(())
    }

    /// Validates that a slice of optional schedules has strictly monotonically
    /// increasing `due_date` values.
    ///
    /// Milestones without a `due_date` are skipped in the monotonicity check,
    /// but any milestone that *does* have a date must be strictly later than
    /// the most recently seen dated milestone.
    fn validate_schedule_monotonicity(
        schedules: &[Option<MilestoneSchedule>],
    ) -> Result<(), EscrowError> {
        let mut last_due: Option<u64> = None;
        for sched_opt in schedules.iter() {
            if let Some(sched) = sched_opt {
                if let Some(due) = sched.due_date {
                    if let Some(prev) = last_due {
                        if due <= prev {
                            return Err(EscrowError::ScheduleDatesNotMonotonic);
                        }
                    }
                    last_due = Some(due);
                }
            }
        }
        Ok(())
    }

    /// Validates a complete set of schedule metadata intended for a new contract.
    ///
    /// Combines individual field validation with cross-milestone monotonicity.
    pub fn validate_schedule_metadata(
        env: &Env,
        schedules: &[Option<MilestoneSchedule>],
    ) -> Result<(), EscrowError> {
        for sched_opt in schedules.iter() {
            if let Some(sched) = sched_opt {
                Self::validate_single_schedule(env, sched)?;
            }
        }
        Self::validate_schedule_monotonicity(schedules)?;
        Ok(())
    }

    /// Persists schedule metadata for every milestone in a newly created contract.
    ///
    /// Entries for milestones without schedules (`None`) are simply not written;
    /// reads for those indices will return `None`.
    fn store_schedule_metadata(
        env: &Env,
        contract_id: u32,
        schedules: &[Option<MilestoneSchedule>],
    ) {
        let now = env.ledger().timestamp();
        for (idx, sched_opt) in schedules.iter().enumerate() {
            if let Some(sched) = sched_opt {
                let key = DataKey::MilestoneSchedule(contract_id, idx as u32);
                let stamped = MilestoneSchedule {
                    due_date: sched.due_date,
                    title: sched.title.clone(),
                    description: sched.description.clone(),
                    updated_at: now,
                };
                env.storage().persistent().set(&key, &stamped);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public contract interface
// ---------------------------------------------------------------------------

#[contractimpl]
impl Escrow {
    // -----------------------------------------------------------------------
    // Pause controls
    // -----------------------------------------------------------------------

    /// Initializes admin-managed pause controls.
    ///
    /// Must be called exactly once; subsequent calls panic with
    /// `"Pause controls already initialized"`.
    pub fn initialize(env: Env, admin: Address) -> bool {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Pause controls already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        true
    }

    /// Returns the configured pause-control administrator.
    pub fn get_admin(env: Env) -> Address {
        Self::read_admin(&env)
    }

    /// Pauses all state-changing operations.
    pub fn pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    /// Lifts a normal pause.
    ///
    /// # Panics
    /// * If emergency mode is still active.
    /// * If the contract is not currently paused.
    pub fn unpause(env: Env) -> bool {
        Self::require_admin(&env);
        if Self::is_emergency_internal(&env) {
            panic!("Emergency pause active");
        }
        if !Self::is_paused_internal(&env) {
            panic!("Contract is not paused");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        true
    }

    /// Activates emergency mode and hard-pauses the contract.
    pub fn activate_emergency_pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &true);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    /// Resolves emergency mode and restores normal operations.
    pub fn resolve_emergency(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        env.storage().instance().set(&DataKey::Paused, &false);
        true
    }

    /// Read-only pause status.
    pub fn is_paused(env: Env) -> bool {
        Self::is_paused_internal(&env)
    }

    /// Read-only emergency status.
    pub fn is_emergency(env: Env) -> bool {
        Self::is_emergency_internal(&env)
    }

    // -----------------------------------------------------------------------
    // Core escrow operations
    // -----------------------------------------------------------------------

    /// Create a new escrow contract with optional per-milestone schedule metadata.
    ///
    /// # Arguments
    /// * `client`            – Address of the client who will fund the escrow.
    /// * `freelancer`        – Address of the freelancer who receives payments.
    /// * `arbiter`           – Optional arbiter for dispute resolution.
    /// * `milestone_amounts` – Non-empty list of per-milestone payment amounts (stroops).
    /// * `release_auth`      – Authorization scheme governing milestone releases.
    /// * `schedules`         – Optional per-milestone schedule metadata.  The
    ///   vector must have the **same length** as `milestone_amounts`, with `None`
    ///   for milestones that do not carry scheduling information.  Pass an
    ///   empty vector to opt out of schedule metadata entirely.
    ///
    /// # Returns
    /// The new contract ID (`u32`).
    ///
    /// # Errors / Panics
    /// * `"Contract is paused"` – if the pause flag is set.
    /// * `"At least one milestone required"` – if `milestone_amounts` is empty.
    /// * `"Milestone amounts must be positive"` – if any amount ≤ 0.
    /// * `"Client and freelancer cannot be the same address"` – same-address guard.
    /// * [`EscrowError::ScheduleDueDateInPast`] – if a due date is not in the future.
    /// * [`EscrowError::ScheduleDatesNotMonotonic`] – if due dates are not ordered.
    /// * [`EscrowError::ScheduleStringTooLong`] – if a title/description is too long.
    ///
    /// # Security
    /// * Client authorization is required (`client.require_auth()`).
    /// * Same-participant attack is blocked.
    /// * Schedule metadata is fully validated before any storage write to
    ///   prevent partial state corruption.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
        schedules: Vec<Option<MilestoneSchedule>>,
    ) -> u32 {
        Self::ensure_not_paused(&env);
        client.require_auth();

        if milestone_amounts.is_empty() {
            panic!("At least one milestone required");
        }
        if client == freelancer {
            panic!("Client and freelancer cannot be the same address");
        }

        // Validate all milestone amounts.
        let params = Self::protocol_parameters(&env);
        let mut total_amount: i128 = 0;
        let mut milestones: Vec<Milestone> = Vec::new(&env);
        let mut idx = 0u32;
        while idx < milestone_amounts.len() {
            let amount = milestone_amounts.get(idx).unwrap();
            if amount <= 0 {
                panic!("Milestone amounts must be positive");
            }
            if amount < params.min_milestone_amount {
                panic!("Milestone amount below minimum");
            }
            total_amount += amount;
            milestones.push_back(Milestone {
                amount,
                released: false,
                approved_by: None,
                approval_timestamp: None,
            });
            idx += 1;
        }
        if milestones.len() > params.max_milestones {
            panic!("Too many milestones");
        }

        // Validate and store schedule metadata.
        // Convert soroban Vec<Option<MilestoneSchedule>> to a Rust slice for validation.
        if !schedules.is_empty() {
            if schedules.len() != milestone_amounts.len() {
                panic!("schedules length must match milestone_amounts length");
            }
            // Build a temporary Rust Vec for validation (no_std compatible via alloc is
            // unavailable in Soroban, so we iterate inline).
            let mut sched_idx = 0u32;
            while sched_idx < schedules.len() {
                if let Some(ref sched) = schedules.get(sched_idx).unwrap() {
                    Self::validate_single_schedule(&env, sched)
                        .unwrap_or_else(|_| panic!("invalid schedule metadata"));
                }
                sched_idx += 1;
            }
            // Monotonicity check across milestones.
            let mut last_due: Option<u64> = None;
            let mut mono_idx = 0u32;
            while mono_idx < schedules.len() {
                if let Some(ref sched) = schedules.get(mono_idx).unwrap() {
                    if let Some(due) = sched.due_date {
                        if let Some(prev) = last_due {
                            if due <= prev {
                                panic!("milestone due dates must be strictly increasing");
                            }
                        }
                        last_due = Some(due);
                    }
                }
                mono_idx += 1;
            }
        }

        // Assign contract ID.
        let contract_id = Self::next_contract_id(&env);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(contract_id + 1));

        let contract_data = EscrowContractData {
            client,
            freelancer,
            arbiter,
            milestones,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            released_milestones: 0,
            status: ContractStatus::Created,
            release_auth,
            reputation_issued: false,
            created_at: env.ledger().timestamp(),
        };
        Self::save_contract(&env, contract_id, &contract_data);

        // Persist schedule metadata after the contract record is safely written.
        if !schedules.is_empty() {
            let now = env.ledger().timestamp();
            let mut si = 0u32;
            while si < schedules.len() {
                if let Some(ref sched) = schedules.get(si).unwrap() {
                    let key = DataKey::MilestoneSchedule(contract_id, si);
                    let stamped = MilestoneSchedule {
                        due_date: sched.due_date,
                        title: sched.title.clone(),
                        description: sched.description.clone(),
                        updated_at: now,
                    };
                    env.storage().persistent().set(&key, &stamped);
                }
                si += 1;
            }
        }

        contract_id
    }

    /// Update schedule metadata for a single milestone on an existing contract.
    ///
    /// Only the **client** of the contract may call this function.
    /// Once a milestone is released, its schedule is **immutable**.
    ///
    /// # Arguments
    /// * `contract_id`   – ID of the target escrow contract.
    /// * `milestone_idx` – Zero-based index of the milestone to update.
    /// * `schedule`      – New schedule metadata to store.
    ///
    /// # Errors / Panics
    /// * `"Contract is paused"`.
    /// * [`EscrowError::ContractNotFound`] – unknown `contract_id`.
    /// * [`EscrowError::ScheduleInvalidMilestoneIndex`] – index out of range.
    /// * [`EscrowError::ScheduleImmutableAfterRelease`] – milestone already released.
    /// * [`EscrowError::ScheduleDueDateInPast`] – due date not in the future.
    /// * [`EscrowError::ScheduleDatesNotMonotonic`] – ordering violated.
    /// * [`EscrowError::ScheduleStringTooLong`] – string field too long.
    ///
    /// # Security
    /// * Requires client authorization.
    /// * Immutability after release prevents retroactive schedule manipulation.
    /// * Full validation runs before any storage write.
    pub fn set_milestone_schedule(
        env: Env,
        contract_id: u32,
        milestone_idx: u32,
        schedule: MilestoneSchedule,
    ) -> bool {
        Self::ensure_not_paused(&env);

        let contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();

        if milestone_idx >= contract.milestones.len() {
            panic!("milestone index out of range");
        }

        let milestone = contract.milestones.get(milestone_idx).unwrap();
        if milestone.released {
            panic!("schedule is immutable after milestone release");
        }

        // Validate the new entry in isolation.
        Self::validate_single_schedule(&env, &schedule)
            .unwrap_or_else(|_| panic!("invalid schedule metadata"));

        // Monotonicity: check against adjacent stored schedules.
        // Check predecessor (milestone_idx - 1) if it exists and has a due date.
        if milestone_idx > 0 {
            let prev_key = DataKey::MilestoneSchedule(contract_id, milestone_idx - 1);
            if let Some(prev_sched) = env
                .storage()
                .persistent()
                .get::<_, MilestoneSchedule>(&prev_key)
            {
                if let (Some(prev_due), Some(new_due)) = (prev_sched.due_date, schedule.due_date) {
                    if new_due <= prev_due {
                        panic!("milestone due dates must be strictly increasing");
                    }
                }
            }
        }
        // Check successor (milestone_idx + 1) if it exists and has a due date.
        let next_key = DataKey::MilestoneSchedule(contract_id, milestone_idx + 1);
        if let Some(next_sched) = env
            .storage()
            .persistent()
            .get::<_, MilestoneSchedule>(&next_key)
        {
            if let (Some(new_due), Some(next_due)) = (schedule.due_date, next_sched.due_date) {
                if new_due >= next_due {
                    panic!("milestone due dates must be strictly increasing");
                }
            }
        }

        let key = DataKey::MilestoneSchedule(contract_id, milestone_idx);
        let stamped = MilestoneSchedule {
            due_date: schedule.due_date,
            title: schedule.title,
            description: schedule.description,
            updated_at: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&key, &stamped);
        true
    }

    /// Retrieve the schedule metadata for a specific milestone.
    ///
    /// Returns `None` if no metadata has been stored for that milestone.
    pub fn get_milestone_schedule(
        env: Env,
        contract_id: u32,
        milestone_idx: u32,
    ) -> Option<MilestoneSchedule> {
        let key = DataKey::MilestoneSchedule(contract_id, milestone_idx);
        env.storage().persistent().get(&key)
    }

    /// Deposit funds into an escrow contract.
    ///
    /// The deposit must equal the contract's `total_amount` exactly.
    /// Only the client of the contract may call this.
    pub fn deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        if amount <= 0 {
            panic!("deposit amount must be positive");
        }

        let mut contract = Self::load_contract(&env, contract_id);

        if caller != contract.client {
            panic!("Only client can deposit funds");
        }
        if contract.status != ContractStatus::Created {
            panic!("Contract must be in Created status to deposit funds");
        }
        if contract.funded_amount + amount > contract.total_amount {
            panic!("Deposit amount must equal total milestone amounts");
        }

        contract.funded_amount += amount;
        if contract.funded_amount == contract.total_amount {
            contract.status = ContractStatus::Funded;
        }
        Self::save_contract(&env, contract_id, &contract);
        true
    }

    /// Approve a specific milestone for release.
    ///
    /// Authorization rules are governed by the contract's [`ReleaseAuthorization`].
    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);

        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to approve milestones");
        }
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic!("Milestone already released");
        }

        let is_authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => caller == contract.client,
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::ClientAndArbiter | ReleaseAuthorization::MultiSig => {
                caller == contract.client
                    || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
        };
        if !is_authorized {
            panic!("Caller not authorized to approve milestone release");
        }
        if milestone
            .approved_by
            .clone()
            .map_or(false, |addr| addr == caller)
        {
            panic!("Milestone already approved by this address");
        }

        let mut updated = milestone;
        updated.approved_by = Some(caller);
        updated.approval_timestamp = Some(env.ledger().timestamp());
        contract.milestones.set(milestone_id, updated);
        Self::save_contract(&env, contract_id, &contract);
        true
    }

    /// Release a milestone payment to the freelancer after proper authorization.
    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);

        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to release milestones");
        }
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic!("Milestone already released");
        }

        let has_approval = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => milestone
                .approved_by
                .clone()
                .map_or(false, |addr| addr == contract.client),
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |arbiter| {
                    milestone
                        .approved_by
                        .clone()
                        .map_or(false, |addr| addr == arbiter)
                })
            }
            ReleaseAuthorization::ClientAndArbiter => milestone
                .approved_by
                .clone()
                .map_or(false, |addr| {
                    addr == contract.client
                        || contract.arbiter.clone().map_or(false, |a| addr == a)
                }),
            ReleaseAuthorization::MultiSig => milestone
                .approved_by
                .clone()
                .map_or(false, |addr| addr == contract.client),
        };
        if !has_approval {
            panic!("Insufficient approvals for milestone release");
        }

        let amount = milestone.amount;
        let mut updated = milestone;
        updated.released = true;
        contract.milestones.set(milestone_id, updated);
        contract.released_amount += amount;
        contract.released_milestones += 1;

        if Self::all_milestones_released(&contract.milestones) {
            contract.status = ContractStatus::Completed;
            Self::add_pending_reputation_credit(&env, &contract.freelancer);
        }
        Self::save_contract(&env, contract_id, &contract);
        true
    }

    /// Issue a reputation credential for the freelancer after contract completion.
    pub fn issue_reputation(env: Env, contract_id: u32, rating: i128) -> bool {
        Self::ensure_not_paused(&env);

        let params = Self::protocol_parameters(&env);
        if rating < params.min_reputation_rating || rating > params.max_reputation_rating {
            panic!("rating out of range");
        }

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Completed {
            panic!("contract not completed");
        }
        if contract.reputation_issued {
            panic!("reputation already issued");
        }

        let key = DataKey::Reputation(contract.freelancer.clone());
        let mut record: ReputationRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(ReputationRecord {
                completed_contracts: 0,
                total_rating: 0,
                last_rating: 0,
            });
        record.completed_contracts += 1;
        record.total_rating += rating;
        record.last_rating = rating;
        env.storage().persistent().set(&key, &record);

        contract.reputation_issued = true;
        Self::save_contract(&env, contract_id, &contract);
        true
    }

    // -----------------------------------------------------------------------
    // Query functions
    // -----------------------------------------------------------------------

    /// Hello-world function used in CI smoke tests.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Returns stored contract state for the given ID.
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::load_contract(&env, contract_id)
    }

    /// Returns the reputation record for a freelancer, if any.
    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    /// Returns the number of pending reputation credits for a freelancer.
    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    /// Returns active protocol parameters (defaults if governance not initialized).
    pub fn get_protocol_parameters(env: Env) -> ProtocolParameters {
        Self::protocol_parameters(&env)
    }

    /// Returns the current governance admin, if governance has been initialized.
    pub fn get_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::GovernanceAdmin)
    }

    /// Returns the pending governance admin, if a transfer is in flight.
    pub fn get_pending_governance_admin(env: Env) -> Option<Address> {
        Self::pending_governance_admin(&env)
    }

    // -----------------------------------------------------------------------
    // Governance
    // -----------------------------------------------------------------------

    /// Initialize protocol governance with an admin and starting parameters.
    pub fn initialize_protocol_governance(
        env: Env,
        admin: Address,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        admin.require_auth();
        let params = Self::validated_protocol_parameters(
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );
        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &params);
        true
    }

    /// Update protocol parameters (governance admin only).
    pub fn update_protocol_parameters(
        env: Env,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        let admin = Self::governance_admin(&env);
        admin.require_auth();
        let params = Self::validated_protocol_parameters(
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &params);
        true
    }

    /// Propose a new governance admin (two-step transfer, step 1).
    pub fn propose_governance_admin(env: Env, new_admin: Address) -> bool {
        let admin = Self::governance_admin(&env);
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &new_admin);
        true
    }

    /// Accept a pending governance admin transfer (two-step transfer, step 2).
    pub fn accept_governance_admin(env: Env) -> bool {
        let pending = Self::pending_governance_admin(&env)
            .unwrap_or_else(|| panic!("no pending governance admin"));
        pending.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &pending);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingGovernanceAdmin);
        true
    }
}

#[cfg(test)]
mod test;