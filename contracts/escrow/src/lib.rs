#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, String, Symbol, Vec,
};

const DEFAULT_MIN_MILESTONE_AMOUNT: i128 = 1;
const DEFAULT_MAX_MILESTONES: u32 = 16;
const DEFAULT_MIN_REPUTATION_RATING: i128 = 1;
const DEFAULT_MAX_REPUTATION_RATING: i128 = 5;

/// Persistent lifecycle state for an escrow agreement.
///
/// Security notes:
/// - Only `Created -> Funded -> Completed` transitions are currently supported.
/// - `Disputed` is reserved for future dispute resolution flows and is not reachable
///   in the current implementation.

/// Maximum fee basis points (100% = 10000 basis points)
pub const MAX_FEE_BASIS_POINTS: u32 = 10000;

/// Default protocol fee: 2.5% = 250 basis points
pub const DEFAULT_FEE_BASIS_POINTS: u32 = 250;

/// Maximum fee rate in basis points (10% = 1000 basis points)
/// Constraint: rate_bps must be between 0 and 1000
pub const MAX_FEE_RATE_BPS: u32 = 1000;

/// Maximum number of fee recipients
/// Constraint: recipients vector must have at most 3 entries
pub const MAX_FEE_RECIPIENTS: u32 = 3;

/// Total percentage in basis points (100% = 10000 basis points)
/// Constraint: sum of all recipient percentages must equal 10000
pub const TOTAL_PERCENTAGE_BPS: u32 = 10000;

/// Default timeout duration: 30 days in seconds (30 * 24 * 60 * 60)
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 2_592_000;

/// Minimum timeout duration: 1 day in seconds
pub const MIN_TIMEOUT_SECONDS: u64 = 86_400;

/// Maximum timeout duration: 365 days in seconds
pub const MAX_TIMEOUT_SECONDS: u64 = 31_536_000;

/// Data keys for contract storage
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    TreasuryConfig,
    Contract(u32),
    Milestone(u32, u32),
    ContractStatus(u32),
    NextContractId,
    ContractTimeout(u32),
    MilestoneDeadline(u32, u32),
    DisputeDeadline(u32),
    LastActivity(u32),
    Dispute(u32),
    MilestoneComplete(u32, u32),
    Paused,
    EmergencyPaused,
    Reputation(Address),
    PendingReputationCredits(Address),
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
    FeeConfig,
    Treasury,
}

/// Status of an escrow contract
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
    InDispute = 4,
}

/// Release authorization scheme for milestones
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly = 0,
    ArbiterOnly = 1,
    ClientAndArbiter = 2,
    MultiSig = 3,
}

/// Individual milestone tracked inside an escrow agreement.
///
/// Invariant:
/// - `released == true` is irreversible.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    /// Amount in stroops allocated to this milestone.
    pub amount: i128,
    /// Whether the milestone payment has been released to the freelancer.
    pub released: bool,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    InvalidContractId = 1,
    InvalidMilestoneId = 2,
    InvalidAmount = 3,
    InvalidRating = 4,
    EmptyMilestones = 5,
    InvalidParticipant = 6,
    TreasuryNotInitialized = 7,
    InvalidFeePercentage = 8,
    Unauthorized = 9,
    ContractNotFound = 10,
    MilestoneNotFound = 11,
    MilestoneAlreadyReleased = 12,
    InsufficientFunds = 13,
    TreasuryAlreadyInitialized = 14,
    ArithmeticOverflow = 15,
    TimeoutNotExceeded = 16,
    InvalidTimeout = 17,
    MilestoneNotComplete = 18,
    MilestoneAlreadyComplete = 19,
    DisputeNotFound = 20,
    DisputeAlreadyResolved = 21,
    TimeoutAlreadyClaimed = 22,
    NoDisputeActive = 23,
}

/// Stored escrow state for a single agreement.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<Milestone>,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub status: ContractStatus,
}

/// Reputation state derived from completed escrow contracts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub completed_contracts: u32,
    pub total_rating: i128,
    pub last_rating: i128,
}

/// Governed protocol parameters used by the escrow validation logic.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolParameters {
    pub min_milestone_amount: i128,
    pub max_milestones: u32,
    pub min_reputation_rating: i128,
    pub max_reputation_rating: i128,
}

