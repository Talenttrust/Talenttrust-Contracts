#![no_std]
#![allow(clippy::derivable_impls)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::useless_vec)]
#![allow(clippy::let_and_return)]
#![allow(clippy::inconsistent_digit_grouping)]
#![allow(clippy::int_plus_one)]
#![allow(clippy::duplicated_attributes)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::module_inception)]
#![allow(clippy::single_match)]
#![allow(clippy::useless_conversion)]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol, Vec};

mod types;
pub use types::{
    ContractStatus, ContractSummary, DataKey, DepositMode, EscrowError, Milestone,
    MilestoneSummary, ReadinessChecklist, CONTRACT_SUMMARY_SCHEMA_VERSION,
};

mod amount_validation;
pub use amount_validation::{safe_add_amounts, safe_subtract_amounts, AmountValidationError};

mod ttl;
pub use ttl::{
    LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS,
    PENDING_MIGRATION_BUMP_THRESHOLD, PENDING_MIGRATION_TTL_LEDGERS,
};

// ─── Bounds constants ─────────────────────────────────────────────────────────

/// Maximum number of milestones allowed per contract.
pub const MAX_MILESTONES: u32 = 10;

/// Hard cap on the total escrow value per contract, in stroops.
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = 10_000_000_000_000;

pub const MAINNET_PROTOCOL_VERSION: u32 = 1u32;
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000_000i128;

// ─── Contract data ────────────────────────────────────────────────────────────

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<i128>,
    pub status: ContractStatus,
    pub total_deposited: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub reputation_issued: bool,
    pub deposit_mode: DepositMode,
}

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneApprovals {
    pub client_approved: bool,
    pub freelancer_approved: bool,
    pub arbiter_approved: bool,
}

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub completed_contracts: u32,
    pub total_rating: i128,
    pub last_rating: i128,
}

