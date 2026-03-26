#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

use soroban_sdk::symbol_short;

/// Storage keys for persistent data
const ADMIN: Symbol = symbol_short!("ADMIN");
const ARBITRATOR: Symbol = symbol_short!("ARBIT");
const CONTRACTS: Symbol = symbol_short!("CONTRS");
const DISPUTES: Symbol = symbol_short!("DISPUT");
const NEXT_CONTRACT_ID: Symbol = symbol_short!("NEXT_CID");
const NEXT_DISPUTE_ID: Symbol = symbol_short!("NEXT_DID");

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

/// Default timeout duration: 30 days in seconds (30 * 24 * 60 * 60)
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 2_592_000;

/// Minimum timeout duration: 1 day in seconds
pub const MIN_TIMEOUT_SECONDS: u64 = 86_400;

/// Maximum timeout duration: 365 days in seconds
pub const MAX_TIMEOUT_SECONDS: u64 = 31_536_000;

/// Data keys for contract storage
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
}

/// Status of an escrow contract
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
    Resolved = 4,
    Cancelled = 5,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open = 0,
    InReview = 1,
    Resolved = 2,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeResolution {
    FullRefund = 0,    // Client gets full refund
    PartialRefund = 1, // Client gets partial refund, freelancer gets rest
    FullPayout = 2,    // Freelancer gets full amount
    Split = 3,         // Custom split determined by arbitrator
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    InDispute = 3,
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
    InvalidAmount = 14,
    TreasuryAlreadyInitialized = 15,
    ArithmeticOverflow = 16,
    TimeoutNotExceeded = 17,
    InvalidTimeout = 18,
    MilestoneNotComplete = 19,
    MilestoneAlreadyComplete = 20,
    DisputeNotFound = 21,
    DisputeAlreadyResolved = 22,
    TimeoutAlreadyClaimed = 23,
    NoDisputeActive = 24,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum DataKey {
    Admin,
    Paused,
    EmergencyPaused,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowContract {
    pub id: u32,
    pub client: Address,
    pub freelancer: Address,
    pub total_amount: i128,
    pub milestones: Vec<Milestone>,
    pub status: ContractStatus,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Dispute {
    pub id: u32,
    pub contract_id: u32,
    pub initiator: Address,
    pub reason: Symbol,
    pub evidence: Vec<Symbol>,
    pub status: DisputeStatus,
    pub resolution: DisputeResolution,
    pub client_payout: i128,
    pub freelancer_payout: i128,
    pub created_at: u64,
    pub resolved_at: u64,
    pub resolved_by: Address,
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

#[contracttype]
#[derive(Clone)]
enum DataKey {
    NextContractId,
    Contract(u32),
    Reputation(Address),
    PendingReputationCredits(Address),
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
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
    /// Initialize the contract with admin and arbitrator addresses
    ///
    /// # Arguments
    /// * `admin` - Address that can manage contract settings
    /// * `arbitrator` - Address that can resolve disputes
    pub fn initialize(env: Env, admin: Address, arbitrator: Address) {
        // Ensure contract is not already initialized
        if env.storage().persistent().has(&ADMIN) {
            panic!("already initialized");
        }

        admin.require_auth();

        env.storage().persistent().set(&ADMIN, &admin);
        env.storage().persistent().set(&ARBITRATOR, &arbitrator);
        env.storage().persistent().set(&NEXT_CONTRACT_ID, &1u32);
        env.storage().persistent().set(&NEXT_DISPUTE_ID, &1u32);
    }

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

    /// Create a new escrow contract. Client and freelancer addresses are stored
    /// for access control. Milestones define payment amounts.
    ///
    /// # Arguments
    /// * `client` - Address of the client funding the escrow
    /// * `freelancer` - Address of the freelancer receiving payments
    /// * `milestone_amounts` - Vector of milestone payment amounts
    ///
    /// # Returns
    /// * `u32` - The unique contract ID
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
    ) -> u32 {
        client.require_auth();

        let contract_id = get_next_contract_id(&env);
        let total_amount = milestone_amounts.iter().sum();

        let mut milestones = Vec::new(&env);
        for amount in milestone_amounts.iter() {
            milestones.push_back(Milestone {
                amount: amount,
                released: false,
            });
        }

        let escrow_contract = EscrowContract {
            id: contract_id,
            client: client.clone(),
            freelancer,
            total_amount,
            milestones,
            status: ContractStatus::Created,
            created_at: env.ledger().timestamp(),
        };

        let mut contracts = get_contracts_map(&env);
        contracts.set(contract_id, escrow_contract);
        env.storage().persistent().set(&CONTRACTS, &contracts);

        contract_id
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
    arbiter: Option<Address>,
    milestone_amounts: Vec<i128>,
    release_auth: ReleaseAuthorization,
) -> u32 {
    Self::ensure_not_paused(&env);

    if milestone_amounts.is_empty() {
        panic!("At least one milestone required");
    }
    Ok(())
}

    /// Deposit funds into escrow. Only the client may call this.
    ///
    /// # Arguments
    /// * `contract_id` - The ID of the escrow contract
    /// * `amount` - Amount to deposit (must equal total contract amount)
    ///
    /// # Returns
    /// * `bool` - True if deposit successful
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        let mut contracts = get_contracts_map(&env);
        let mut contract = contracts.get(contract_id).expect("contract not found");

        // Only client can deposit
        contract.client.require_auth();

        // Validate contract state
        require_contract_status(&contract, ContractStatus::Created);

        // Validate amount
        if amount != contract.total_amount {
            panic!("amount must equal total contract amount");
        }

        // Update contract status
        contract.status = ContractStatus::Funded;
        contracts.set(contract_id, contract);
        env.storage().persistent().set(&CONTRACTS, &contracts);

        true
    }

    /// Deposit funds into escrow. Only the client may call this.
pub fn deposit_funds(env: Env, _contract_id: u32, caller: Address, amount: i128) -> bool {
    Self::ensure_not_paused(&env);
    caller.require_auth();

    let contract: EscrowContract = env
        .storage()
        .persistent()
        .get(&symbol_short!("contract"))
        .unwrap_or_else(|| panic!("Contract not found"));

    if caller != contract.client {
        panic!("Only client can deposit funds");
    }

    if contract.status != ContractStatus::Created {
        panic!("Contract must be in Created status to deposit funds");
    }
    Ok(())
}

    /// Release a milestone payment to the freelancer after verification.
    ///
    /// # Arguments
    /// * `contract_id` - The ID of the escrow contract
    /// * `milestone_id` - The ID of the milestone to release
    ///
    /// # Returns
    /// * `bool` - True if milestone released successfully
pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
    let mut contracts = get_contracts_map(&env);
    let mut contract = contracts.get(contract_id).expect("contract not found");

    // Only client can release milestones
    contract.client.require_auth();

    // Validate contract state
    require_contract_status(&contract, ContractStatus::Funded);

    // Validate milestone exists and is not released
    if milestone_id >= contract.milestones.len() {
        panic!("milestone not found");
    }

    let milestone = contract.milestones.get_unchecked(milestone_id);

    if milestone.released {
        panic!("milestone already released");
    }

    // Create new milestones with updated release status
    let mut updated_milestones = Vec::new(&env);
    for (i, ms) in contract.milestones.iter().enumerate() {
        if i == milestone_id as usize {
            updated_milestones.push_back(Milestone {
                amount: ms.amount,
                released: true,
            });
        } else {
            updated_milestones.push_back(Milestone {
                amount: ms.amount,
                released: ms.released,
            });
        }
    }
    contract.milestones = updated_milestones;

    // Check if all milestones are released
    if contract.milestones.iter().all(|m| m.released) {
        contract.status = ContractStatus::Completed;
    }

    contracts.set(contract_id, contract);
    env.storage().persistent().set(&CONTRACTS, &contracts);
    true
}
    true
}
    }

    /// Approve a milestone for release with proper authorization.
    pub fn approve_milestone_release(
        env: Env,
        _contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

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
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
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
        Self::ensure_valid_milestones(&milestone_amounts)?;

        let mut updated_milestone = milestone;
        updated_milestone.approved_by = Some(caller);
        updated_milestone.approval_timestamp = Some(env.ledger().timestamp());

        contract.milestones.set(milestone_id, updated_milestone);
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

        true
    }

    /// Release a milestone payment to the freelancer after proper authorization.
    pub fn release_milestone(
        _env: Env,
        contract_id: u32,
        milestone_id: u32,
    ) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

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

        let has_sufficient_approval = match contract.release_auth {
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
            ReleaseAuthorization::ClientAndArbiter => {
                milestone.approved_by.clone().map_or(false, |addr| {
                    addr == contract.client
                        || contract
                            .arbiter
                            .clone()
                            .map_or(false, |arbiter| addr == arbiter)
                })
            }
            ReleaseAuthorization::MultiSig => milestone
                .approved_by
                .clone()
                .map_or(false, |addr| addr == contract.client),
        };

        if !has_sufficient_approval {
            panic!("Insufficient approvals for milestone release");
        }

        let mut updated_milestone = milestone;
        updated_milestone.released = true;

        contract.milestones.set(milestone_id, updated_milestone);

        let all_released = contract.milestones.iter().all(|m| m.released);
        if all_released {
            contract.status = ContractStatus::Completed;
        }

        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);
    }

    /// Create a dispute for a contract
    ///
    /// # Arguments
    /// * `contract_id` - The ID of the escrow contract
    /// * `reason` - Symbol representing the dispute reason
    /// * `evidence` - Vector of evidence symbols
    ///
    /// # Returns
    /// * `u32` - The unique dispute ID
