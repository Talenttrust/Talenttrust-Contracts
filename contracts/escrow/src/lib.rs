//! # TalentTrust Escrow Contract
//!
//! A Soroban smart contract that manages milestone-based escrow agreements
//! between a client and a freelancer, with optional on-chain reputation
//! credentialing and governance-managed protocol parameters.
//!
//! ## Event model
//!
//! Every state-changing operation publishes a structured event **only on success**.
//!
//! | Topics                  | Data payload                                                   |
//! |-------------------------|----------------------------------------------------------------|
//! | `("pause","init")`      | `admin: Address`                                               |
//! | `("pause","pause")`     | `admin: Address`                                               |
//! | `("pause","unpause")`   | `admin: Address`                                               |
//! | `("pause","emerg")`     | `admin: Address`                                               |
//! | `("pause","resolv")`    | `admin: Address`                                               |
//! | `("gov","init")`        | `admin: Address`                                               |
//! | `("gov","params")`      | `(min_m, max_m, min_r, max_r)`                                 |
//! | `("gov","propose")`     | `new_admin: Address`                                           |
//! | `("gov","accept")`      | `new_admin: Address`                                           |
//! | `("escrow","create")`   | `(id: u32, client: Address, freelancer: Address, total: i128)` |
//! | `("escrow","deposit")`  | `(id: u32, amount: i128, funded: i128)`                        |
//! | `("escrow","release")`  | `(id: u32, milestone_id: u32, amount: i128)`                   |
//! | `("escrow","complete")` | `id: u32`  *(only when last milestone released)*               |
//! | `("escrow","rep")`      | `(id: u32, freelancer: Address, rating: i128)`                 |

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec,
};

// ─── Constants ───────────────────────────────────────────────────────────────

const DEFAULT_MIN_MILESTONE_AMOUNT: i128 = 1;
const DEFAULT_MAX_MILESTONES: u32 = 16;
const DEFAULT_MIN_REPUTATION_RATING: i128 = 1;
const DEFAULT_MAX_REPUTATION_RATING: i128 = 5;

/// Reported deployment version for operators (`major * 1_000_000 + minor * 1_000 + patch`).
pub const MAINNET_PROTOCOL_VERSION: u32 = 1_000_000;

/// Hard ceiling on the sum of milestone amounts per escrow (stroops). Not governed; change only via wasm upgrade.
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000;

// ─── Storage keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    // Instance storage (pause controls)
    Admin,
    Paused,
    EmergencyPaused,
    // Persistent storage (escrow core)
    NextContractId,
    Contract(u32),
    PendingReputationCredits(Address),
    Reputation(Address),
    // Persistent storage (governance)
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
}

// ─── Domain types ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
}

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
    pub milestone_count: u32,
    pub released_milestones: u32,
    pub reputation_issued: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub completed_contracts: u32,
    pub ratings_count: u32,
    pub total_rating: i128,
    pub last_rating: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolParameters {
    pub min_milestone_amount: i128,
    pub max_milestones: u32,
    pub min_reputation_rating: i128,
    pub max_reputation_rating: i128,
}

/// On-chain summary for mainnet deployment review and monitoring integration.
/// Fields mirror [`ProtocolParameters`] without nesting (Soroban SDK nesting limits).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainnetReadinessInfo {
    pub protocol_version: u32,
    pub max_escrow_total_stroops: i128,
    pub min_milestone_amount: i128,
    pub max_milestones: u32,
    pub min_reputation_rating: i128,
    pub max_reputation_rating: i128,
}

// ─── Error codes ─────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    ContractNotFound = 1,
    MilestoneNotFound = 2,
    InvalidAmount = 3,
    InvalidRating = 4,
    EmptyMilestones = 5,
    InvalidParticipants = 6,
    FundingExceedsRequired = 7,
    InvalidState = 8,
    InsufficientEscrowBalance = 9,
    MilestoneAlreadyReleased = 10,
    ReputationAlreadyIssued = 11,
}

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct Escrow;

