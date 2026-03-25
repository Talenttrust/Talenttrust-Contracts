#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec};

// ============================================================================
// Constants & Configuration
// ============================================================================

/// Default minimum milestone amount
const DEFAULT_MIN_MILESTONE_AMOUNT: i128 = 1;

/// Default maximum number of milestones per contract
const DEFAULT_MAX_MILESTONES: u32 = 16;

/// Default minimum reputation rating
const DEFAULT_MIN_REPUTATION_RATING: i128 = 1;

/// Default maximum reputation rating
const DEFAULT_MAX_REPUTATION_RATING: i128 = 5;

/// Maximum fee basis points (100% = 10000 basis points)
pub const MAX_FEE_BASIS_POINTS: u32 = 10000;

/// Default protocol fee: 2.5% = 250 basis points
pub const DEFAULT_FEE_BASIS_POINTS: u32 = 250;

/// Default timeout duration: 30 days in seconds
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 2_592_000;

/// Minimum timeout duration
pub const MIN_TIMEOUT_SECONDS: u64 = 86_400;

/// Maximum timeout duration
pub const MAX_TIMEOUT_SECONDS: u64 = 31_536_000;

// ============================================================================
// Data Types & Enums
// ============================================================================

/// Data keys for contract storage
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Paused,
    EmergencyPaused,
    TreasuryConfig,
    Contract(u32),
    NextContractId,
    Reputation(Address),
    PendingReputationCredits(Address),
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
    Dispute(u32),
}

/// Status of an escrow contract lifecycle
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    InDispute = 3,
    Closed = 4,
}

/// Authorization scheme for milestone releases
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly = 0,
    ArbiterOnly = 1,
    ClientAndArbiter = 2,
    MultiSig = 3,
}

/// Individual milestone tracked inside an escrow agreement
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
    pub approved_by: Option<Address>,
    pub approval_timestamp: Option<u64>,
}

/// Custom errors for the escrow contract
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowError {
    TreasuryNotInitialized = 1,
    InvalidFeePercentage = 2,
    Unauthorized = 3,
    ContractNotFound = 4,
    MilestoneNotFound = 5,
    MilestoneAlreadyReleased = 6,
    InsufficientFunds = 7,
    InvalidAmount = 8,
    TreasuryAlreadyInitialized = 9,
    ArithmeticOverflow = 10,
    TimeoutNotExceeded = 11,
    InvalidTimeout = 12,
    MilestoneNotComplete = 13,
    MilestoneAlreadyComplete = 14,
    DisputeNotFound = 15,
    DisputeAlreadyResolved = 16,
    EmptyMilestones = 17,
    InvalidContractId = 18,
    InvalidMilestoneId = 19,
    InvalidParticipant = 20,
    ContractPaused = 21,
}

/// Full on-chain state of an escrow contract
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowContract {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<Milestone>,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub status: ContractStatus,
    pub release_auth: ReleaseAuthorization,
    pub created_at: u64,
}

/// Reputation state derived from completed escrow contracts
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub completed_contracts: u32,
    pub total_rating: i128,
    pub last_rating: i128,
}

/// Governed protocol parameters
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolParameters {
    pub min_milestone_amount: i128,
    pub max_milestones: u32,
    pub min_reputation_rating: i128,
    pub max_reputation_rating: i128,
}

/// Treasury configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryConfig {
    pub address: Address,
    pub fee_basis_points: u32,
}

/// Dispute structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct Dispute {
    pub initiator: Address,
    pub reason: String,
    pub created_at: u64,
    pub resolved: bool,
}

// ============================================================================
// Event Types
// ============================================================================

/// Event emitted when a new escrow contract is created
#[contracttype]
#[derive(Clone, Debug)]
pub struct ContractCreatedEvent {
    pub contract_id: u32,
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestone_count: u32,
    pub total_amount: i128,
    pub release_auth: u8,
    pub timestamp: u64,
}

/// Event emitted when funds are deposited
#[contracttype]
#[derive(Clone, Debug)]
pub struct ContractFundedEvent {
    pub contract_id: u32,
    pub depositor: Address,
    pub amount: i128,
    pub funded_amount: i128,
    pub is_fully_funded: bool,
    pub timestamp: u64,
}

/// Event emitted when a milestone is released
#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneReleasedEvent {
    pub contract_id: u32,
    pub milestone_id: u32,
    pub amount: i128,
    pub approved_by: Address,
    pub all_released: bool,
    pub timestamp: u64,
}

