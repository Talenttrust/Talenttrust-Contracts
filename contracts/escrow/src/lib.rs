#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN,
    Env, Symbol, Vec,
};

mod ttl;
mod types;
mod amount_validation;

pub use ttl::{
    LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS,
    PENDING_MIGRATION_BUMP_THRESHOLD, PENDING_MIGRATION_TTL_LEDGERS,
};
pub use amount_validation::{
    validate_single_amount, validate_milestone_amounts, validate_deposit_amount,
    validate_contract_total, safe_add_amounts, safe_subtract_amounts, AmountValidationError,
};
pub use crate::types::{
    CONTRACT_SUMMARY_SCHEMA_VERSION, ContractSummary, MilestoneSummary, ReadinessChecklist,
    ContractStatus,
};

// ─── Bounds constants ─────────────────────────────────────────────────────────
/// Maximum number of milestones allowed per contract.
pub const MAX_MILESTONES: u32 = 10;

/// Hard cap on the total escrow value per contract, in stroops (7 decimal places).
/// Equals 1 000 000 tokens.
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = 10_000_000_000_000; // 1 M tokens × 10^7 = 10^13

pub const MAINNET_PROTOCOL_VERSION: u32 = 1u32;
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000_000i128;

#[contract]
pub struct Escrow;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowBounds {
    pub max_milestones: u32,
    pub max_total_escrow_stroops: i128,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    UnauthorizedRole = 6,
    InvalidStatusTransition = 7,
    AlreadyCancelled = 8,
    ContractNotFound = 9,
    MilestonesAlreadyReleased = 10,
    TooManyMilestones = 11,
    EmptyRefundRequest = 12,
    DuplicateMilestoneInRefund = 13,
    FreelancerMismatch = 14,
    NotCompleted = 15,
    InvalidRating = 16,
    ReputationAlreadyIssued = 17,
    // Pause / emergency controls
    AlreadyInitialized = 18,
    NotInitialized = 19,
    ContractPaused = 20,
    EmergencyActive = 21,
    // Amount validation errors (1000+ to avoid conflicts)
    NonPositiveAmount = 1000,
    AmountExceedsMaximum = 1001,
    PotentialOverflow = 1002,
    InvalidStroopPrecision = 1003,
    ExceedsContractMaximum = 1004,
}

/// Per-contract storage record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    /// Milestone amounts (in stroops).  Index matches milestone index.
    pub milestones: Vec<i128>,
    pub status: ContractStatus,
    /// Cumulative amount deposited into escrow.
    pub total_deposited: i128,
    /// Cumulative amount released to the freelancer.
    pub released_amount: i128,
    /// Cumulative amount refunded to the client.
    pub refunded_amount: i128,
    /// Whether reputation has been issued for this contract.
    pub reputation_issued: bool,
}

/// Reputation record for a freelancer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub total_rating: i128,
    pub ratings_count: u32,
    pub last_rating: i128,
    pub completed_contracts: u32,
}

impl Default for ReputationRecord {
    fn default() -> Self {
        ReputationRecord {
            total_rating: 0,
            last_rating: 0,
            ratings_count: 0,
            completed_contracts: 0,
        }
    }
}

/// Metadata stored when a dispute is raised.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeMetadata {
    pub reason_hash: BytesN<32>,
    pub raised_at: u64,
    pub raised_by: Address,
}

/// Arbiter decision when resolving a dispute.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeResolution {
    Release = 0,
    Refund = 1,
    Cancel = 2,
}