impl Default for ReputationRecord {
    fn default() -> Self {
        ReputationRecord {
            completed_contracts: 0,
            total_rating: 0,
            last_rating: 0,
        }
    }
}

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainnetReadinessInfo {
    pub initialized: bool,
    pub governed_params_set: bool,
    pub emergency_controls_enabled: bool,
    pub caps_set: bool,
    pub protocol_version: u32,
    pub max_escrow_total_stroops: i128,
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    // ─── Guard ───────────────────────────────────────────────────────────────

    /// Panics with `ContractPaused` if the contract is paused or in emergency.
    fn require_not_paused(env: &Env) {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::ContractPaused);
        }
    }

    /// Panics with `NotInitialized` if `initialize` has not been called.
    fn require_initialized(env: &Env) {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::NotInitialized);
        }
    }

    /// Panics with `UnauthorizedRole` if `caller` is not the stored admin.
    #[allow(dead_code)] // retained for future admin-gated operations
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        if *caller != admin {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
    }

    // ─── Audit event helper ───────────────────────────────────────────────

    /// Emit a compact audit log event for a state transition.
    /// Tuple: (contract_id, from_status, to_status, actor, timestamp)
    fn emit_audit_event(
        env: &Env,
        contract_id: u32,
        from: ContractStatus,
        to: ContractStatus,
        actor: &Address,
    ) {
        env.events().publish(
            (symbol_short!("audit"), contract_id),
            (
                from as u32,
                to as u32,
                actor.clone(),
                env.ledger().timestamp(),
            ),
        );
    }

    // ─── Accounting invariants ───────────────────────────────────────────

    /// Validate the core accounting invariant:
    ///   total_deposited == released_amount + refunded_amount + available_balance
    /// Panics with `AccountingInvariantViolated` if the invariant is broken.
    fn check_accounting_invariant(env: &Env, contract: &EscrowContractData, _contract_id: u32) {
        let available_balance =
            contract.total_deposited - contract.released_amount - contract.refunded_amount;
        if available_balance < 0 {
            env.panic_with_error(EscrowError::AccountingInvariantViolated);
        }
        if contract.total_deposited
            != contract.released_amount + contract.refunded_amount + available_balance
        {
            env.panic_with_error(EscrowError::AccountingInvariantViolated);
        }
    }

    // ─── Initialization ───────────────────────────────────────────────────────

    /// One-time initialization. Sets the admin address.
    pub fn initialize(env: Env, admin: Address) -> bool {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Initialized, &true);

        // Update readiness checklist
        let mut checklist = Self::load_checklist(&env);
        checklist.initialized = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (symbol_short!("init"), Symbol::new(&env, "admin_set")),
            (admin, env.ledger().timestamp()),
        );
        true
    }

    /// Returns the current admin address, or `None` if not initialized.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    // ─── Pause controls ───────────────────────────────────────────────────────

    /// Pause all mutating operations. Admin only.
    pub fn pause(env: Env) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &true);
        env.events().publish(
            (symbol_short!("paused"), env.ledger().timestamp()),
            (admin,),
        );
        true
    }

    /// Resume operations. Admin only. Fails if emergency is active.
    pub fn unpause(env: Env) -> bool {
        Self::require_initialized(&env);
        // Cannot unpause while emergency is active
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Emergency)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::EmergencyActive);
        }
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &false);
        env.events().publish(
            (symbol_short!("unpaused"), env.ledger().timestamp()),
            (admin,),
        );
        true
    }

    /// Returns `true` if the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }

    // ─── Emergency controls ───────────────────────────────────────────────────

    /// Activate emergency pause. Sets both `Paused` and `Emergency` flags. Admin only.
    pub fn activate_emergency_pause(env: Env) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &true);
        env.storage().persistent().set(&DataKey::Emergency, &true);

        // Update readiness checklist
        let mut checklist = Self::load_checklist(&env);
        checklist.emergency_controls_enabled = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (symbol_short!("emergency"), Symbol::new(&env, "activated")),
            (admin, env.ledger().timestamp()),
        );
        true
    }

    /// Resolve emergency and clear both flags. Admin only.
    pub fn resolve_emergency(env: Env) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Emergency, &false);
        env.storage().persistent().set(&DataKey::Paused, &false);

        // Update readiness checklist
        let mut checklist = Self::load_checklist(&env);
        checklist.emergency_controls_enabled = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (symbol_short!("emergency"), Symbol::new(&env, "resolved")),
            (admin, env.ledger().timestamp()),
        );
        true
    }

    /// Returns `true` if the contract is in emergency mode.
    pub fn is_emergency(env: Env) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::Emergency)
            .unwrap_or(false)
    }

    // ─── Mainnet readiness ────────────────────────────────────────────────────

    pub fn get_mainnet_readiness_info(env: Env) -> MainnetReadinessInfo {
        let checklist = Self::load_checklist(&env);
        MainnetReadinessInfo {
            initialized: checklist.initialized,
            governed_params_set: checklist.governed_params_set,
            emergency_controls_enabled: checklist.emergency_controls_enabled,
            caps_set: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0,
            protocol_version: MAINNET_PROTOCOL_VERSION,
            max_escrow_total_stroops: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
        }
    }

    fn load_checklist(env: &Env) -> ReadinessChecklist {
        env.storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default()
    }

    // ─── Contract lifecycle ───────────────────────────────────────────────────

    /// Create a new escrow contract. Blocked when paused.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
        deposit_mode: DepositMode,
    ) -> u32 {
        Self::require_not_paused(&env);
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

        let mut total: i128 = 0;
        for i in 0..milestone_amounts.len() {
            let amt = milestone_amounts.get(i).unwrap();
            if amt <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
            total = safe_add_amounts(total, amt)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        }
        if total > MAX_TOTAL_ESCROW_STROOPS {
            env.panic_with_error(EscrowError::InvalidMilestoneAmount);
        }

        let id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(id + 1));

        let data = EscrowContractData {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            milestones: milestone_amounts,
            status: ContractStatus::Created,
            total_deposited: 0,
            released_amount: 0,
            refunded_amount: 0,
            reputation_issued: false,
            deposit_mode,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &data);

        // Audit: contract created
        Self::emit_audit_event(
            &env,
            id,
            ContractStatus::Created,
            ContractStatus::Created,
            &client,
        );

        env.events().publish(
            (symbol_short!("created"), id),
            (client, freelancer, env.ledger().timestamp()),
        );
        id
    }

    /// Deposit funds into an escrow contract. Blocked when paused.
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        Self::require_not_paused(&env);

        if amount <= 0 {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        let key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        let old_status = contract.status;
        let prior_deposited = contract.total_deposited;

        // Sum of all milestone amounts is the total required contract value.
        let mut total_milestones: i128 = 0;
        for m in contract.milestones.iter() {
            total_milestones = safe_add_amounts(total_milestones, m)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        }

        if contract.deposit_mode == DepositMode::ExactTotal {
            if amount != total_milestones || prior_deposited > 0 {
                env.panic_with_error(EscrowError::ExactDepositRequired);
            }
            contract.total_deposited = amount;
            contract.status = ContractStatus::Funded;
        } else {
            let new_total = safe_add_amounts(prior_deposited, amount)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
            if new_total > total_milestones {
                env.panic_with_error(EscrowError::DepositWouldExceedTotal);
            }
            contract.total_deposited = new_total;
            if new_total == total_milestones {
                contract.status = ContractStatus::Funded;
            } else {
                contract.status = ContractStatus::PartiallyFunded;
            }
        }

        // Enforce accounting invariant
        Self::check_accounting_invariant(&env, &contract, contract_id);

        env.storage().persistent().set(&key, &contract);

        // Audit: deposit with state transition
        if old_status != contract.status {
            Self::emit_audit_event(
                &env,
                contract_id,
                old_status,
                contract.status,
                &contract.client,
            );
        }

        true
    }

    /// Release a milestone to the freelancer. Blocked when paused.
    pub fn release_milestone(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        Self::require_not_paused(&env);

        let key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if milestone_index >= contract.milestones.len() {
            env.panic_with_error(EscrowError::InvalidMilestone);
        }

        let released_key = DataKey::MilestoneReleased(contract_id, milestone_index);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&released_key)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::AlreadyReleased);
        }

        let milestone_amount = contract.milestones.get(milestone_index).unwrap();
        let available =
            contract.total_deposited - contract.released_amount - contract.refunded_amount;
        if available < milestone_amount {
            env.panic_with_error(EscrowError::InsufficientFunds);
        }

        env.storage().persistent().set(&released_key, &true);
        contract.released_amount = safe_add_amounts(contract.released_amount, milestone_amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        let old_status = contract.status;

        // Check if all milestones released → Completed
        let all_released = (0..contract.milestones.len()).all(|i| {
            env.storage()
                .persistent()
                .get::<_, bool>(&DataKey::MilestoneReleased(contract_id, i))
                .unwrap_or(false)
        });
        if all_released {
            contract.status = ContractStatus::Completed;
            // Increment pending reputation credits
            let credits_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
            let credits: u32 = env.storage().persistent().get(&credits_key).unwrap_or(0);
            env.storage().persistent().set(&credits_key, &(credits + 1));
        }

        // Enforce accounting invariant
        Self::check_accounting_invariant(&env, &contract, contract_id);

        env.storage().persistent().set(&key, &contract);

        // Audit: release with state transition
        if old_status != contract.status {
            Self::emit_audit_event(
                &env,
                contract_id,
                old_status,
                contract.status,
                &contract.freelancer,
            );
        }

        env.events().publish(
            (symbol_short!("released"), contract_id, milestone_index),
            (milestone_amount, env.ledger().timestamp()),
        );
        true
    }

    /// Issue reputation for a completed contract. Blocked when paused.
    pub fn issue_reputation(
        env: Env,
        contract_id: u32,
        caller: Address,
        freelancer: Address,
        rating: i128,
    ) -> bool {
        Self::require_not_paused(&env);
        caller.require_auth();

        let key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&key)
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

        let rep_key = DataKey::ReputationIssued(contract_id);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&rep_key)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::ReputationAlreadyIssued);
        }
        env.storage().persistent().set(&rep_key, &true);

        let old_status = contract.status;

        contract.reputation_issued = true;
        env.storage().persistent().set(&key, &contract);

        // Audit: reputation issued
        Self::emit_audit_event(&env, contract_id, old_status, contract.status, &caller);

        let reputation_key = DataKey::Reputation(freelancer.clone());
        let mut record: ReputationRecord = env
            .storage()
            .persistent()
            .get(&reputation_key)
            .unwrap_or_default();
        record.total_rating += rating;
        record.completed_contracts += 1;
        record.last_rating = rating;
        env.storage().persistent().set(&reputation_key, &record);

        let credits_key = DataKey::PendingReputationCredits(freelancer.clone());
        let credits: u32 = env.storage().persistent().get(&credits_key).unwrap_or(0);
        if credits > 0 {
            env.storage().persistent().set(&credits_key, &(credits - 1));
        }

        env.events().publish(
            (symbol_short!("rep_issd"), contract_id),
            (freelancer, rating, env.ledger().timestamp()),
        );
        true
    }

    /// Cancel an escrow contract. Blocked when paused.
    pub fn cancel_contract(env: Env, contract_id: u32, caller: Address) -> bool {
        Self::require_not_paused(&env);
        caller.require_auth();

        let key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if contract.status == ContractStatus::Cancelled {
            env.panic_with_error(EscrowError::AlreadyCancelled);
        }
        if contract.status == ContractStatus::Completed {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        if !is_client && !is_freelancer {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        let old_status = contract.status;
        contract.status = ContractStatus::Cancelled;

        // Enforce accounting invariant
        Self::check_accounting_invariant(&env, &contract, contract_id);

        env.storage().persistent().set(&key, &contract);

        // Audit: cancel with state transition
        Self::emit_audit_event(&env, contract_id, old_status, contract.status, &caller);

        env.events().publish(
            (symbol_short!("cancelled"), contract_id),
            (caller, env.ledger().timestamp()),
        );
        true
    }

    // ─── Read-only queries (not blocked by pause) ─────────────────────────────

    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get::<_, EscrowContractData>(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
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
}

#[cfg(test)]
mod proptest;
#[cfg(test)]
mod simple_amount_test;

#[cfg(test)]
mod test;
