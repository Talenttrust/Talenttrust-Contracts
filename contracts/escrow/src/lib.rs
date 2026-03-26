#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, String,
    Symbol, Vec,
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

/// Default timeout duration: 30 days in seconds (30 * 24 * 60 * 60)
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 2_592_000;

/// Minimum timeout duration: 1 day in seconds
pub const MIN_TIMEOUT_SECONDS: u64 = 86_400;

/// Maximum timeout duration: 365 days in seconds
pub const MAX_TIMEOUT_SECONDS: u64 = 31_536_000;

/// Data keys for contract storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    // Pause controls (instance storage)
    Admin,
    Paused,
    EmergencyPaused,

    // Escrow state (persistent storage)
    NextContractId,
    Contract(u32),

    // Dispute state (persistent storage)
    Dispute(u32),

    // Reputation / governance
    Reputation(Address),
    PendingReputationCredits(Address),
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
}

/// Status of an escrow contract
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
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
    pub released_milestones: u32,
    pub status: ContractStatus,
    pub reputation_issued: bool,
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

/// Custom errors for the escrow contract.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    InvalidContractId = 1,
    InvalidMilestoneId = 2,
    InvalidAmount = 3,
    InvalidRating = 4,
    EmptyMilestones = 5,
    InvalidParticipants = 6,

    ContractNotFound = 7,
    AmountMustBePositive = 8,
    FundingExceedsRequired = 9,
    InvalidState = 10,
    InsufficientEscrowBalance = 11,
    MilestoneNotFound = 12,
    MilestoneAlreadyReleased = 13,
    ReputationAlreadyIssued = 14,

    DisputeAlreadyOpen = 15,
    NoDisputeActive = 16,
    DisputeNotResolved = 17,
    DisputeAlreadyPaidOut = 18,
    Unauthorized = 19,
}

/// Immutable record created when a dispute is initiated.
/// Written once to persistent storage and never overwritten.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRecord {
    pub initiator: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Evidence attached to an open dispute.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeEvidence {
    pub submitter: Address,
    pub uri: String,
    pub timestamp: u64,
}

/// Resolution outcomes for disputes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeOutcome {
    Client,
    Freelancer,
    Split(i128),
}

/// Resolution record written when the dispute is resolved.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeResolution {
    pub resolver: Address,
    pub outcome: DisputeOutcome,
    pub timestamp: u64,
}