pub type ContractData = EscrowContractData;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingApproval {
    pub approver: Address,
    pub contract_id: u32,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingMigration {
    pub proposer: Address,
    pub new_wasm_hash: BytesN<32>,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingClientMigration {
    pub current_client: Address,
    pub proposed_client: Address,
    pub proposed_client_confirmed: bool,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

/// Mainnet readiness info returned by `get_mainnet_readiness_info`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainnetReadinessInfo {
    pub initialized: bool,
    pub governed_params_set: bool,
    pub emergency_controls_enabled: bool,
    pub caps_set: bool,
    pub protocol_version: u32,
    pub max_escrow_total_stroops: i128,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Contract(u32),
    ContractCount,
    MilestoneReleased(u32, u32),
    RefundableBalance(u32),
    MilestoneApprovalTime(u32, u32),
    ReadinessChecklist,
    PendingClientMigration(u32),
    Reputation(Address),
    PendingReputationCredits(Address),
    ReputationIssued(u32),
    // Pause / emergency controls
    Admin,
    Paused,
    Emergency,
}

// ─── Guard function ───────────────────────────────────────────────────────────

/// Panics with `ContractPaused` if the contract is paused or in emergency mode.
/// Call this at the top of every mutating entrypoint.
fn require_not_paused(env: &Env) {
    let paused: bool = env
        .storage()
        .persistent()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        env.panic_with_error(EscrowError::ContractPaused);
    }
}

#[contractimpl]
impl Escrow {
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    // ─── Admin / pause controls ───────────────────────────────────────────────

    /// One-time initialization: sets the admin address.
    /// Returns `true` on success; panics with `AlreadyInitialized` if called again.
    pub fn initialize(env: Env, admin: Address) -> bool {
        if env.storage().persistent().has(&DataKey::Admin) {
            env.panic_with_error(EscrowError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);

        // Update readiness checklist
        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or(ReadinessChecklist {
                initialized: false,
                governed_params_set: false,
                emergency_controls_enabled: false,
            });
        checklist.initialized = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (Symbol::new(&env, "initialized"),),
            (admin, env.ledger().timestamp()),
        );
        true
    }

    /// Pause all mutating operations. Requires admin auth and prior initialization.
    pub fn pause(env: Env) -> bool {
        let admin = Self::require_admin(&env);
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &true);
        env.events().publish(
            (Symbol::new(&env, "paused"),),
            (admin, env.ledger().timestamp()),
        );
        true
    }

    /// Unpause operations. Fails if emergency mode is active.
    pub fn unpause(env: Env) -> bool {
        let admin = Self::require_admin(&env);
        admin.require_auth();
        let emergency: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Emergency)
            .unwrap_or(false);
        if emergency {
            env.panic_with_error(EscrowError::EmergencyActive);
        }
        env.storage().persistent().set(&DataKey::Paused, &false);
        env.events().publish(
            (Symbol::new(&env, "unpaused"),),
            (admin, env.ledger().timestamp()),
        );
        true
    }

    /// Activate emergency pause: sets both Paused and Emergency flags.
    /// Also marks `emergency_controls_enabled` in the readiness checklist.
    /// If initialized, requires admin auth; otherwise operates in bootstrap mode.
    pub fn activate_emergency_pause(env: Env) -> bool {
        // If admin is set, require their auth; otherwise allow bootstrap (for readiness tracking)
        if let Some(admin) = env.storage().persistent().get::<_, Address>(&DataKey::Admin) {
            admin.require_auth();
        }
        env.storage().persistent().set(&DataKey::Paused, &true);
        env.storage().persistent().set(&DataKey::Emergency, &true);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or(ReadinessChecklist {
                initialized: false,
                governed_params_set: false,
                emergency_controls_enabled: false,
            });
        checklist.emergency_controls_enabled = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (Symbol::new(&env, "emergency_pause"),),
            (env.ledger().timestamp(),),
        );
        true
    }

    /// Resolve emergency: clears both Emergency and Paused flags.
    /// Also marks `emergency_controls_enabled` in the readiness checklist.
    /// If initialized, requires admin auth; otherwise operates in bootstrap mode.
    pub fn resolve_emergency(env: Env) -> bool {
        // If admin is set, require their auth; otherwise allow bootstrap (for readiness tracking)
        if let Some(admin) = env.storage().persistent().get::<_, Address>(&DataKey::Admin) {
            admin.require_auth();
        }
        env.storage().persistent().set(&DataKey::Emergency, &false);
        env.storage().persistent().set(&DataKey::Paused, &false);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or(ReadinessChecklist {
                initialized: false,
                governed_params_set: false,
                emergency_controls_enabled: false,
            });
        checklist.emergency_controls_enabled = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (Symbol::new(&env, "emergency_resolved"),),
            (env.ledger().timestamp(),),
        );
        true
    }

    /// Returns `true` if the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Returns `true` if the contract is in emergency mode.
    pub fn is_emergency(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Emergency)
            .unwrap_or(false)
    }

    /// Returns the admin address, or `None` if not initialized.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    /// Initialize protocol governance parameters. Sets `governed_params_set` in the checklist.
    pub fn initialize_protocol_governance(
        env: Env,
        admin: Address,
        _min_milestone_amount: i128,
        _max_milestones: u32,
        _min_reputation_rating: i128,
        _max_reputation_rating: i128,
    ) -> bool {
        admin.require_auth();
        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or(ReadinessChecklist {
                initialized: false,
                governed_params_set: false,
                emergency_controls_enabled: false,
            });
        checklist.governed_params_set = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);
        true
    }

    /// Update protocol parameters. Sets `governed_params_set` in the checklist.
    pub fn update_protocol_parameters(
        env: Env,
        _min_milestone_amount: i128,
        _max_milestones: u32,
        _min_reputation_rating: i128,
        _max_reputation_rating: i128,
    ) -> bool {
        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or(ReadinessChecklist {
                initialized: false,
                governed_params_set: false,
                emergency_controls_enabled: false,
            });
        checklist.governed_params_set = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);
        true
    }

    /// Returns mainnet readiness info (read-only, no auth required).
    pub fn get_mainnet_readiness_info(env: Env) -> MainnetReadinessInfo {
        let checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or(ReadinessChecklist {
                initialized: false,
                governed_params_set: false,
                emergency_controls_enabled: false,
            });
        MainnetReadinessInfo {
            initialized: checklist.initialized,
            governed_params_set: checklist.governed_params_set,
            emergency_controls_enabled: checklist.emergency_controls_enabled,
            caps_set: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0,
            protocol_version: MAINNET_PROTOCOL_VERSION,
            max_escrow_total_stroops: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
        }
    }

    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
    ) -> u32 {
        require_not_paused(&env);
        client.require_auth();

        if client == freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }

        if milestone_amounts.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }
        if milestone_amounts.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        let mut total_amount: i128 = 0;
        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts.get(i).unwrap();
            if amount <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
            total_amount = safe_add_amounts(total_amount, amount)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        }

        if total_amount > MAX_TOTAL_ESCROW_STROOPS {
            env.panic_with_error(EscrowError::InvalidMilestoneAmount);
        }

        let id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ContractCount)
            .unwrap_or(0u32);

        let data = EscrowContractData {
            client,
            freelancer,
            arbiter: None,
            milestones: milestone_amounts,
            status: ContractStatus::Created,
            total_deposited: 0,
            released_amount: 0,
            refunded_amount: 0,
            reputation_issued: false,
        };

        env.storage().persistent().set(&DataKey::Contract(id), &data);
        env.storage()
            .persistent()
            .set(&DataKey::ContractCount, &(id + 1));

        id
    }

    /// Deposit funds into the escrow.
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        require_not_paused(&env);

        if amount <= 0 {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        contract.total_deposited = safe_add_amounts(contract.total_deposited, amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        if contract.status == ContractStatus::Created {
            contract.status = ContractStatus::Funded;
        }

        env.storage().persistent().set(&contract_key, &contract);
        true
    }

    /// Release a milestone payment to the freelancer.
    pub fn release_milestone(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        require_not_paused(&env);

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if milestone_index >= contract.milestones.len() {
            env.panic_with_error(EscrowError::InvalidMilestone);
        }

        let milestone_key = DataKey::MilestoneReleased(contract_id, milestone_index);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&milestone_key)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::MilestonesAlreadyReleased);
        }

        env.storage().persistent().set(&milestone_key, &true);

        let amount = contract.milestones.get(milestone_index).unwrap();
        contract.released_amount = safe_add_amounts(contract.released_amount, amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        let all_released = Self::all_milestones_released(&env, contract_id, &contract);
        if all_released && contract.status == ContractStatus::Funded {
            contract.status = ContractStatus::Completed;

            let credits_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
            let credits: u32 = env
                .storage()
                .persistent()
                .get(&credits_key)
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&credits_key, &(credits + 1));
        }

        env.storage().persistent().set(&contract_key, &contract);
        true
    }

    /// Refund one or more unreleased milestones back to the client.
    pub fn refund_milestone(env: Env, contract_id: u32, milestone_ids: Vec<u32>) -> i128 {
        require_not_paused(&env);

        if milestone_ids.is_empty() {
            env.panic_with_error(EscrowError::EmptyRefundRequest);
        }

        let len = milestone_ids.len();
        for i in 0..len {
            for j in (i + 1)..len {
                if milestone_ids.get(i).unwrap() == milestone_ids.get(j).unwrap() {
                    env.panic_with_error(EscrowError::DuplicateMilestoneInRefund);
                }
            }
        }

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        let mut total_refunded: i128 = 0;
        for i in 0..milestone_ids.len() {
            let milestone_index = milestone_ids.get(i).unwrap();
            if milestone_index >= contract.milestones.len() {
                env.panic_with_error(EscrowError::InvalidMilestone);
            }
            let amount = contract.milestones.get(milestone_index).unwrap();
            contract.refunded_amount = safe_add_amounts(contract.refunded_amount, amount)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
            total_refunded = safe_add_amounts(total_refunded, amount)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        }

        env.storage().persistent().set(&contract_key, &contract);
        total_refunded
    }

    /// Cancel an escrow contract.
    pub fn cancel_contract(env: Env, contract_id: u32, caller: Address) -> bool {
        require_not_paused(&env);
        caller.require_auth();

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if contract.status == ContractStatus::Cancelled {
            env.panic_with_error(EscrowError::AlreadyCancelled);
        }
        if contract.status == ContractStatus::Completed {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref().is_some_and(|a| *a == caller);

        match contract.status {
            ContractStatus::Created => {
                if !is_client && !is_freelancer {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ContractStatus::Funded => {
                if is_client {
                    let released =
                        Self::calculate_released_amount(&env, contract_id, &contract);
                    if released > 0 {
                        env.panic_with_error(EscrowError::MilestonesAlreadyReleased);
                    }
                } else if !is_freelancer && !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ContractStatus::Disputed => {
                if !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            _ => {
                env.panic_with_error(EscrowError::InvalidStatusTransition);
            }
        }

        contract.status = ContractStatus::Cancelled;
        env.storage().persistent().set(&contract_key, &contract);

        env.events().publish(
            (Symbol::new(&env, "contract_cancelled"), contract_id),
            (caller, contract.status, env.ledger().timestamp()),
        );

        true
    }

    /// Issue reputation for a completed contract.
    pub fn issue_reputation(
        env: Env,
        contract_id: u32,
        caller: Address,
        freelancer: Address,
        rating: i128,
    ) -> bool {
        require_not_paused(&env);
        caller.require_auth();

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if caller != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        if freelancer != contract.freelancer {
            env.panic_with_error(EscrowError::FreelancerMismatch);
        }
        if contract.status != ContractStatus::Completed {
            env.panic_with_error(EscrowError::NotCompleted);
        }
        if rating < 1 || rating > 5 {
            env.panic_with_error(EscrowError::InvalidRating);
        }

        let reputation_issued_key = DataKey::ReputationIssued(contract_id);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&reputation_issued_key)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::ReputationAlreadyIssued);
        }

        env.storage()
            .persistent()
            .set(&reputation_issued_key, &true);

        contract.reputation_issued = true;
        env.storage().persistent().set(&contract_key, &contract);

        let reputation_key = DataKey::Reputation(freelancer.clone());
        let mut reputation: ReputationRecord = env
            .storage()
            .persistent()
            .get(&reputation_key)
            .unwrap_or_default();
        reputation.total_rating += rating;
        reputation.ratings_count += 1;
        reputation.last_rating = rating;
        reputation.completed_contracts += 1;
        env.storage().persistent().set(&reputation_key, &reputation);

        let credits_key = DataKey::PendingReputationCredits(freelancer.clone());
        let credits: u32 = env
            .storage()
            .persistent()
            .get(&credits_key)
            .unwrap_or(0);
        if credits > 0 {
            env.storage()
                .persistent()
                .set(&credits_key, &(credits - 1));
        }

        env.events().publish(
            (Symbol::new(&env, "reputation_issued"), contract_id),
            (freelancer, rating, env.ledger().timestamp()),
        );

        true
    }

    // ─── Read-only methods ────────────────────────────────────────────────────

    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get::<_, EscrowContractData>(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<i128> {
        let contract = Self::get_contract(env, contract_id);
        contract.milestones
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

    pub fn get_refundable_balance(env: Env, contract_id: u32) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::RefundableBalance(contract_id))
            .unwrap_or(0)
    }

    pub fn get_checklist(env: Env) -> ReadinessChecklist {
        env.storage()
            .persistent()
            .get::<_, ReadinessChecklist>(&DataKey::ReadinessChecklist)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    // ─── Private helpers ──────────────────────────────────────────────────────

    fn require_admin(env: &Env) -> Address {
        env.storage()
            .persistent()
            .get::<_, Address>(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized))
    }

    fn all_milestones_released(
        env: &Env,
        contract_id: u32,
        contract: &EscrowContractData,
    ) -> bool {
        for i in 0..contract.milestones.len() {
            let milestone_key = DataKey::MilestoneReleased(contract_id, i);
            if !env
                .storage()
                .persistent()
                .get::<_, bool>(&milestone_key)
                .unwrap_or(false)
            {
                return false;
            }
        }
        true
    }

    fn calculate_released_amount(
        env: &Env,
        contract_id: u32,
        contract: &EscrowContractData,
    ) -> i128 {
        let mut released = 0i128;
        for i in 0..contract.milestones.len() {
            let key = DataKey::MilestoneReleased(contract_id, i);
            if env
                .storage()
                .persistent()
                .get::<_, bool>(&key)
                .unwrap_or(false)
            {
                let amount = contract.milestones.get(i).unwrap();
                released = safe_add_amounts(released, amount).unwrap_or(released);
            }
        }
        released
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod simple_amount_test;

#[cfg(test)]
mod test_read_notfound;