/// Event emitted when a dispute is initiated
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeInitiatedEvent {
    pub contract_id: u32,
    pub initiator: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Event emitted when a dispute is resolved
#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeResolvedEvent {
    pub contract_id: u32,
    pub resolution: String,
    pub new_status: u8,
    pub timestamp: u64,
}

/// Event emitted when a contract is closed
#[contracttype]
#[derive(Clone, Debug)]
pub struct ContractClosedEvent {
    pub contract_id: u32,
    pub reason: String,
    pub final_status: u8,
    pub total_released: i128,
    pub timestamp: u64,
}

// ============================================================================
// Contract Implementation
// ============================================================================

#[contract]
pub struct Escrow;

impl Escrow {
    fn read_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("Admin not initialized"))
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

    fn load_contract(env: &Env, contract_id: u32) -> EscrowContract {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| panic!("Contract not found"))
    }

    fn save_contract(env: &Env, contract_id: u32, contract: &EscrowContract) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), contract);
    }

    fn next_contract_id(env: &Env) -> u32 {
        let id: u32 = env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(id + 1));
        id
    }

    fn validate_milestones(env: &Env, milestone_amounts: &Vec<i128>) -> Result<(), EscrowError> {
        if milestone_amounts.is_empty() {
            return Err(EscrowError::EmptyMilestones);
        }

        let params = Self::protocol_parameters(env);

        if milestone_amounts.len() as u32 > params.max_milestones {
            return Err(EscrowError::InvalidAmount);
        }

        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts.get(i).unwrap();
            if amount < params.min_milestone_amount {
                return Err(EscrowError::InvalidAmount);
            }
        }

        Ok(())
    }

    fn emit_contract_created(
        env: &Env,
        contract_id: u32,
        client: &Address,
        freelancer: &Address,
        arbiter: &Option<Address>,
        milestone_count: u32,
        total_amount: i128,
        release_auth: ReleaseAuthorization,
    ) {
        let event = ContractCreatedEvent {
            contract_id,
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: arbiter.clone(),
            milestone_count,
            total_amount,
            release_auth: release_auth as u8,
            timestamp: env.ledger().timestamp(),
        };

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("created")),
            event,
        );
    }

    fn emit_contract_funded(
        env: &Env,
        contract_id: u32,
        depositor: &Address,
        amount: i128,
        funded_amount: i128,
        is_fully_funded: bool,
    ) {
        let event = ContractFundedEvent {
            contract_id,
            depositor: depositor.clone(),
            amount,
            funded_amount,
            is_fully_funded,
            timestamp: env.ledger().timestamp(),
        };

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("funded")),
            event,
        );
    }

    fn emit_milestone_released(
        env: &Env,
        contract_id: u32,
        milestone_id: u32,
        amount: i128,
        approved_by: &Address,
        all_released: bool,
    ) {
        let event = MilestoneReleasedEvent {
            contract_id,
            milestone_id,
            amount,
            approved_by: approved_by.clone(),
            all_released,
            timestamp: env.ledger().timestamp(),
        };

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("released")),
            event,
        );
    }

    fn emit_dispute_initiated(
        env: &Env,
        contract_id: u32,
        initiator: &Address,
        reason: &String,
    ) {
        let event = DisputeInitiatedEvent {
            contract_id,
            initiator: initiator.clone(),
            reason: reason.clone(),
            timestamp: env.ledger().timestamp(),
        };

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("dispute_initiated")),
            event,
        );
    }

    fn emit_dispute_resolved(
        env: &Env,
        contract_id: u32,
        resolution: &String,
        new_status: ContractStatus,
    ) {
        let event = DisputeResolvedEvent {
            contract_id,
            resolution: resolution.clone(),
            new_status: new_status as u8,
            timestamp: env.ledger().timestamp(),
        };

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("dispute_resolved")),
            event,
        );
    }

    fn emit_contract_closed(
        env: &Env,
        contract_id: u32,
        reason: &String,
        final_status: ContractStatus,
        total_released: i128,
    ) {
        let event = ContractClosedEvent {
            contract_id,
            reason: reason.clone(),
            final_status: final_status as u8,
            total_released,
            timestamp: env.ledger().timestamp(),
        };

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("closed")),
            event,
        );
    }

    fn all_milestones_released(milestones: &Vec<Milestone>) -> bool {
        let mut index = 0;
        while index < milestones.len() {
            let milestone = milestones.get(index).unwrap();
            if !milestone.released {
                return false;
            }
            index += 1;
        }
        true
    }

    fn add_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let key = DataKey::PendingReputationCredits(freelancer.clone());
        let current = env.storage()
            .persistent()
            .get::<_, u32>(&key)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &(current + 1));
    }
}

