#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, Symbol,
    Vec,
};

const DEFAULT_MIN_MILESTONE_AMOUNT: i128 = 1;
const DEFAULT_MAX_MILESTONES: u32 = 16;
const DEFAULT_MIN_REPUTATION_RATING: i128 = 1;
const DEFAULT_MAX_REPUTATION_RATING: i128 = 5;

/// Persistent storage keys used by the escrow contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum DataKey {
    PauseAdmin,
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

/// High-level lifecycle state for an escrow agreement.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
}

/// Immutable milestone definition with a one-way release flag.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    /// Amount assigned to this milestone.
    pub amount: i128,
    /// Whether the milestone amount has been released.
    pub released: bool,
}

/// Persisted escrow state for a single agreement.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    /// Address that created and funds the escrow.
    pub client: Address,
    /// Address that receives released milestone funds.
    pub freelancer: Address,
    /// Ordered milestone definitions.
    pub milestones: Vec<Milestone>,
    /// Cached milestone count for cheaper reads and review clarity.
    pub milestone_count: u32,
    /// Sum of all milestone amounts.
    pub total_amount: i128,
    /// Total amount funded into escrow.
    pub funded_amount: i128,
    /// Total amount released to the freelancer.
    pub released_amount: i128,
    /// Number of milestones already released.
    pub released_milestones: u32,
    /// Current lifecycle status.
    pub status: ContractStatus,
    /// Whether a reputation rating has already been issued for this contract.
    pub reputation_issued: bool,
    /// Ledger timestamp when the record was created.
    pub created_at: u64,
    /// Ledger timestamp for the last persisted mutation.
    pub updated_at: u64,
}

/// Reputation aggregate derived from completed escrow contracts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    /// Number of completed contracts that were rated.
    pub completed_contracts: u32,
    /// Running sum of issued ratings.
    pub total_rating: i128,
    /// Most recent rating issued to the freelancer.
    pub last_rating: i128,
    /// Number of individual ratings recorded.
    pub ratings_count: u32,
}