// Internal helpers
impl Escrow {
    fn pause_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("Pause controls are not initialized"))
    }

    fn require_pause_admin(env: &Env) {
        Self::pause_admin(env).require_auth();
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

    fn require_not_paused(env: &Env) {
        if Self::is_paused_internal(env) {
            panic!("Contract is paused");
        }
    }

    fn next_contract_id(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1_u32)
    }

    fn load_contract(env: &Env, contract_id: u32) -> Result<EscrowContractData, EscrowError> {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .ok_or(EscrowError::ContractNotFound)
    }

    fn save_contract(env: &Env, contract_id: u32, contract: &EscrowContractData) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), contract);
    }

    fn add_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let key = DataKey::PendingReputationCredits(freelancer.clone());
        let current: u32 = env.storage().persistent().get(&key).unwrap_or(0);
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
}

// ─── Public contract interface ───────────────────────────────────────────────

#[contractimpl]
impl Escrow {
    // ────────────────────────────────────────────────────────────────────────
    // Utility
    // ────────────────────────────────────────────────────────────────────────

    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    // ────────────────────────────────────────────────────────────────────────
    // Pause controls
    // ────────────────────────────────────────────────────────────────────────

    /// Initialises pause controls.  Emits `("pause","init")`.
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
        env.events()
            .publish((symbol_short!("pause"), symbol_short!("init")), admin);
        true
    }

    pub fn get_admin(env: Env) -> Address {
        Self::pause_admin(&env)
    }

    /// Pauses the contract.  Emits `("pause","pause")`.
    pub fn pause(env: Env) -> bool {
        Self::require_pause_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
        let admin = Self::pause_admin(&env);
        env.events()
            .publish((symbol_short!("pause"), symbol_short!("pause")), admin);
        true
    }

    /// Unpauses the contract.  Emits `("pause","unpause")`.
    pub fn unpause(env: Env) -> bool {
        Self::require_pause_admin(&env);
        if Self::is_emergency_internal(&env) {
            panic!("Emergency pause active");
        }
        if !Self::is_paused_internal(&env) {
            panic!("Contract is not paused");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        let admin = Self::pause_admin(&env);
        env.events()
            .publish((symbol_short!("pause"), symbol_short!("unpause")), admin);
        true
    }

    /// Activates emergency mode.  Emits `("pause","emerg")`.
    pub fn activate_emergency_pause(env: Env) -> bool {
        Self::require_pause_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &true);
        env.storage().instance().set(&DataKey::Paused, &true);
        let admin = Self::pause_admin(&env);
        env.events()
            .publish((symbol_short!("pause"), symbol_short!("emerg")), admin);
        true
    }

    /// Resolves emergency mode.  Emits `("pause","resolv")`.
    pub fn resolve_emergency(env: Env) -> bool {
        Self::require_pause_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        env.storage().instance().set(&DataKey::Paused, &false);
        let admin = Self::pause_admin(&env);
        env.events()
            .publish((symbol_short!("pause"), symbol_short!("resolv")), admin);
        true
    }

    pub fn is_paused(env: Env) -> bool {
        Self::is_paused_internal(&env)
    }

    pub fn is_emergency(env: Env) -> bool {
        Self::is_emergency_internal(&env)
    }

    // ────────────────────────────────────────────────────────────────────────
    // Protocol governance
    // ────────────────────────────────────────────────────────────────────────

    /// Bootstraps governance.  Emits `("gov","init")`.
    pub fn initialize_protocol_governance(
        env: Env,
        admin: Address,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        if env.storage().persistent().has(&DataKey::GovernanceAdmin) {
            panic!("governance already initialized");
        }
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
        env.events()
            .publish((symbol_short!("gov"), symbol_short!("init")), admin);
        true
    }

    /// Updates protocol parameters.  Emits `("gov","params")`.
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
        env.events().publish(
            (symbol_short!("gov"), symbol_short!("params")),
            (
                params.min_milestone_amount,
                params.max_milestones,
                params.min_reputation_rating,
                params.max_reputation_rating,
            ),
        );
        true
    }

    /// Proposes a successor governance admin.  Emits `("gov","propose")`.
    pub fn propose_governance_admin(env: Env, new_admin: Address) -> bool {
        let current = Self::governance_admin(&env);
        current.require_auth();
        if new_admin == current {
            panic!("cannot propose current admin as successor");
        }
        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &new_admin);
        env.events()
            .publish((symbol_short!("gov"), symbol_short!("propose")), new_admin);
        true
    }

    /// Completes the governance-admin transfer.  Emits `("gov","accept")`.
    pub fn accept_governance_admin(env: Env) -> bool {
        let new_admin = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::PendingGovernanceAdmin)
            .unwrap_or_else(|| panic!("no pending admin transfer"));
        new_admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &new_admin);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingGovernanceAdmin);
        env.events()
            .publish((symbol_short!("gov"), symbol_short!("accept")), new_admin);
        true
    }

    pub fn get_protocol_parameters(env: Env) -> ProtocolParameters {
        Self::protocol_parameters(&env)
    }

    pub fn get_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::GovernanceAdmin)
    }

    pub fn get_pending_governance_admin(env: Env) -> Option<Address> {
        Self::pending_governance_admin(&env)
    }

    // ────────────────────────────────────────────────────────────────────────
    // Escrow core
    // ────────────────────────────────────────────────────────────────────────

    /// Creates an escrow agreement.  Emits `("escrow","create")`.
    ///
    /// # Errors
    /// [`EscrowError::InvalidParticipants`] | [`EscrowError::EmptyMilestones`] |
    /// [`EscrowError::InvalidAmount`]
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
    ) -> Result<u32, EscrowError> {
        Self::require_not_paused(&env);

        if client == freelancer {
            return Err(EscrowError::InvalidParticipants);
        }
        if milestone_amounts.is_empty() {
            return Err(EscrowError::EmptyMilestones);
        }

        let params = Self::protocol_parameters(&env);

        let mut total_amount: i128 = 0;
        let mut milestones: Vec<Milestone> = Vec::new(&env);
        let milestone_count = milestone_amounts.len();

        for i in 0..milestone_count {
            let amount = milestone_amounts
                .get(i)
                .unwrap_or_else(|| panic!("missing milestone"));
            if amount <= 0 || amount < params.min_milestone_amount {
                return Err(EscrowError::InvalidAmount);
            }
            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic!("overflow"));
            milestones.push_back(Milestone {
                amount,
                released: false,
            });
        }

        if total_amount > MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS {
            panic!("total escrow exceeds mainnet hard cap");
        }

        client.require_auth();

        let contract_id = Self::next_contract_id(&env);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(contract_id + 1));

        let contract = EscrowContractData {
            client: client.clone(),
            freelancer: freelancer.clone(),
            milestones,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            status: ContractStatus::Created,
            milestone_count,
            released_milestones: 0,
            reputation_issued: false,
        };
        Self::save_contract(&env, contract_id, &contract);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("create")),
            (contract_id, client, freelancer, total_amount),
        );

        Ok(contract_id)
    }

    /// Records a deposit.  Emits `("escrow","deposit")`.
    ///
    /// # Errors
    /// [`EscrowError::InvalidAmount`] | [`EscrowError::ContractNotFound`] |
    /// [`EscrowError::InvalidState`] | [`EscrowError::FundingExceedsRequired`]
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> Result<bool, EscrowError> {
        Self::require_not_paused(&env);

        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        let mut contract = Self::load_contract(&env, contract_id)?;

        if contract.status == ContractStatus::Completed {
            return Err(EscrowError::InvalidState);
        }

        let new_funded = contract
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic!("overflow"));
        if new_funded > contract.total_amount {
            return Err(EscrowError::FundingExceedsRequired);
        }

        contract.funded_amount = new_funded;
        if contract.status == ContractStatus::Created {
            contract.status = ContractStatus::Funded;
        }
        Self::save_contract(&env, contract_id, &contract);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("deposit")),
            (contract_id, amount, contract.funded_amount),
        );

        Ok(true)
    }

    /// Releases milestone payment.  Emits `("escrow","release")` and, on last
    /// milestone, additionally emits `("escrow","complete")`.
    ///
    /// # Errors
    /// [`EscrowError::MilestoneNotFound`] | [`EscrowError::ContractNotFound`] |
    /// [`EscrowError::InvalidState`] | [`EscrowError::MilestoneAlreadyReleased`] |
    /// [`EscrowError::InsufficientEscrowBalance`]
    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        milestone_id: u32,
    ) -> Result<bool, EscrowError> {
        Self::require_not_paused(&env);

        // Sentinel guard before touching storage.
        if milestone_id == u32::MAX {
            return Err(EscrowError::MilestoneNotFound);
        }

        let mut contract = Self::load_contract(&env, contract_id)?;

        if contract.status != ContractStatus::Funded {
            return Err(EscrowError::InvalidState);
        }

        if milestone_id >= contract.milestone_count {
            return Err(EscrowError::MilestoneNotFound);
        }

        let milestone = contract
            .milestones
            .get(milestone_id)
            .unwrap_or_else(|| panic!("missing milestone"));

        if milestone.released {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }

        let available = contract
            .funded_amount
            .checked_sub(contract.released_amount)
            .unwrap_or(0);
        if available < milestone.amount {
            return Err(EscrowError::InsufficientEscrowBalance);
        }

        let released_amount = milestone.amount;

        contract.milestones.set(
            milestone_id,
            Milestone {
                amount: milestone.amount,
                released: true,
            },
        );
        contract.released_amount = contract
            .released_amount
            .checked_add(released_amount)
            .unwrap_or_else(|| panic!("overflow"));
        contract.released_milestones += 1;

        let all_released = contract.released_milestones == contract.milestone_count;
        if all_released {
            contract.status = ContractStatus::Completed;
            Self::add_pending_reputation_credit(&env, &contract.freelancer);
        }

        Self::save_contract(&env, contract_id, &contract);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("release")),
            (contract_id, milestone_id, released_amount),
        );

        if all_released {
            env.events().publish(
                (symbol_short!("escrow"), symbol_short!("complete")),
                contract_id,
            );
        }

        Ok(true)
    }

    /// Issues a reputation credential.  Emits `("escrow","rep")`.
    ///
    /// # Errors
    /// [`EscrowError::InvalidRating`] | [`EscrowError::ContractNotFound`] |
    /// [`EscrowError::InvalidState`] | [`EscrowError::ReputationAlreadyIssued`]
    pub fn issue_reputation(env: Env, contract_id: u32, rating: i128) -> Result<bool, EscrowError> {
        Self::require_not_paused(&env);

        let params = Self::protocol_parameters(&env);
        if rating < params.min_reputation_rating || rating > params.max_reputation_rating {
            return Err(EscrowError::InvalidRating);
        }

        let mut contract = Self::load_contract(&env, contract_id)?;

        if contract.status != ContractStatus::Completed {
            return Err(EscrowError::InvalidState);
        }

        if contract.reputation_issued {
            return Err(EscrowError::ReputationAlreadyIssued);
        }

        contract.reputation_issued = true;

        // Decrement pending credit counter.
        let credit_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
        let credits: u32 = env.storage().persistent().get(&credit_key).unwrap_or(0);
        if credits > 0 {
            env.storage().persistent().set(&credit_key, &(credits - 1));
        }

        // Update aggregated reputation.
        let rep_key = DataKey::Reputation(contract.freelancer.clone());
        let mut record = env
            .storage()
            .persistent()
            .get::<_, ReputationRecord>(&rep_key)
            .unwrap_or(ReputationRecord {
                completed_contracts: 0,
                ratings_count: 0,
                total_rating: 0,
                last_rating: 0,
            });
        record.completed_contracts += 1;
        record.ratings_count += 1;
        record.total_rating = record
            .total_rating
            .checked_add(rating)
            .unwrap_or_else(|| panic!("overflow"));
        record.last_rating = rating;
        env.storage().persistent().set(&rep_key, &record);

        Self::save_contract(&env, contract_id, &contract);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("rep")),
            (contract_id, contract.freelancer, rating),
        );

        Ok(true)
    }

    // ────────────────────────────────────────────────────────────────────────
    // Read-only accessors
    // ────────────────────────────────────────────────────────────────────────

    pub fn get_contract(env: Env, contract_id: u32) -> Result<EscrowContractData, EscrowError> {
        Self::load_contract(&env, contract_id)
    }

    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    /// Aggregates immutable caps, protocol version, and current governed parameters for mainnet readiness review.
    pub fn get_mainnet_readiness_info(env: Env) -> MainnetReadinessInfo {
        let p = Self::protocol_parameters(&env);
        MainnetReadinessInfo {
            protocol_version: MAINNET_PROTOCOL_VERSION,
            max_escrow_total_stroops: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
            min_milestone_amount: p.min_milestone_amount,
            max_milestones: p.max_milestones,
            min_reputation_rating: p.min_reputation_rating,
            max_reputation_rating: p.max_reputation_rating,
        }
    }
}

#[cfg(test)]
mod test;