/// Stored dispute state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeState {
    pub record: DisputeRecord,
    pub evidence: Vec<DisputeEvidence>,
    pub resolution: Vec<DisputeResolution>,
    pub paid_out: bool,
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

    fn dispute_key(contract_id: u32) -> DataKey {
        DataKey::Dispute(contract_id)
    }

    fn load_dispute(env: &Env, contract_id: u32) -> Option<DisputeState> {
        env.storage().persistent().get(&Self::dispute_key(contract_id))
    }

    fn save_dispute(env: &Env, contract_id: u32, dispute: &DisputeState) {
        env.storage()
            .persistent()
            .set(&Self::dispute_key(contract_id), dispute);
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

    pub fn create_contract(env: Env, client: Address, freelancer: Address, milestones: Vec<i128>) -> u32 {
        Self::ensure_not_paused(&env);
        client.require_auth();

        if client == freelancer {
            panic_with_error!(&env, EscrowError::InvalidParticipants);
        }
        if milestones.is_empty() {
            panic_with_error!(&env, EscrowError::EmptyMilestones);
        }

        let parameters = Self::protocol_parameters(&env);
        if milestones.len() > parameters.max_milestones {
            panic!("too many milestones");
        }

        let mut milestone_vec: Vec<Milestone> = Vec::new(&env);
        let mut total_amount: i128 = 0;
        let mut i = 0_u32;
        while i < milestones.len() {
            let amount = milestones.get(i).unwrap();
            if amount < parameters.min_milestone_amount {
                panic_with_error!(&env, EscrowError::InvalidAmount);
            }
            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount));
            milestone_vec.push_back(Milestone {
                amount,
                released: false,
            });
            i += 1;
        }

        let contract_id = Self::next_contract_id(&env);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(contract_id + 1));

        let contract = EscrowContractData {
            client: client.clone(),
            freelancer,
            milestones: milestone_vec,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            released_milestones: 0,
            status: ContractStatus::Created,
            reputation_issued: false,
        };
        Self::save_contract(&env, contract_id, &contract);
        contract_id
    }

    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        Self::ensure_not_paused(&env);
        if amount <= 0 {
            panic_with_error!(&env, EscrowError::AmountMustBePositive);
        }

        let mut contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();

        if contract.status != ContractStatus::Created {
            panic_with_error!(&env, EscrowError::InvalidState);
        }

        let next_funded = contract
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount));
        if next_funded > contract.total_amount {
            panic_with_error!(&env, EscrowError::FundingExceedsRequired);
        }

        contract.funded_amount = next_funded;
        if contract.funded_amount == contract.total_amount {
            contract.status = ContractStatus::Funded;
        }

        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        Self::ensure_not_paused(&env);
        let mut contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();

        if contract.status != ContractStatus::Funded {
            panic_with_error!(&env, EscrowError::InvalidState);
        }
        if milestone_id >= contract.milestones.len() {
            panic_with_error!(&env, EscrowError::MilestoneNotFound);
        }

        let mut milestone = contract.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic_with_error!(&env, EscrowError::MilestoneAlreadyReleased);
        }

        if contract
            .released_amount
            .checked_add(milestone.amount)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount))
            > contract.funded_amount
        {
            panic_with_error!(&env, EscrowError::InsufficientEscrowBalance);
        }

        let milestone_amount = milestone.amount;
        milestone.released = true;
        contract.milestones.set(milestone_id, milestone);

        contract.released_amount = contract
            .released_amount
            .checked_add(milestone_amount)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount));
        contract.released_milestones += 1;

        if Self::all_milestones_released(&contract.milestones) {
            contract.status = ContractStatus::Completed;
            Self::add_pending_reputation_credit(&env, &contract.freelancer);
        }

        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn issue_reputation(env: Env, contract_id: u32, rating: i128) -> bool {
        Self::ensure_not_paused(&env);
        let contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();

        if contract.status != ContractStatus::Completed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }
        if contract.reputation_issued {
            panic_with_error!(&env, EscrowError::ReputationAlreadyIssued);
        }

        let params = Self::protocol_parameters(&env);
        if rating < params.min_reputation_rating || rating > params.max_reputation_rating {
            panic_with_error!(&env, EscrowError::InvalidRating);
        }

        let freelancer = contract.freelancer.clone();
        let credits = Self::get_pending_reputation_credits(env.clone(), freelancer.clone());
        if credits == 0 {
            panic!("no reputation credits available");
        }

        let key = DataKey::Reputation(freelancer.clone());
        let mut record = env
            .storage()
            .persistent()
            .get::<_, ReputationRecord>(&key)
            .unwrap_or(ReputationRecord {
                completed_contracts: 0,
                total_rating: 0,
                last_rating: 0,
            });

        record.completed_contracts += 1;
        record.total_rating += rating;
        record.last_rating = rating;
        env.storage().persistent().set(&key, &record);

        env.storage().persistent().set(
            &DataKey::PendingReputationCredits(freelancer.clone()),
            &(credits - 1),
        );

        let mut updated = contract;
        updated.reputation_issued = true;
        Self::save_contract(&env, contract_id, &updated);

        true
    }

    /// Opens a dispute for a funded contract.
    pub fn open_dispute(env: Env, contract_id: u32, initiator: Address, reason: String) -> bool {
        Self::ensure_not_paused(&env);
        initiator.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Funded {
            panic_with_error!(&env, EscrowError::InvalidState);
        }
        if initiator != contract.client && initiator != contract.freelancer {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }
        if Self::load_dispute(&env, contract_id).is_some() {
            panic_with_error!(&env, EscrowError::DisputeAlreadyOpen);
        }

        contract.status = ContractStatus::Disputed;
        Self::save_contract(&env, contract_id, &contract);

        let dispute = DisputeState {
            record: DisputeRecord {
                initiator,
                reason,
                timestamp: env.ledger().timestamp(),
            },
            evidence: Vec::new(&env),
            resolution: Vec::new(&env),
            paid_out: false,
        };
        Self::save_dispute(&env, contract_id, &dispute);
        true
    }

    /// Appends evidence to an open dispute.
    pub fn submit_dispute_evidence(
        env: Env,
        contract_id: u32,
        submitter: Address,
        uri: String,
    ) -> bool {
        Self::ensure_not_paused(&env);
        submitter.require_auth();

        let contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Disputed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }
        if submitter != contract.client && submitter != contract.freelancer {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        let mut dispute = Self::load_dispute(&env, contract_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NoDisputeActive));
        if dispute.resolution.len() > 0 {
            panic!("dispute already resolved");
        }

        dispute.evidence.push_back(DisputeEvidence {
            submitter,
            uri,
            timestamp: env.ledger().timestamp(),
        });

        Self::save_dispute(&env, contract_id, &dispute);
        true
    }

    /// Resolves an open dispute.
    ///
    /// Security: resolution is restricted to the pause-control admin.
    pub fn resolve_dispute(
        env: Env,
        contract_id: u32,
        resolver: Address,
        outcome: DisputeOutcome,
    ) -> bool {
        Self::ensure_not_paused(&env);
        resolver.require_auth();

        let admin = Self::read_admin(&env);
        if resolver != admin {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        let contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Disputed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }

        let mut dispute = Self::load_dispute(&env, contract_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NoDisputeActive));
        if dispute.resolution.len() > 0 {
            panic!("dispute already resolved");
        }

        dispute.resolution.push_back(DisputeResolution {
            resolver,
            outcome,
            timestamp: env.ledger().timestamp(),
        });
        Self::save_dispute(&env, contract_id, &dispute);
        true
    }

    /// Applies the dispute resolution, marking the contract as completed.
    pub fn payout_dispute(env: Env, contract_id: u32) -> bool {
        Self::ensure_not_paused(&env);

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Disputed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }

        let mut dispute = Self::load_dispute(&env, contract_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NoDisputeActive));

        if dispute.resolution.len() == 0 {
            panic_with_error!(&env, EscrowError::DisputeNotResolved);
        }
        let resolution = dispute.resolution.get(0).unwrap();
        if dispute.paid_out {
            panic_with_error!(&env, EscrowError::DisputeAlreadyPaidOut);
        }

        // This contract implementation does not move tokens; payout is recorded
        // as ledger state updates for testing / integration.
        match resolution.outcome {
            DisputeOutcome::Client => {
                // No additional release recorded.
            }
            DisputeOutcome::Freelancer => {
                contract.released_amount = contract.funded_amount;
            }
            DisputeOutcome::Split(freelancer_amount) => {
                if freelancer_amount < 0 || freelancer_amount > contract.funded_amount {
                    panic_with_error!(&env, EscrowError::InvalidAmount);
                }
                contract.released_amount = freelancer_amount;
            }
        }

        contract.status = ContractStatus::Completed;
        Self::save_contract(&env, contract_id, &contract);

        dispute.paid_out = true;
        Self::save_dispute(&env, contract_id, &dispute);
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

    pub fn get_dispute(env: Env, contract_id: u32) -> Option<DisputeState> {
        Self::load_dispute(&env, contract_id)
    }

    pub fn initialize_protocol_governance(
        env: Env,
        admin: Address,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        admin.require_auth();
        if env.storage().persistent().has(&DataKey::GovernanceAdmin) {
            panic!("protocol governance already initialized");
        }

        let params = Self::validated_protocol_parameters(
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );
        env.storage().persistent().set(&DataKey::GovernanceAdmin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &params);
        true
    }

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

    pub fn propose_governance_admin(env: Env, pending_admin: Address) -> bool {
        let admin = Self::governance_admin(&env);
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &pending_admin);
        true
    }

    pub fn accept_governance_admin(env: Env) -> bool {
        let pending = Self::pending_governance_admin(&env)
            .unwrap_or_else(|| panic!("no pending governance admin"));
        pending.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &pending);
        env.storage().persistent().remove(&DataKey::PendingGovernanceAdmin);
        true
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
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::ContractNotFound))
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