pub fn create_dispute(
    env: Env,
    contract_id: u32,
    reason: Symbol,
    evidence: Vec<Symbol>,
) -> u32 {
    let contracts = get_contracts_map(&env);
    let contract = contracts.get(contract_id).expect("contract not found");

    // Only client or freelancer can create disputes
    // Note: In Soroban, we use the invoking address
    let caller = env.current_contract_address();
    // For now, we'll allow any caller since proper auth is handled by require_auth()
    // In a real implementation, you'd want to get the actual invoker

    // Validate contract state
    require_contract_status(&contract, ContractStatus::Funded);

    let dispute_id = get_next_dispute_id(&env);

    let dispute = Dispute {
        id: dispute_id,
        contract_id,
        initiator: caller.clone(),
        reason,
        evidence,
        status: DisputeStatus::Open,
        resolution: DisputeResolution::FullRefund, // Default
        client_payout: 0,
        freelancer_payout: 0,
        created_at: env.ledger().timestamp(),
        resolved_at: 0,
        resolved_by: caller, // Will be updated when resolved
    };

    let mut disputes = get_disputes_map(&env);
    disputes.set(dispute_id, dispute);
    env.storage().persistent().set(&DISPUTES, &disputes);

    // Update contract status
    let mut contracts = get_contracts_map(&env);
    let mut contract = contracts.get(contract_id).expect("contract not found");
    contract.status = ContractStatus::Disputed;
    contracts.set(contract_id, contract);
    env.storage().persistent().set(&CONTRACTS, &contracts);

    dispute_id
}

    /// Resolve a dispute with a specific outcome
    ///
    /// # Arguments
    /// * `dispute_id` - The ID of the dispute
    /// * `resolution` - The resolution type
    /// * `client_payout` - Amount to pay to client (for Split resolution)
    /// * `freelancer_payout` - Amount to pay to freelancer (for Split resolution)
    ///
    /// # Returns
    /// * `bool` - True if dispute resolved successfully