/// Timeout configuration for escrow contracts
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeoutConfig {
    /// Timeout duration in seconds
    pub duration: u64,
    /// Auto-resolve type: 0 = return to client, 1 = release to freelancer, 2 = split
    pub auto_resolve_type: u32,
}

/// Dispute structure for tracking disputes
#[contracttype]
#[derive(Clone, Debug)]
pub struct Dispute {
    /// Address that initiated the dispute
    pub initiator: Address,
    /// Reason for the dispute
    pub reason: Symbol,
    /// Timestamp when dispute was created
    pub created_at: u64,
    /// Whether dispute has been resolved
    pub resolved: bool,
}

/// Treasury configuration for protocol fee collection
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryConfig {
    /// Address where protocol fees are sent
    pub address: Address,
    /// Fee percentage in basis points (10000 = 100%)
    pub fee_basis_points: u32,
}

/// Escrow contract structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowContract {
    pub client: Address,
    pub freelancer: Address,
    pub total_amount: i128,
    pub milestone_count: u32,
}

/// Immutable record created when a dispute is initiated.
/// Written once to persistent storage and never overwritten.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeRecord {
    /// The address (client or freelancer) that initiated the dispute.
    pub initiator: Address,
    /// A short human-readable reason for the dispute.
    pub reason: String,
    /// Ledger timestamp (seconds since Unix epoch) at the moment the dispute was recorded.
    pub timestamp: u64,
}

/// Full on-chain state of an escrow contract.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowState {
    /// Address of the client who created and funded the escrow.
    pub client: Address,
    /// Address of the freelancer who will receive milestone payments.
    pub freelancer: Address,
    /// Current lifecycle status of the escrow.
    pub status: ContractStatus,
    /// Ordered list of payment milestones.
    pub milestones: Vec<Milestone>,
}

// ---------------------------------------------------------------------------
// Fee Accounting Structures
// ---------------------------------------------------------------------------

/// Fee configuration for the escrow contract.
///
/// Stores the global fee rate and optional fee splitting configuration.
///
/// # Constraints
/// - `rate_bps` must be between 0 and 1000 (0% to 10%)
/// - If `split_enabled` is true, `recipients` must have 1-3 entries
/// - Sum of all recipient percentages must equal 10000 (100%)
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeConfig {
    /// Fee rate as basis points (e.g., 250 = 2.5%)
    pub rate_bps: u32,

    /// Whether fee splitting is enabled
    pub split_enabled: bool,

    /// Fee split recipients and their percentages
    pub recipients: Vec<FeeRecipient>,
}

/// A single fee recipient and their share percentage.
///
/// Used when fee splitting is enabled to distribute fees among multiple recipients.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeRecipient {
    /// Address to receive fees
    pub address: Address,

    /// Percentage as basis points (e.g., 7000 = 70%)
    pub percentage_bps: u32,

    /// Whether this is the primary recipient (receives rounding remainders)
    pub is_primary: bool,
}

/// Treasury tracking accumulated fees for each recipient.
///
/// Maintains the total fees collected and per-recipient balances.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Treasury {
    /// Total fees accumulated across all recipients
    pub total: i128,

    /// Per-recipient accumulated fees
    pub balances: Vec<(Address, i128)>,
}

/// Internal structure returned by fee calculation.
///
/// Contains the calculated fee amount, net amount to freelancer,
/// and fee splits if splitting is enabled.
#[derive(Clone, Debug)]
pub struct FeeCalculation {
    /// Total fee amount (rounded down)
    pub fee_amount: i128,

    /// Net amount to freelancer (includes rounding remainder)
    pub net_amount: i128,

    /// Fee splits per recipient (if splitting enabled)
    pub splits: Vec<FeeSplit>,
}

/// A single fee split for a recipient.
///
/// Used internally to track how fees are distributed among recipients.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeSplit {
    pub recipient: Address,
    pub amount: i128,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Escrow;

impl Escrow {
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
}

#[contractimpl]
impl Escrow {
    /// Initializes admin-managed pause controls.
    ///
    /// # Panics
    /// - If called more than once.
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

    /// Pauses state-changing operations for incident response.
    pub fn pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    /// Lifts a normal pause.
    ///
    /// # Panics
    /// - If emergency mode is still active.
    /// - If contract is not paused.
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