/// Governance-controlled validation bounds for escrow creation and ratings.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolParameters {
    /// Minimum allowed milestone amount.
    pub min_milestone_amount: i128,
    /// Maximum number of milestones per contract.
    pub max_milestones: u32,
    /// Minimum allowed reputation rating.
    pub min_reputation_rating: i128,
    /// Maximum allowed reputation rating.
    pub max_reputation_rating: i128,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    ContractPaused = 3,
    EmergencyActive = 4,
    InvalidContractId = 5,
    ContractNotFound = 6,
    InvalidParticipants = 7,
    EmptyMilestones = 8,
    InvalidMilestoneAmount = 9,
    AmountMustBePositive = 10,
    FundingExceedsRequired = 11,
    InvalidState = 12,
    InsufficientEscrowBalance = 13,
    MilestoneNotFound = 14,
    MilestoneAlreadyReleased = 15,
    InvalidRating = 16,
    ReputationAlreadyIssued = 17,
    GovernanceAlreadyInitialized = 18,
    GovernanceNotInitialized = 19,
    InvalidProtocolParameters = 20,
    TooManyMilestones = 21,
    ArithmeticOverflow = 22,
    NotPaused = 23,
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Echo helper kept for template compatibility.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Initializes pause and emergency controls.
    pub fn initialize(env: Env, admin: Address) -> bool {
        if env.storage().persistent().has(&DataKey::PauseAdmin) {
            panic_with_error!(&env, EscrowError::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().persistent().set(&DataKey::PauseAdmin, &admin);
        env.storage().persistent().set(&DataKey::Paused, &false);
        env.storage()
            .persistent()
            .set(&DataKey::EmergencyPaused, &false);
        true
    }

    /// Returns the configured pause admin, if pause controls were initialized.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::PauseAdmin)
    }

    /// Pauses all state-changing escrow operations.
    pub fn pause(env: Env) -> bool {
        let admin = Self::read_pause_admin(&env);
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &true);
        true
    }

    /// Clears a standard pause, but refuses to clear an active emergency pause.
    pub fn unpause(env: Env) -> bool {
        let admin = Self::read_pause_admin(&env);
        admin.require_auth();

        if Self::is_emergency_active(&env) {
            panic_with_error!(&env, EscrowError::EmergencyActive);
        }

        if !Self::is_paused_flag_set(&env) {
            panic_with_error!(&env, EscrowError::NotPaused);
        }

        env.storage().persistent().set(&DataKey::Paused, &false);
        true
    }

    /// Activates emergency pause mode and blocks standard unpause until resolved.
    pub fn activate_emergency_pause(env: Env) -> bool {
        let admin = Self::read_pause_admin(&env);
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &true);
        env.storage()
            .persistent()
            .set(&DataKey::EmergencyPaused, &true);
        true
    }

    /// Resolves an emergency and restores normal operation.
    pub fn resolve_emergency(env: Env) -> bool {
        let admin = Self::read_pause_admin(&env);
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &false);
        env.storage()
            .persistent()
            .set(&DataKey::EmergencyPaused, &false);
        true
    }

    /// Returns whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        Self::is_paused_flag_set(&env)
    }

    /// Returns whether emergency mode is currently active.
    pub fn is_emergency(env: Env) -> bool {
        Self::is_emergency_active(&env)
    }

    /// Creates and persists an escrow agreement and its participant metadata.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
    ) -> u32 {
        Self::ensure_not_paused(&env);
        client.require_auth();

        if client == freelancer {
            panic_with_error!(&env, EscrowError::InvalidParticipants);
        }

        let milestone_count = milestone_amounts.len();
        if milestone_count == 0 {
            panic_with_error!(&env, EscrowError::EmptyMilestones);
        }

        let parameters = Self::read_protocol_parameters(&env);
        if milestone_count > parameters.max_milestones {
            panic_with_error!(&env, EscrowError::TooManyMilestones);
        }

        let mut total_amount = 0_i128;
        let mut milestones = Vec::new(&env);
        for amount in milestone_amounts.iter() {
            if amount < parameters.min_milestone_amount {
                panic_with_error!(&env, EscrowError::InvalidMilestoneAmount);
            }

            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic_with_error!(&env, EscrowError::ArithmeticOverflow));

            milestones.push_back(Milestone {
                amount,
                released: false,
            });
        }

        let contract_id = Self::next_contract_id(&env);
        let timestamp = env.ledger().timestamp();
        let record = EscrowContractData {
            client,
            freelancer,
            milestones,
            milestone_count,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            released_milestones: 0,
            status: ContractStatus::Created,
            reputation_issued: false,
            created_at: timestamp,
            updated_at: timestamp,
        };

        Self::write_contract(&env, contract_id, &record);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(contract_id + 1));
        contract_id
    }

    /// Adds funds to an escrow agreement without exceeding the required total.
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        Self::ensure_not_paused(&env);
        Self::require_valid_contract_id(&env, contract_id);
        Self::require_positive_amount(&env, amount);

        let mut record = Self::read_contract(&env, contract_id);
        record.client.require_auth();

        let next_funded_amount = record
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::ArithmeticOverflow));
        if next_funded_amount > record.total_amount {
            panic_with_error!(&env, EscrowError::FundingExceedsRequired);
        }

        if record.status == ContractStatus::Completed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }

        record.funded_amount = next_funded_amount;
        if record.funded_amount > 0 {
            record.status = ContractStatus::Funded;
        }
        record.updated_at = env.ledger().timestamp();

        Self::write_contract(&env, contract_id, &record);
        true
    }

    /// Releases a funded milestone and advances lifecycle state.
    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        Self::ensure_not_paused(&env);
        Self::require_valid_contract_id(&env, contract_id);

        let mut record = Self::read_contract(&env, contract_id);
        record.client.require_auth();

        if record.status != ContractStatus::Funded && record.status != ContractStatus::Completed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }

        if milestone_id >= record.milestone_count {
            panic_with_error!(&env, EscrowError::MilestoneNotFound);
        }

        let mut milestone = record
            .milestones
            .get(milestone_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::MilestoneNotFound));
        if milestone.released {
            panic_with_error!(&env, EscrowError::MilestoneAlreadyReleased);
        }

        let available_balance = record
            .funded_amount
            .checked_sub(record.released_amount)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::ArithmeticOverflow));
        if available_balance < milestone.amount {
            panic_with_error!(&env, EscrowError::InsufficientEscrowBalance);
        }

        milestone.released = true;
        record.milestones.set(milestone_id, milestone);
        record.released_amount = record
            .released_amount
            .checked_add(
                record
                    .milestones
                    .get(milestone_id)
                    .unwrap_or_else(|| panic_with_error!(&env, EscrowError::MilestoneNotFound))
                    .amount,
            )
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::ArithmeticOverflow));
        record.released_milestones += 1;
        record.updated_at = env.ledger().timestamp();

        if record.released_milestones == record.milestone_count {
            record.status = ContractStatus::Completed;
            let pending_key = DataKey::PendingReputationCredits(record.freelancer.clone());
            let pending: u32 = env.storage().persistent().get(&pending_key).unwrap_or(0);
            env.storage().persistent().set(&pending_key, &(pending + 1));
        } else {
            record.status = ContractStatus::Funded;
        }

        Self::write_contract(&env, contract_id, &record);
        true
    }

    /// Issues a governed reputation rating for a completed contract exactly once.
    pub fn issue_reputation(env: Env, contract_id: u32, rating: i128) -> bool {
        Self::ensure_not_paused(&env);
        Self::require_valid_contract_id(&env, contract_id);

        let parameters = Self::read_protocol_parameters(&env);
        if rating < parameters.min_reputation_rating || rating > parameters.max_reputation_rating {
            panic_with_error!(&env, EscrowError::InvalidRating);
        }

        let mut record = Self::read_contract(&env, contract_id);
        record.client.require_auth();

        if record.status != ContractStatus::Completed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }
        if record.reputation_issued {
            panic_with_error!(&env, EscrowError::ReputationAlreadyIssued);
        }

        let pending_key = DataKey::PendingReputationCredits(record.freelancer.clone());
        let pending_credits: u32 = env.storage().persistent().get(&pending_key).unwrap_or(0);
        if pending_credits == 0 {
            panic_with_error!(&env, EscrowError::InvalidState);
        }

        let rep_key = DataKey::Reputation(record.freelancer.clone());
        let mut reputation = env
            .storage()
            .persistent()
            .get(&rep_key)
            .unwrap_or(ReputationRecord {
                completed_contracts: 0,
                total_rating: 0,
                last_rating: 0,
                ratings_count: 0,
            });
        reputation.completed_contracts += 1;
        reputation.ratings_count += 1;
        reputation.total_rating = reputation
            .total_rating
            .checked_add(rating)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::ArithmeticOverflow));
        reputation.last_rating = rating;

        env.storage().persistent().set(&rep_key, &reputation);
        env.storage()
            .persistent()
            .set(&pending_key, &(pending_credits - 1));

        record.reputation_issued = true;
        record.updated_at = env.ledger().timestamp();
        Self::write_contract(&env, contract_id, &record);
        true
    }

    /// Returns the persisted escrow agreement for a contract id.
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::require_valid_contract_id(&env, contract_id);
        Self::read_contract(&env, contract_id)
    }

    /// Returns reputation data for a freelancer, if any has been issued.
    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    /// Returns how many completed contracts can still issue reputation for the freelancer.
    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    /// Initializes protocol governance and persisted validation bounds.
    pub fn initialize_protocol_governance(
        env: Env,
        admin: Address,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        if env.storage().persistent().has(&DataKey::GovernanceAdmin) {
            panic_with_error!(&env, EscrowError::GovernanceAlreadyInitialized);
        }

        admin.require_auth();
        let parameters = Self::validate_protocol_parameters(
            &env,
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
            .set(&DataKey::ProtocolParameters, &parameters);
        true
    }

    /// Updates live validation bounds for future escrow creation and ratings.
    pub fn update_protocol_parameters(
        env: Env,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        let admin = Self::read_governance_admin(&env);
        admin.require_auth();

        let parameters = Self::validate_protocol_parameters(
            &env,
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &parameters);
        true
    }

    /// Returns the live protocol validation bounds, falling back to safe defaults.
    pub fn get_protocol_parameters(env: Env) -> ProtocolParameters {
        Self::read_protocol_parameters(&env)
    }

    /// Returns the governance admin, if governance has been initialized.
    pub fn get_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::GovernanceAdmin)
    }

    /// Returns the pending governance admin for the two-step handover flow.
    pub fn get_pending_governance_admin(env: Env) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingGovernanceAdmin)
    }

    /// Proposes a new governance admin.
    pub fn propose_governance_admin(env: Env, next_admin: Address) -> bool {
        let admin = Self::read_governance_admin(&env);
        admin.require_auth();

        if admin == next_admin {
            panic_with_error!(&env, EscrowError::InvalidParticipants);
        }

        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &next_admin);
        true
    }

    /// Accepts a pending governance admin transfer.
    pub fn accept_governance_admin(env: Env) -> bool {
        let pending_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingGovernanceAdmin)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidState));
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
    fn read_pause_admin(env: &Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::PauseAdmin)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotInitialized))
    }

    fn read_governance_admin(env: &Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::GovernanceAdmin)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::GovernanceNotInitialized))
    }

    fn next_contract_id(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    }

    fn read_contract(env: &Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::ContractNotFound))
    }

    fn write_contract(env: &Env, contract_id: u32, record: &EscrowContractData) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), record);
    }

    fn require_valid_contract_id(env: &Env, contract_id: u32) {
        if contract_id == 0 {
            panic_with_error!(env, EscrowError::InvalidContractId);
        }
    }

    fn require_positive_amount(env: &Env, amount: i128) {
        if amount <= 0 {
            panic_with_error!(env, EscrowError::AmountMustBePositive);
        }
    }

    fn is_paused_flag_set(env: &Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn is_emergency_active(env: &Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::EmergencyPaused)
            .unwrap_or(false)
    }

    fn ensure_not_paused(env: &Env) {
        if Self::is_paused_flag_set(env) {
            panic_with_error!(env, EscrowError::ContractPaused);
        }
    }

    fn read_protocol_parameters(env: &Env) -> ProtocolParameters {
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

    fn validate_protocol_parameters(
        env: &Env,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> ProtocolParameters {
        if min_milestone_amount <= 0
            || max_milestones == 0
            || min_reputation_rating <= 0
            || min_reputation_rating > max_reputation_rating
        {
            panic_with_error!(env, EscrowError::InvalidProtocolParameters);
        }

        ProtocolParameters {
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        }
    }
}

#[cfg(test)]
mod test;