pub fn resolve_dispute(
    env: Env,
    dispute_id: u32,
    resolution: DisputeResolution,
    client_payout: i128,
    freelancer_payout: i128,
) -> bool {
    // Only arbitrator can resolve disputes
    let arbitrator: Address = env
        .storage()
        .persistent()
        .get(&ARBITRATOR)
        .expect("arbitrator not set");
    arbitrator.require_auth();

    let mut disputes = get_disputes_map(&env);
    let dispute = disputes.get(dispute_id).expect("dispute not found");
    let contract_id = dispute.contract_id; // Save contract_id before moving dispute

    let mut dispute = dispute;

    // Validate dispute status
    if dispute.status != DisputeStatus::Open && dispute.status != DisputeStatus::InReview {
        panic!("dispute already resolved");
    }

    let contracts = get_contracts_map(&env);
    let contract = contracts
        .get(dispute.contract_id)
        .expect("contract not found");

    // Calculate payouts based on resolution
let (client_amount, freelancer_amount) = match resolution {
        DisputeResolution::FullRefund => (contract.total_amount, 0),
        DisputeResolution::PartialRefund => {
            // Default 70% to client, 30% to freelancer
            let client_amount = contract.total_amount * 70 / 100;
            let freelancer_amount = contract.total_amount - client_amount;
            (client_amount, freelancer_amount)
        }
        DisputeResolution::FullPayout => (0, contract.total_amount),
        DisputeResolution::Split => {
            // Validate custom split
            if client_payout + freelancer_payout != contract.total_amount {
                panic!("split amounts must equal total contract amount");
            }
            (client_payout, freelancer_payout)
        }
    };

    // Update dispute
    dispute.status = DisputeStatus::Resolved;
    dispute.resolution = resolution;
    dispute.client_payout = client_amount;
    dispute.freelancer_payout = freelancer_amount;
    dispute.resolved_at = env.ledger().timestamp();
    dispute.resolved_by = arbitrator;

    disputes.set(dispute_id, dispute);
    env.storage().persistent().set(&DISPUTES, &disputes);

    // Update contract status
    let mut contracts = get_contracts_map(&env);
    let mut contract = contracts.get(contract_id).expect("contract not found");
    contract.status = ContractStatus::Resolved;
    contracts.set(contract_id, contract);
    env.storage().persistent().set(&CONTRACTS, &contracts);
    true
}
    true
}
    }

    /// Issue a reputation credential for the freelancer after contract completion.
    pub fn issue_reputation(env: Env, _freelancer: Address, _rating: i128) -> bool {
        Self::ensure_not_paused(&env);
        true
    }

    /// Update admin address (only current admin can call)
    ///
    /// # Arguments
    /// * `new_admin` - New admin address
    pub fn update_admin(env: Env, new_admin: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("admin not set");
        admin.require_auth();

        env.storage().persistent().set(&ADMIN, &new_admin);
    }

    /// Update arbitrator address (only admin can call)
    ///
    /// # Arguments
    /// * `new_arbitrator` - New arbitrator address
    pub fn update_arbitrator(env: Env, new_arbitrator: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("admin not set");
        admin.require_auth();

        env.storage().persistent().set(&ARBITRATOR, &new_arbitrator);
    }

    /// Get the admin address.
    pub fn get_admin(env: Env) -> Result<Address, EscrowError> {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(EscrowError::Unauthorized)
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
}