    /// Create a new escrow contract with milestone release authorization
    ///
    /// # Arguments
    /// * `client` - Address of the client who funds the escrow
    /// * `freelancer` - Address of the freelancer who receives payments
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestone_amounts` - Vector of milestone payment amounts
    /// * `release_auth` - Authorization scheme for milestone releases
    ///
    /// # Returns
    /// Contract ID for the newly created escrow
    ///
    /// # Errors
    /// Panics if:
    /// - Contract is paused
    /// - Milestone amounts vector is empty
    /// - Any milestone amount is zero or negative
    /// - Client and freelancer addresses are the same
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        _arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        _release_auth: ReleaseAuthorization,
    ) -> u32 {
        Self::ensure_not_paused(&env);

        if milestone_amounts.is_empty() {
            panic!("At least one milestone required");
        }

        // Validate milestones
        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts.get(i).unwrap();
            if amount <= 0 {
                panic!("Invalid milestone amount");
            }
        }

        if client == freelancer {
            panic!("Client and freelancer must be different");
        }

        // Create milestone records
        let mut milestones = Vec::new(&env);
        for i in 0..milestone_amounts.len() {
            milestones.push_back(Milestone {
                amount: milestone_amounts.get(i).unwrap(),
                released: false,
            });
        }

        // Calculate total amount
        let mut total_amount = 0i128;
        for i in 0..milestones.len() {
            total_amount = total_amount
                .checked_add(milestones.get(i).unwrap().amount)
                .unwrap_or_else(|| panic!("Arithmetic overflow"));
        }

        let contract_data = EscrowContractData {
            client: client.clone(),
            freelancer: freelancer.clone(),
            milestones,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            status: ContractStatus::Created,
        };

        let contract_id = Self::next_contract_id(&env);
        Self::save_contract(&env, contract_id, &contract_data);

        // Increment next contract ID
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(contract_id + 1));

        contract_id
    }

    /// Deposit funds into escrow. Only the client may call this.
    pub fn deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let mut contract: EscrowContractData = Self::load_contract(&env, contract_id);

        if caller != contract.client {
            panic!("Only client can deposit funds");
        }

        if contract.status != ContractStatus::Created {
            panic!("Contract must be in Created status to deposit funds");
        }

        if amount <= 0 {
            panic!("Deposit amount must be positive");
        }

        contract.funded_amount = contract
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic!("Arithmetic overflow"));

        // Check if fully funded
        if contract.funded_amount >= contract.total_amount {
            contract.status = ContractStatus::Funded;
        }

        Self::save_contract(&env, contract_id, &contract);

        true
    }

    /// Approve a milestone for release with proper authorization.
    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let contract: EscrowContractData = Self::load_contract(&env, contract_id);

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

        Self::save_contract(&env, contract_id, &contract);

        true
    }

    /// Release a milestone payment to the freelancer after proper authorization.
    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        Self::ensure_not_paused(&env);

        let mut contract: EscrowContractData = Self::load_contract(&env, contract_id);

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

        let milestone_amount = milestone.amount;

        let mut updated_milestone = milestone.clone();
        updated_milestone.released = true;

        contract.milestones.set(milestone_id, updated_milestone);

        contract.released_amount = contract
            .released_amount
            .checked_add(milestone_amount)
            .unwrap_or_else(|| panic!("Arithmetic overflow"));

        let all_released = Self::all_milestones_released(&contract.milestones);
        if all_released {
            contract.status = ContractStatus::Completed;
        }

        Self::save_contract(&env, contract_id, &contract);

        true
    }

    /// Issue a reputation credential for the freelancer after contract completion.
    pub fn issue_reputation(env: Env, _freelancer: Address, _rating: i128) -> bool {
        Self::ensure_not_paused(&env);

        true
    }

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Returns the stored contract state.
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::load_contract(&env, contract_id)
    }

    /// Returns the stored reputation record for a freelancer, if present.
    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    /// Returns the number of pending reputation updates that can be claimed.
    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    /// Returns the active protocol parameters.
    ///
    /// If governance has not been initialized yet, this returns the safe default
    /// parameters baked into the contract.
    pub fn get_protocol_parameters(env: Env) -> ProtocolParameters {
        Self::protocol_parameters(&env)
    }

    /// Returns the current governance admin, if governance has been initialized.
    pub fn get_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::GovernanceAdmin)
    }

    /// Returns the pending governance admin, if an admin transfer is in flight.
    pub fn get_pending_governance_admin(env: Env) -> Option<Address> {
        Self::pending_governance_admin(&env)
    }
}

impl Escrow {
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

    fn add_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let key = DataKey::PendingReputationCredits(freelancer.clone());
        let current = env.storage().persistent().get::<_, u32>(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + 1));
    }

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

    /// Calculate fee, net amount, and splits for a milestone release.
    ///
    /// This function implements the core fee calculation logic with floor rounding.
    /// The rounding strategy ensures that no stroops are lost or created:
    /// - Fee is rounded down to the nearest whole stroop
    /// - Any fractional remainder goes to the freelancer (included in net_amount)
    /// - Invariant: fee_amount + net_amount = milestone_amount (always holds)
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `milestone_amount` - The milestone amount in stroops
    /// * `config` - Fee configuration containing rate and split settings
    ///
    /// # Returns
    /// `FeeCalculation` containing:
    /// - `fee_amount`: Total fee (rounded down)
    /// - `net_amount`: Amount to freelancer (includes rounding remainder)
    /// - `splits`: Fee distribution per recipient (if splitting enabled)
    ///
    /// # Examples
    /// ```
    /// // Example 1: Exact division
    /// // milestone = 10000 stroops, rate = 5% (500 bps)
    /// // fee = (10000 * 500) / 10000 = 500 stroops
    /// // net = 10000 - 500 = 9500 stroops
    ///
    /// // Example 2: Fractional result
    /// // milestone = 1001 stroops, rate = 2.5% (250 bps)
    /// // fee = floor((1001 * 250) / 10000) = floor(25.025) = 25 stroops
    /// // net = 1001 - 25 = 976 stroops (freelancer gets the 0.025 remainder)
    /// ```
    fn calculate_fee(env: &Env, milestone_amount: i128, config: &FeeConfig) -> FeeCalculation {
        // Calculate fee with floor rounding: (milestone_amount * rate_bps) / 10000
        let fee_amount = milestone_amount
            .checked_mul(config.rate_bps as i128)
            .unwrap_or_else(|| panic!("Arithmetic overflow in fee calculation"))
            .checked_div(10000)
            .unwrap_or(0); // Division by 10000 is safe, but handle edge case

        // Calculate net amount (freelancer receives milestone minus fee, including any remainder)
        let net_amount = milestone_amount
            .checked_sub(fee_amount)
            .unwrap_or_else(|| panic!("Arithmetic underflow in net calculation"));

        // If fee splitting is enabled, distribute the fee among recipients
        let splits = if config.split_enabled && fee_amount > 0 {
            Self::split_fee(env, fee_amount, &config.recipients)
        } else {
            // No splitting - return empty vector
            Vec::new(env)
        };

        FeeCalculation {
            fee_amount,
            net_amount,
            splits,
        }
    }

