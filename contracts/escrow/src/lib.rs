#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

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
    InDispute = 4,
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

/// Storage keys for contract data.
#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Paused,
    EmergencyPaused,
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

/// Custom errors for the escrow contract
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowError {
    /// Treasury not initialized
    TreasuryNotInitialized = 1,
    /// Invalid fee percentage (exceeds 100%)
    InvalidFeePercentage = 2,
    /// Unauthorized access
    Unauthorized = 3,
    /// Contract not found
    ContractNotFound = 4,
    /// Milestone not found
    MilestoneNotFound = 5,
    /// Milestone already released
    MilestoneAlreadyReleased = 6,
    /// Insufficient funds
    InsufficientFunds = 7,
    /// Invalid amount
    InvalidAmount = 8,
    /// Treasury already initialized
    TreasuryAlreadyInitialized = 9,
    /// Arithmetic overflow
    ArithmeticOverflow = 10,
    /// Timeout not exceeded
    TimeoutNotExceeded = 11,
    /// Invalid timeout duration
    InvalidTimeout = 12,
    /// Milestone not marked complete
    MilestoneNotComplete = 13,
    /// Milestone already complete
    MilestoneAlreadyComplete = 14,
    /// Dispute not found
    DisputeNotFound = 15,
    /// Dispute already resolved
    DisputeAlreadyResolved = 16,
    /// Timeout already claimed
    TimeoutAlreadyClaimed = 17,
    /// No dispute active
    NoDisputeActive = 18,
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

    fn ensure_valid_milestones(milestone_amounts: &Vec<i128>) -> Result<(), EscrowError> {
        if milestone_amounts.is_empty() {
            return Err(EscrowError::EmptyMilestones);
        }

        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts.get(i).unwrap();
            if amount <= 0 {
                return Err(EscrowError::InvalidAmount);
            }
        }

        let mut milestones = Vec::new(&env);
        for i in 0..milestone_amounts.len() {
            milestones.push_back(Milestone {
                amount: milestone_amounts.get(i).unwrap(),
                released: false,
                approved_by: None,
                approval_timestamp: None,
            });
        }

        let contract_data = EscrowContract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            milestones,
            status: ContractStatus::Created,
            release_auth,
            created_at: env.ledger().timestamp(),
        };

        let contract_id = env.ledger().sequence();

        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract_data);

        contract_id
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

        let mut total_required = 0i128;
        for i in 0..contract.milestones.len() {
            total_required += contract.milestones.get(i).unwrap().amount;
        }
        Ok(())
    }

    fn ensure_valid_milestone_id(milestone_id: u32) -> Result<(), EscrowError> {
        // `u32::MAX` is reserved as an invalid sentinel in this placeholder implementation.
        if milestone_id == u32::MAX {
            return Err(EscrowError::InvalidMilestoneId);
        }

        let mut updated_contract = contract;
        updated_contract.status = ContractStatus::Funded;
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &updated_contract);

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

        true
    }

    /// Issue a reputation credential for the freelancer after contract completion.
    pub fn issue_reputation(env: Env, _freelancer: Address, _rating: i128) -> bool {
        Self::ensure_not_paused(&env);

        true
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

    /// Initialize protocol governance with the first admin.
    ///
    /// # Panics
    /// - If governance is already initialized.
    pub fn initialize_governance(env: Env, admin: Address) -> bool {
        if env.storage().persistent().has(&DataKey::GovernanceAdmin) {
            panic!("protocol governance is already initialized");
        }

        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &admin);
        true
    }

    /// Update protocol parameters. Only the governance admin may call this.
    ///
    /// # Panics
    /// - If governance is not initialized.
    /// - If caller is not the governance admin.
    /// - If parameters fail validation.
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

    /// Propose a new governance admin. Only the current admin may call this.
    ///
    /// # Panics
    /// - If governance is not initialized.
    /// - If caller is not the current governance admin.
    pub fn propose_governance_admin(env: Env, new_admin: Address) -> bool {
        let admin = Self::governance_admin(&env);
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &new_admin);
        true
    }

    /// Accept governance admin transfer. Only the pending admin may call this.
    ///
    /// # Panics
    /// - If there is no pending admin transfer.
    /// - If caller is not the pending admin.
    pub fn accept_governance_admin(env: Env) -> bool {
        let pending_admin = Self::pending_governance_admin(&env)
            .unwrap_or_else(|| panic!("no pending admin transfer"));

        pending_admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &pending_admin);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingGovernanceAdmin);
        true
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

#[cfg(test)]
mod test;