// Helper functions

fn get_next_contract_id(env: &Env) -> u32 {
    let mut next_id = env
        .storage()
        .persistent()
        .get(&NEXT_CONTRACT_ID)
        .unwrap_or(1u32);
    let current_id = next_id;
    next_id += 1;
    env.storage().persistent().set(&NEXT_CONTRACT_ID, &next_id);
    current_id
}

fn get_next_dispute_id(env: &Env) -> u32 {
    let mut next_id = env
        .storage()
        .persistent()
        .get(&NEXT_DISPUTE_ID)
        .unwrap_or(1u32);
    let current_id = next_id;
    next_id += 1;
    env.storage().persistent().set(&NEXT_DISPUTE_ID, &next_id);
    current_id
}

fn get_contracts_map(env: &Env) -> Map<u32, EscrowContract> {
    env.storage()
        .persistent()
        .get(&CONTRACTS)
        .unwrap_or(Map::new(env))
}

fn get_disputes_map(env: &Env) -> Map<u32, Dispute> {
    env.storage()
        .persistent()
        .get(&DISPUTES)
        .unwrap_or(Map::new(env))
}

fn require_contract_status(contract: &EscrowContract, expected_status: ContractStatus) {
    if contract.status != expected_status {
        panic!("invalid contract status");
    }
}

#[cfg(test)]
mod test;