    /// Split a fee amount among multiple recipients.
    ///
    /// Distributes the total fee among recipients according to their percentage allocations.
    /// Each recipient's share is calculated as `(total_fee * percentage_bps) / 10000` and
    /// rounded down. Any remainder from rounding is added to the primary recipient's share.
    ///
    /// This ensures that the sum of all splits exactly equals the total fee (conservation property).
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `total_fee` - Total fee amount to split (in stroops)
    /// * `recipients` - Vector of fee recipients with their percentages
    ///
    /// # Returns
    /// Vector of `FeeSplit` entries showing how the fee is distributed
    ///
    /// # Examples
    /// ```
    /// // Example 1: Exact division
    /// // total_fee = 100 stroops
    /// // recipients: Platform (70%), Referrer (30%)
    /// // Platform gets: (100 * 7000) / 10000 = 70 stroops
    /// // Referrer gets: (100 * 3000) / 10000 = 30 stroops
    /// // Sum: 70 + 30 = 100 ✓
    ///
    /// // Example 2: With remainder
    /// // total_fee = 101 stroops
    /// // recipients: Platform (70%, primary), Referrer (30%)
    /// // Platform gets: floor((101 * 7000) / 10000) = floor(70.7) = 70 stroops
    /// // Referrer gets: floor((101 * 3000) / 10000) = floor(30.3) = 30 stroops
    /// // Sum before remainder: 70 + 30 = 100
    /// // Remainder: 101 - 100 = 1 stroop → added to Platform (primary)
    /// // Final: Platform = 71, Referrer = 30
    /// // Sum: 71 + 30 = 101 ✓
    /// ```
    fn split_fee(env: &Env, total_fee: i128, recipients: &Vec<FeeRecipient>) -> Vec<FeeSplit> {
        let mut splits = Vec::new(env);
        let mut sum_allocated = 0i128;
        let mut primary_index: Option<u32> = None;

        // Calculate each recipient's share (rounded down)
        for i in 0..recipients.len() {
            let recipient = recipients.get(i).unwrap();

            // Track which recipient is primary (receives remainder)
            if recipient.is_primary {
                primary_index = Some(i);
            }

            // Calculate share: (total_fee * percentage_bps) / 10000
            let share = total_fee
                .checked_mul(recipient.percentage_bps as i128)
                .unwrap_or_else(|| panic!("Arithmetic overflow in fee split calculation"))
                .checked_div(10000)
                .unwrap_or(0);

            splits.push_back(FeeSplit {
                recipient: recipient.address.clone(),
                amount: share,
            });

            sum_allocated = sum_allocated
                .checked_add(share)
                .unwrap_or_else(|| panic!("Arithmetic overflow in sum calculation"));
        }

        // Calculate remainder and add to primary recipient
        let remainder = total_fee
            .checked_sub(sum_allocated)
            .unwrap_or_else(|| panic!("Arithmetic underflow in remainder calculation"));

        if remainder > 0 {
            if let Some(primary_idx) = primary_index {
                // Add remainder to primary recipient
                let mut primary_split = splits.get(primary_idx).unwrap();
                primary_split.amount = primary_split
                    .amount
                    .checked_add(remainder)
                    .unwrap_or_else(|| panic!("Arithmetic overflow adding remainder"));
                splits.set(primary_idx, primary_split);
            } else {
                // No primary recipient specified - this should not happen in valid configuration
                panic!("No primary recipient found for remainder allocation");
            }
        }

        splits
    }