#[contractimpl]
impl Escrow {
    /// Initializes the escrow contract with an admin
    pub fn initialize(env: Env, admin: Address) -> bool {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        true
    }

    /// Returns the configured admin
    pub fn get_admin(env: Env) -> Address {
        Self::read_admin(&env)
    }

    /// Pauses state-changing operations
    pub fn pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    /// Unpauses the contract
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

    /// Activates emergency mode
    pub fn activate_emergency_pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &true);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    /// Resolves emergency mode
    pub fn resolve_emergency(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        env.storage().instance().set(&DataKey::Paused, &false);
        true
    }

    /// Returns the pause status
    pub fn is_paused(env: Env) -> bool {
        Self::is_paused_internal(&env)
    }

    /// Returns the emergency status
    pub fn is_emergency(env: Env) -> bool {
        Self::is_emergency_internal(&env)
    }

    /// Creates a new escrow contract
    ///
    /// Emits: ContractCreatedEvent
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
    ) -> u32 {
        Self::ensure_not_paused(&env);
        client.require_auth();

        if client == freelancer {
            panic!("Client and freelancer cannot be the same");
        }

        Self::validate_milestones(&env, &milestone_amounts).unwrap_or_else(|_| {
            panic!("Invalid milestone configuration");
        });

        let mut total_amount: i128 = 0;
        let mut milestones = Vec::new(&env);

        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts.get(i).unwrap();
            total_amount = total_amount.checked_add(amount).unwrap_or_else(|| {
                panic!("Total amount overflow");
            });

            milestones.push_back(Milestone {
                amount,
                released: false,
                approved_by: None,
                approval_timestamp: None,
            });
        }

        let contract_id = Self::next_contract_id(&env);
        let contract = EscrowContract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: arbiter.clone(),
            milestones,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            status: ContractStatus::Created,
            release_auth,
            created_at: env.ledger().timestamp(),
        };

        Self::save_contract(&env, contract_id, &contract);

        Self::emit_contract_created(
            &env,
            contract_id,
            &client,
            &freelancer,
            &arbiter,
            milestone_amounts.len() as u32,
            total_amount,
            release_auth,
        );

        contract_id
    }

    /// Deposits funds into an escrow contract
    ///
    /// Emits: ContractFundedEvent
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        Self::ensure_not_paused(&env);
        let caller = env.invoker();

        let mut contract = Self::load_contract(&env, contract_id);

        if caller != contract.client {
            panic!("Only client can deposit");
        }

        if contract.status != ContractStatus::Created {
            panic!("Contract not in Created state");
        }

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let new_funded = contract.funded_amount.checked_add(amount).unwrap_or_else(|| {
            panic!("Overflow");
        });

        if new_funded > contract.total_amount {
            panic!("Exceeds total amount");
        }

        contract.funded_amount = new_funded;

        let is_fully_funded = contract.funded_amount == contract.total_amount;
        if is_fully_funded {
            contract.status = ContractStatus::Funded;
        }

        Self::save_contract(&env, contract_id, &contract);

        Self::emit_contract_funded(
            &env,
            contract_id,
            &caller,
            amount,
            contract.funded_amount,
            is_fully_funded,
        );

        true
    }

    /// Approves a milestone for release
    pub fn approve_milestone_release(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        Self::ensure_not_paused(&env);
        let caller = env.invoker();

        let mut contract = Self::load_contract(&env, contract_id);

        if contract.status != ContractStatus::Funded {
            panic!("Contract not Funded");
        }

        if milestone_id >= contract.milestones.len() as u32 {
            panic!("Invalid milestone");
        }

        let mut milestone = contract.milestones.get(milestone_id).unwrap();

        if milestone.released {
            panic!("Already released");
        }

        let is_authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => caller == contract.client,
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::ClientAndArbiter => {
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::MultiSig => {
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
        };

        if !is_authorized {
            panic!("Not authorized");
        }

        if milestone.approved_by.clone().map_or(false, |a| a == caller) {
            panic!("Already approved by this address");
        }

        milestone.approved_by = Some(caller.clone());
        milestone.approval_timestamp = Some(env.ledger().timestamp());

        contract.milestones.set(milestone_id, milestone);
        Self::save_contract(&env, contract_id, &contract);

        true
    }

    /// Releases a milestone payment
    ///
    /// Emits: MilestoneReleasedEvent, ContractClosedEvent (if all released)
    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        Self::ensure_not_paused(&env);
        let _caller = env.invoker();

        let mut contract = Self::load_contract(&env, contract_id);

        if contract.status != ContractStatus::Funded {
            panic!("Contract not Funded");
        }

        if milestone_id >= contract.milestones.len() as u32 {
            panic!("Invalid milestone");
        }

        let mut milestone = contract.milestones.get(milestone_id).unwrap();

        if milestone.released {
            panic!("Already released");
        }

        if milestone.approved_by.is_none() {
            panic!("Not approved");
        }

        let amount = milestone.amount;

        milestone.released = true;
        contract.milestones.set(milestone_id, milestone.clone());
        contract.released_amount = contract.released_amount.checked_add(amount).unwrap_or_else(|| {
            panic!("Overflow");
        });

        let all_released = Self::all_milestones_released(&contract.milestones);
        if all_released {
            contract.status = ContractStatus::Completed;
        }

        Self::save_contract(&env, contract_id, &contract);

        if let Some(approved_by) = &milestone.approved_by {
            Self::emit_milestone_released(
                &env,
                contract_id,
                milestone_id,
                amount,
                approved_by,
                all_released,
            );
        }

        if all_released {
            let reason = String::from_slice(&env, "All milestones released");
            Self::emit_contract_closed(
                &env,
                contract_id,
                &reason,
                ContractStatus::Completed,
                contract.released_amount,
            );

            Self::add_pending_reputation_credit(&env, &contract.freelancer);
        }

        true
    }

    /// Initiates a dispute
    ///
    /// Emits: DisputeInitiatedEvent
    pub fn initiate_dispute(env: Env, contract_id: u32, reason: String) -> bool {
        Self::ensure_not_paused(&env);
        let caller = env.invoker();

        let mut contract = Self::load_contract(&env, contract_id);

        if caller != contract.client && caller != contract.freelancer {
            panic!("Only parties can initiate dispute");
        }

        if contract.status == ContractStatus::InDispute {
            panic!("Already in dispute");
        }

        let dispute = Dispute {
            initiator: caller.clone(),
            reason: reason.clone(),
            created_at: env.ledger().timestamp(),
            resolved: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Dispute(contract_id), &dispute);

        contract.status = ContractStatus::InDispute;
        Self::save_contract(&env, contract_id, &contract);

        Self::emit_dispute_initiated(&env, contract_id, &caller, &reason);

        true
    }

    /// Resolves a dispute
    ///
    /// Emits: DisputeResolvedEvent
    pub fn resolve_dispute(env: Env, contract_id: u32, resolution_type: u32) -> bool {
        Self::ensure_not_paused(&env);
        Self::require_admin(&env);

        let mut contract = Self::load_contract(&env, contract_id);

        if contract.status != ContractStatus::InDispute {
            panic!("Not in dispute");
        }

        let mut dispute: Dispute = env.storage()
            .persistent()
            .get(&DataKey::Dispute(contract_id))
            .unwrap_or_else(|| panic!("Dispute not found"));

        if dispute.resolved {
            panic!("Already resolved");
        }

        dispute.resolved = true;
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(contract_id), &dispute);

        let (new_status, resolution_str) = match resolution_type {
            0 => (
                ContractStatus::Closed,
                String::from_slice(&env, "Refunded to client"),
            ),
            1 => (
                ContractStatus::Completed,
                String::from_slice(&env, "Released to freelancer"),
            ),
            2 => (
                ContractStatus::Completed,
                String::from_slice(&env, "Split between parties"),
            ),
            _ => panic!("Invalid resolution type"),
        };

        contract.status = new_status;
        Self::save_contract(&env, contract_id, &contract);

        Self::emit_dispute_resolved(&env, contract_id, &resolution_str, new_status);

        true
    }

    /// Closes a contract
    ///
    /// Emits: ContractClosedEvent
    pub fn close_contract(env: Env, contract_id: u32, reason: String) -> bool {
        Self::ensure_not_paused(&env);
        let caller = env.invoker();

        let mut contract = Self::load_contract(&env, contract_id);

        if caller != contract.client && caller != contract.freelancer {
            panic!("Only parties can close");
        }

        if contract.status == ContractStatus::Created {
            panic!("Cannot close created contract");
        }

        contract.status = ContractStatus::Closed;
        Self::save_contract(&env, contract_id, &contract);

        Self::emit_contract_closed(
            &env,
            contract_id,
            &reason,
            ContractStatus::Closed,
            contract.released_amount,
        );

        true
    }

    /// Gets contract details
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContract {
        Self::load_contract(&env, contract_id)
    }

    /// Gets reputation record
    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    /// Gets pending reputation credits
    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    /// Gets protocol parameters
    pub fn get_protocol_parameters(env: Env) -> ProtocolParameters {
        Self::protocol_parameters(&env)
    }

    /// Hello world test
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_events;