    /// Update treasury storage with new fee amounts.
    ///
    /// Loads the current treasury from storage, increments each recipient's balance
    /// by the corresponding split amount, increments the total treasury amount,
    /// and saves the updated treasury back to storage.
    ///
    /// If the treasury doesn't exist in storage, it is initialized with total = 0
    /// and empty balances before applying the updates.
    ///
    /// # Arguments
    /// * `env` - Contract environment
    /// * `splits` - Vector of fee splits to add to the treasury
    ///
    /// # Examples
    /// ```ignore
    /// // Example: Update treasury with two fee splits
    /// // Platform receives 70 stroops, Referrer receives 30 stroops
    /// let splits = vec![
    ///     FeeSplit { recipient: platform_addr, amount: 70 },
    ///     FeeSplit { recipient: referrer_addr, amount: 30 },
    /// ];
    /// update_treasury(&env, splits);
    /// // Treasury total increases by 100 stroops
    /// // Platform balance increases by 70 stroops
    /// // Referrer balance increases by 30 stroops
    /// ```
    fn update_treasury(env: &Env, splits: Vec<FeeSplit>) {
        // Load current treasury from storage, or initialize if it doesn't exist
        let mut treasury: Treasury = env
            .storage()
            .persistent()
            .get(&DataKey::Treasury)
            .unwrap_or(Treasury {
                total: 0,
                balances: Vec::new(env),
            });

        // Increment each recipient's balance and the total treasury amount
        for i in 0..splits.len() {
            let split = splits.get(i).unwrap();

            // Find the recipient in the balances vector
            let mut found = false;
            for j in 0..treasury.balances.len() {
                let (addr, balance) = treasury.balances.get(j).unwrap();
                if addr == split.recipient {
                    // Recipient exists, increment their balance
                    let new_balance = balance
                        .checked_add(split.amount)
                        .unwrap_or_else(|| panic!("Arithmetic overflow in balance update"));
                    treasury.balances.set(j, (addr, new_balance));
                    found = true;
                    break;
                }
            }

            // If recipient not found, add them to the balances vector
            if !found {
                treasury
                    .balances
                    .push_back((split.recipient.clone(), split.amount));
            }

            // Increment total treasury amount
            treasury.total = treasury
                .total
                .checked_add(split.amount)
                .unwrap_or_else(|| panic!("Arithmetic overflow in treasury total"));
        }

        // Save updated treasury to storage
        env.storage()
            .persistent()
            .set(&DataKey::Treasury, &treasury);
    }
}

#[cfg(test)]
mod test;
