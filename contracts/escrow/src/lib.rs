//! TalentTrust escrow contract for milestone-based freelancer payments.
//!
//! The crate root exposes the Soroban contract and still owns several public
//! entrypoints directly: initialization, settlement-token binding, deposits,
//! milestone release/refund/cancel flows, reputation, work evidence, protocol
//! fee withdrawal, and dispute entrypoints. Supporting modules keep reusable
//! validation, storage, governance, and lifecycle helpers close to the paths
//! that use them.
//!
//! ## Escrow source tree map
//!
//! | Source | Responsibility | Storage keys owned or touched |
//! | --- | --- | --- |
//! | `lib.rs` | Contract wrapper plus root entrypoints for setup, custody, money movement, reads, reputation, work evidence, pause/emergency, fee withdrawal, and dispute orchestration. | `DataKey::Initialized`, `Admin`, `SettlementToken`, `Paused`, `Emergency`, `ReadinessChecklist`, `Contract(id)`, `(Contract(id), "milestones")`, `MilestoneApprovals`, `AccumulatedProtocolFees`, `ReputationIssued`, `PendingReputationCredits`, `Reputation`, `ReputationComment` |
//! | `amount_validation` | Stateless validation and checked arithmetic for stroop amounts and milestone totals. | None directly; callers write validated amounts to `Contract(id)` and milestone vectors. |
//! | `approvals` | Temporary milestone release approvals and release-authorization checks. | Temporary `DataKey::MilestoneApprovals(contract_id, milestone_index)`; reads `Contract(id)` and `(Contract(id), "milestones")`. |
//! | `deposit` | Deposit preflight and post-transfer accounting used by `deposit_funds`. | `DataKey::Contract(contract_id)` and `(DataKey::Contract(contract_id), "milestones")`. |
//! | `finalize` | Immutable finalization records, finalization guards, and final contract summaries. | `DataKey::Finalization(contract_id)`; reads `Contract(id)`, `(Contract(id), "milestones")`, `Paused`, and `Emergency`. |
//! | `migration` | Client migration proposals, acceptance checks, cancellation, and pending-migration reads. | Temporary `DataKey::PendingClientMigration(contract_id)`; reads and updates `DataKey::Contract(contract_id)`. |
//! | `ttl` | TTL constants plus helpers for temporary and persistent storage renewal. | Extends caller-provided keys, especially `Contract(id)`, `(Contract(id), "milestones")`, `NextContractId`, participant indexes, approvals, and migrations. |
//! | `types` | Shared Soroban types, error enums, summaries, governance records, dispute records, and the canonical `DataKey` enum. | Declares storage key schema only; does not access storage itself. |
//! | `utils` | Small deterministic helpers shared by entrypoints, currently ledger timestamp access. | None. |
//! | `create_contract` | Contract creation, participant/milestone validation, ID allocation, and creation events. | `DataKey::Contract(id)`, `(DataKey::Contract(id), "milestones")`, `NextContractId`, and `GovernedParameters`. |
//! | `dispute` | Pure dispute payout arithmetic and final-status selection for dispute resolution. | None directly; root dispute entrypoints update `DataKey::Contract(contract_id)`. |
//! | `governance` | Admin-controlled protocol fee, governed parameter, readiness, and admin-rotation entrypoints. | `DataKey::Admin`, `ProtocolFeeBps`, `PendingAdmin`, `GovernedParameters`, and `ReadinessChecklist`. |
//!
//! Generate this map with `cargo doc -p escrow --no-deps` and open
//! `target/doc/escrow/index.html`.
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

mod amount_validation;
mod approvals;
mod deposit;
mod finalize;
mod migration;
mod ttl;
mod types;
mod utils;

use crate::utils::now_seconds;
use soroban_sdk::{
    contract, contracterror, contractimpl, symbol_short, token, Address, Env, String, Symbol, Vec,
};

pub use amount_validation::accumulate_amounts;
pub use amount_validation::safe_add_amounts;
pub use amount_validation::safe_subtract_amounts;
pub use dispute::final_status_after_resolution;
pub use dispute::resolution_payouts;
pub use migration::PendingClientMigration;
pub use ttl::{ADMIN_ROTATION_MIN_DELAY_LEDGERS, PENDING_MIGRATION_TTL_LEDGERS};
pub use types::{
    Contract, ContractStatus, ContractSummary, DataKey, DepositMode, DisputeResolution,
    DisputeSplit, Error, GovernedParameters, Milestone, MilestoneApprovals, MilestoneSummary,
    PendingAdminProposal, ReadinessChecklist, ReleaseAuthorization, Reputation, SplitAmounts,
    CONTRACT_SUMMARY_SCHEMA_VERSION,
};

pub const MAX_MILESTONES: u32 = 10;
pub const MAX_SINGLE_AMOUNT_STROOPS: i128 = crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS;
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = MAX_SINGLE_AMOUNT_STROOPS;

#[contract]
pub struct Escrow;

mod create_contract;
mod dispute;
mod governance;

/// Governance-level errors for admin-gated operations.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    ContractNotFound = 6,
    EmptyRefundRequest = 7,
    DuplicateMilestoneInRefund = 8,
    AlreadyReleased = 9,
    AlreadyRefunded = 10,
    InsufficientFunds = 11,
    AlreadyInitialized = 12,
    InsufficientAccumulatedFees = 13,
    NotInitialized = 14,
    UnauthorizedRole = 15,
    ContractPaused = 16,
    EmergencyActive = 17,
    InvalidState = 18,
    InvalidRating = 19,
    SelfRating = 20,
    ReputationAlreadyIssued = 21,
    NotCompleted = 22,
    FreelancerMismatch = 23,
    InvalidStatusTransition = 24,
    ArbiterRequired = 25,
    InvalidDisputeSplit = 26,
    AccountingInvariantViolated = 27,
    PotentialOverflow = 28,
    AlreadyFinalized = 29,
    AmountMustBePositive = 30,
    SettlementTokenNotConfigured = 31,
    SettlementTokenAlreadyBound = 32,
    TotalCapExceeded = 33,
    TooManyMilestones = 34,
    MissingArbiter = 35,
    InvalidArbiter = 36,
    ContractCancelled = 37,
    ContractRefunded = 38,
    InvalidSettlementToken = 39,
    SettlementTokenIsSelf = 40,
    SettlementTokenIsAdmin = 41,
    EmptyComment = 42,
    CommentTooLong = 43,
}

impl Escrow {
    pub(crate) fn read_settlement_token(env: &Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::SettlementToken)
    }

    pub(crate) fn write_settlement_token(env: &Env, token: &Address) {
        env.storage()
            .persistent()
            .set(&DataKey::SettlementToken, token);
    }
}

#[contractimpl]
impl Escrow {
    pub fn bind_settlement_token(env: Env, admin: Address, token: Address) -> bool {
        Self::require_initialized(&env);
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));

        if admin != stored_admin {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        admin.require_auth();

        if Self::read_settlement_token(&env).is_some() {
            env.panic_with_error(EscrowError::SettlementTokenAlreadyBound);
        }

        if token == env.current_contract_address() {
            env.panic_with_error(EscrowError::SettlementTokenIsSelf);
        }

        if token == stored_admin {
            env.panic_with_error(EscrowError::SettlementTokenIsAdmin);
        }

        let token_client = token::Client::new(&env, &token);
        let _probe: i128 = token_client.balance(&env.current_contract_address());

        Self::write_settlement_token(&env, &token);

        env.events().publish(
            (Symbol::new(&env, "settlement_token_bound"),),
            (admin, token, env.ledger().timestamp()),
        );
        true
    }

    pub fn set_settlement_token(env: Env, admin: Address, token: Address) -> bool {
        Self::bind_settlement_token(env, admin, token)
    }

    pub fn get_settlement_token(env: Env) -> Option<Address> {
        Self::read_settlement_token(&env)
    }

    pub fn is_settlement_token_bound(env: Env) -> bool {
        Self::read_settlement_token(&env).is_some()
    }

    pub fn initialize(env: Env, admin: Address) -> bool {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().persistent().set(&DataKey::Initialized, &true);
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &1u32);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default();
        checklist.initialized = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (symbol_short!("init"), Symbol::new(&env, "admin_set")),
            (admin.clone(), env.ledger().timestamp()),
        );

        true
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    pub fn get_bounds(env: Env) -> ContractSummary {
        ContractSummary {
            schema_version: CONTRACT_SUMMARY_SCHEMA_VERSION,
            client: env.current_contract_address(),
            freelancer: env.current_contract_address(),
            arbiter: None,
            status: ContractStatus::Created,
            reputation_issued: false,
            total_amount: MAX_TOTAL_ESCROW_STROOPS,
            funded_amount: MAX_MILESTONES as i128,
            released_amount: 0,
            refundable_balance: 0,
            released_milestone_count: 0,
            milestones: Vec::new(&env),
        }
    }

    pub fn get_mainnet_readiness_info(env: Env) -> ReadinessChecklist {
        env.storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default()
    }

    pub fn deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool {
        Self::require_initialized(&env);
        Self::require_not_paused(&env);

        let validated = deposit::validate_deposit(&env, contract_id, &caller, amount);

        let token = Self::read_settlement_token(&env)
            .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured));

        let token_client = token::Client::new(&env, &token);

        token_client.transfer(&caller, &env.current_contract_address(), &amount);

        deposit::apply_validated_deposit(&env, contract_id, caller, validated)
    }

    pub fn finalize_contract(env: Env, contract_id: u32, finalizer: Address) -> bool {
        finalize::finalize_contract_impl(&env, contract_id, finalizer)
    }

    pub fn get_finalization_record(
        env: Env,
        contract_id: u32,
    ) -> Option<finalize::FinalizationRecord> {
        finalize::get_finalization_record_impl(&env, contract_id)
    }

    pub fn propose_client_migration(
        env: Env,
        contract_id: u32,
        current_client: Address,
        new_client: Address,
    ) -> bool {
        Self::require_not_paused(&env);
        Self::propose_client_migration_impl(&env, contract_id, current_client, new_client)
    }

    pub fn accept_client_migration(env: Env, contract_id: u32, new_client: Address) -> bool {
        Self::require_not_paused(&env);
        Self::accept_client_migration_impl(&env, contract_id, new_client)
    }

    pub fn has_pending_client_migration(env: Env, contract_id: u32) -> bool {
        Self::has_pending_client_migration_impl(&env, contract_id)
    }

    pub fn get_pending_client_migration(env: Env, contract_id: u32) -> PendingClientMigration {
        Self::get_pending_client_migration_impl(&env, contract_id)
    }

    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        Self::require_not_paused(&env);
        Self::require_not_finalized(&env, contract_id);
        approvals::approve_milestone(&env, contract_id, milestone_index, &caller)
            .unwrap_or_else(|e| env.panic_with_error(e))
    }

    fn grant_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let pending_key = DataKey::PendingReputationCredits(freelancer.clone());
        let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
        env.storage().persistent().set(&pending_key, &(pending + 1));
    }

    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        Self::require_not_paused(&env);
        caller.require_auth();

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);

        if contract.status != ContractStatus::Funded {
            env.panic_with_error(Error::InvalidState);
        }

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref() == Some(&caller);

        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if !is_client {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if !is_client && !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::MultiSig => {
                if !is_client && !is_freelancer {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
        }

        let mut milestones: Vec<Milestone> = ttl::load_milestones(&env, contract_id);
        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap().clone();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }

        if milestone.refunded {
            env.panic_with_error(Error::AlreadyRefunded);
        }

        approvals::check_approvals(&env, &contract, contract_id, milestone_index)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let available =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available < milestone.amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        let gross_amount = milestone.amount;

        let protocol_fee: i128 = if Self::is_initialized(&env) {
            let fee_bps = Self::read_protocol_fee_bps(&env);
            if fee_bps > 0 {
                Self::calculate_protocol_fee(&env, gross_amount, fee_bps)
            } else {
                0
            }
        } else {
            0
        };

        let net_amount = gross_amount - protocol_fee;

        let accumulated_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);
        let available_balance = contract.funded_amount
            - contract.released_amount
            - contract.refunded_amount
            - accumulated_fees;
        if available_balance < gross_amount {
            env.panic_with_error(EscrowError::InsufficientFunds);
        }

        let token = Self::read_settlement_token(&env)
            .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured));
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &contract.freelancer,
            &net_amount,
        );

        if protocol_fee > 0 {
            env.storage().persistent().set(
                &DataKey::AccumulatedProtocolFees,
                &(accumulated_fees + protocol_fee),
            );
        }

        milestone.released = true;
        milestone.funded_amount = gross_amount;
        milestones.set(milestone_index, milestone.clone());

        contract.released_amount = contract
            .released_amount
            .checked_add(net_amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        let new_accumulated = accumulated_fees + protocol_fee;
        let invariant_sum = contract.released_amount + contract.refunded_amount + new_accumulated;
        if invariant_sum > contract.funded_amount {
            env.panic_with_error(EscrowError::AccountingInvariantViolated);
        }

        approvals::clear_approvals(&env, contract_id, milestone_index);

        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
            Self::grant_pending_reputation_credit(&env, &contract.freelancer);
        }

        ttl::store_milestones(&env, contract_id, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("mlstn_rls"), contract_id),
            (
                milestone_index,
                gross_amount,
                protocol_fee,
                contract.released_amount,
                caller.clone(),
                env.ledger().timestamp(),
            ),
        );

        if all_released {
            env.events().publish(
                (symbol_short!("ctrct_cmp"), contract_id),
                (caller, env.ledger().timestamp()),
            );
        }

        true
    }

    pub fn is_milestone_overdue(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        let _contract: Contract = match env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
        {
            Some(c) => c,
            None => return false,
        };

        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = match env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
        {
            Some(m) => m,
            None => return false,
        };

        if milestone_index >= milestones.len() {
            return false;
        }

        let milestone = milestones.get(milestone_index).unwrap();

        if milestone.released {
            return false;
        }

        match milestone.deadline {
            None => false,
            Some(deadline) => now_seconds(&env) > deadline,
        }
    }

    pub fn refund_unreleased_milestones(
        env: Env,
        contract_id: u32,
        milestone_indices: Vec<u32>,
    ) -> i128 {
        Self::require_not_paused(&env);
        if milestone_indices.is_empty() {
            env.panic_with_error(EscrowError::EmptyRefundRequest);
        }

        for i in 0..milestone_indices.len() {
            for j in (i + 1)..milestone_indices.len() {
                if milestone_indices.get(i).unwrap() == milestone_indices.get(j).unwrap() {
                    env.panic_with_error(EscrowError::DuplicateMilestoneInRefund);
                }
            }
        }

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);

        if contract.status != ContractStatus::Created
            && contract.status != ContractStatus::Funded
            && contract.status != ContractStatus::Disputed
        {
            env.panic_with_error(EscrowError::InvalidState);
        }

        contract.client.require_auth();

        let mut milestones: Vec<Milestone> = ttl::load_milestones(&env, contract_id);

        let mut total_refund_amount: i128 = 0;

        for idx in milestone_indices.iter() {
            if idx >= milestones.len() {
                env.panic_with_error(Error::IndexOutOfBounds);
            }

            let milestone = milestones.get(idx).unwrap();

            if milestone.released {
                env.panic_with_error(Error::AlreadyReleased);
            }

            if milestone.refunded {
                env.panic_with_error(EscrowError::AlreadyRefunded);
            }

            if let Some(_deadline) = milestone.deadline {
                if !Self::is_milestone_overdue(env.clone(), contract_id, idx) {
                    env.panic_with_error(Error::MilestoneNotOverdue);
                }
            }

            total_refund_amount += milestone.amount;
        }

        let available_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available_balance < total_refund_amount {
            env.panic_with_error(EscrowError::InsufficientFunds);
        }

        let token = Self::read_settlement_token(&env)
            .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured));

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &contract.client,
            &total_refund_amount,
        );

        for idx in milestone_indices.iter() {
            let mut milestone = milestones.get(idx).unwrap();
            milestone.refunded = true;
            milestone.refunded_amount = milestone.amount;
            milestones.set(idx, milestone);
        }

        contract.refunded_amount = contract
            .refunded_amount
            .checked_add(total_refund_amount)
            .unwrap_or_else(|| env.panic_with_error(Error::InsufficientFunds));

        let all_refunded_or_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_refunded_or_released {
            let all_refunded = milestones.iter().all(|m| m.refunded);
            if all_refunded {
                contract.status = ContractStatus::Refunded;
            } else {
                contract.status = ContractStatus::Completed;
                Self::grant_pending_reputation_credit(&env, &contract.freelancer);
            }
        }

        ttl::store_milestones(&env, contract_id, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("refunded"), contract_id),
            (
                total_refund_amount,
                contract.status,
                env.ledger().timestamp(),
            ),
        );

        total_refund_amount
    }

    pub fn contract_exists(env: Env, contract_id: u32) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Contract(contract_id))
    }

    pub fn get_contract(env: Env, contract_id: u32) -> Contract {
        let contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        contract
    }

    pub fn get_next_contract_id(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    }

    pub fn get_contract_summary(env: Env, contract_id: u32) -> ContractSummary {
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_contract_and_milestones_ttl(&env, contract_id);

        let milestones = ttl::load_milestones(&env, contract_id);
        let total_amount: i128 =
            crate::amount_validation::accumulate_amounts(milestones.iter().map(|m| m.amount))
                .unwrap_or_else(|_| env.panic_with_error(EscrowError::PotentialOverflow));
        let released_milestone_count = milestones.iter().filter(|m| m.released).count() as u32;

        let mut milestone_summaries = Vec::new(&env);
        for (idx, m) in milestones.iter().enumerate() {
            milestone_summaries.push_back(MilestoneSummary {
                index: idx as u32,
                amount: m.amount,
                released: m.released,
                refunded: m.refunded,
            });
        }

        let reputation_issued = env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::ReputationIssued(contract_id))
            .unwrap_or(contract.reputation_issued);

        let refundable_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;

        ContractSummary {
            schema_version: CONTRACT_SUMMARY_SCHEMA_VERSION,
            client: contract.client,
            freelancer: contract.freelancer,
            arbiter: contract.arbiter,
            status: contract.status,
            reputation_issued,
            total_amount,
            funded_amount: contract.funded_amount,
            released_amount: contract.released_amount,
            refundable_balance,
            released_milestone_count,
            milestones: milestone_summaries,
        }
    }

    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<Milestone> {
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_milestone_ttl(&env, contract_id);
        milestones
    }

    pub fn get_milestone(env: Env, contract_id: u32, milestone_index: u32) -> Option<Milestone> {
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_milestone_ttl(&env, contract_id);
        milestones.get(milestone_index)
    }

    pub fn get_refundable_balance(env: Env, contract_id: u32) -> i128 {
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_contract_ttl(&env, contract_id);
        contract.funded_amount - contract.released_amount - contract.refunded_amount
    }

    pub fn get_milestone_approvals(
        env: Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<MilestoneApprovals> {
        let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
        let approvals = env.storage().temporary().get(&approval_key);
        if approvals.is_some() {
            env.storage().temporary().extend_ttl(
                &approval_key,
                ttl::PENDING_APPROVAL_BUMP_THRESHOLD,
                ttl::PENDING_APPROVAL_TTL_LEDGERS,
            );
        }
        approvals
    }

    pub fn pause(env: Env) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &true);

        env.events()
            .publish((symbol_short!("pause"), env.ledger().timestamp()), (admin,));
        true
    }

    pub fn unpause(env: Env) -> bool {
        Self::require_initialized(&env);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Emergency)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::EmergencyActive);
        }
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &false);

        env.events().publish(
            (symbol_short!("unpaused"), env.ledger().timestamp()),
            (admin,),
        );
        true
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn activate_emergency_pause(env: Env) -> bool {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));

        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            admin.require_auth();
        }
        env.storage().persistent().set(&DataKey::Emergency, &true);
        env.storage().persistent().set(&DataKey::Paused, &true);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default();
        checklist.emergency_controls_enabled = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (
                Symbol::new(&env, "emergency"),
                Symbol::new(&env, "activated"),
            ),
            (
                env.storage()
                    .persistent()
                    .get::<_, Address>(&DataKey::Admin)
                    .unwrap(),
                env.ledger().timestamp(),
            ),
        );
        true
    }

    pub fn resolve_emergency(env: Env) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Emergency, &false);
        env.storage().persistent().set(&DataKey::Paused, &false);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default();
        checklist.emergency_controls_enabled = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);
        env.events().publish(
            (
                Symbol::new(&env, "emergency"),
                Symbol::new(&env, "resolved"),
            ),
            (admin, env.ledger().timestamp()),
        );
        true
    }

    pub fn is_emergency(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Emergency)
            .unwrap_or(false)
    }

    pub fn cancel_contract(env: Env, contract_id: u32, client: Address) -> bool {
        Self::require_not_paused(&env);
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_contract_ttl(&env, contract_id);

        Self::require_not_finalized(&env, contract_id);

        if client != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        if contract.status == ContractStatus::Cancelled {
            env.panic_with_error(Error::AlreadyCancelled);
        }

        if contract.status != ContractStatus::Created && contract.status != ContractStatus::Funded {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        if contract.released_amount != 0 {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        client.require_auth();

        let refund_amount =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if refund_amount > 0 {
            let token = Self::read_settlement_token(&env)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &client,
                &refund_amount,
            );
        }

        contract.refunded_amount = contract
            .refunded_amount
            .checked_add(refund_amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InsufficientFunds));
        contract.status = ContractStatus::Cancelled;

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);
        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("cancelled"), contract_id),
            (client, refund_amount, env.ledger().timestamp()),
        );

        true
    }

    pub fn issue_reputation(
        env: Env,
        contract_id: u32,
        caller: Address,
        rating: u32,
        comment: String,
    ) -> bool {
        Self::require_not_paused(&env);
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        ttl::extend_contract_ttl(&env, contract_id);

        if caller != contract.client {
            env.panic_with_error(Error::UnauthorizedRole);
        }

        if rating < 1 || rating > 5 {
            env.panic_with_error(Error::InvalidRating);
        }

        if comment.len() == 0 {
            env.panic_with_error(Error::EmptyComment);
        }

        if comment.len() > 200 {
            env.panic_with_error(Error::CommentTooLong);
        }

        if contract.status != ContractStatus::Completed {
            env.panic_with_error(Error::NotCompleted);
        }

        if contract.reputation_issued {
            env.panic_with_error(Error::ReputationAlreadyIssued);
        }
        if contract.client == contract.freelancer {
            env.panic_with_error(Error::SelfRating);
        }

        caller.require_auth();
        contract.reputation_issued = true;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);
        env.storage()
            .persistent()
            .set(&DataKey::ReputationIssued(contract_id), &true);
        env.storage().persistent().extend_ttl(
            &DataKey::ReputationIssued(contract_id),
            ttl::PERSISTENT_BUMP_THRESHOLD,
            ttl::PERSISTENT_TTL_LEDGERS,
        );

        let pending_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
        let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
        if pending <= 0 {
            env.panic_with_error(Error::InvalidState);
        }
        env.storage().persistent().set(&pending_key, &(pending - 1));

        let rep_key = DataKey::Reputation(contract.freelancer.clone());
        let mut rep: types::Reputation =
            env.storage().persistent().get(&rep_key).unwrap_or_default();
        rep.completed_contracts += 1;
        rep.total_rating += rating as i128;
        rep.last_rating = rating as i128;
        env.storage().persistent().set(&rep_key, &rep);

        let comment_key = DataKey::ReputationComment(contract_id);
        env.storage().persistent().set(&comment_key, &comment);
        env.storage().persistent().extend_ttl(
            &comment_key,
            ttl::PERSISTENT_BUMP_THRESHOLD,
            ttl::PERSISTENT_TTL_LEDGERS,
        );

        true
    }

    pub fn get_reputation_comment(env: Env, contract_id: u32) -> Option<String> {
        let comment_key = DataKey::ReputationComment(contract_id);
        let comment: Option<String> = env.storage().persistent().get(&comment_key);
        if comment.is_some() {
            env.storage().persistent().extend_ttl(
                &comment_key,
                ttl::PERSISTENT_BUMP_THRESHOLD,
                ttl::PERSISTENT_TTL_LEDGERS,
            );
        }
        comment
    }

    pub fn get_reputation(env: Env, address: Address) -> Option<types::Reputation> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(address))
    }

    pub fn get_average_rating(env: Env, address: Address) -> Option<i128> {
        const SCALE: i128 = 10_000;

        let rep: types::Reputation = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(address))?;

        if rep.completed_contracts == 0 {
            return None;
        }

        rep.total_rating
            .checked_mul(SCALE)
            .and_then(|scaled| scaled.checked_div(rep.completed_contracts))
    }

    pub fn get_pending_reputation_credits(env: Env, address: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(address))
            .unwrap_or(0)
    }

    pub fn submit_work_evidence(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
        evidence: String,
    ) -> bool {
        Self::require_initialized(&env);
        Self::require_not_paused(&env);
        caller.require_auth();

        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);

        if caller != contract.freelancer {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        if contract.status != ContractStatus::Funded {
            env.panic_with_error(EscrowError::InvalidState);
        }

        if evidence.len() > 256 {
            env.panic_with_error(Error::EvidenceTooLong);
        }

        let milestone_key = Symbol::new(&env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }
        if milestone.refunded {
            env.panic_with_error(EscrowError::AlreadyRefunded);
        }

        milestone.work_evidence = Some(evidence.clone());
        milestones.set(milestone_index, milestone);

        ttl::store_milestones(&env, contract_id, &milestones);

        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("evidence"), contract_id),
            (
                milestone_index,
                contract.freelancer,
                env.ledger().timestamp(),
            ),
        );

        true
    }

    pub fn get_work_evidence(env: Env, contract_id: u32, milestone_index: u32) -> Option<String> {
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            return None;
        }

        milestones.get(milestone_index).unwrap().work_evidence
    }

    pub fn get_accumulated_protocol_fees(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get::<_, i128>(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0)
    }

    pub fn withdraw_protocol_fees(env: Env, amount: i128, to: Address) -> bool {
        Self::require_initialized(&env);

        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::ContractPaused);
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));

        admin.require_auth();

        if amount <= 0 {
            env.panic_with_error(EscrowError::AmountMustBePositive);
        }

        let accumulated: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);

        if amount > accumulated {
            env.panic_with_error(EscrowError::InsufficientAccumulatedFees);
        }

        let token = match Self::read_settlement_token(&env) {
            Some(t) => t,
            None => env.panic_with_error(Error::SettlementTokenNotConfigured),
        };

        let new_accumulated = accumulated - amount;
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &new_accumulated);

        env.storage().persistent().extend_ttl(
            &DataKey::AccumulatedProtocolFees,
            ttl::PERSISTENT_BUMP_THRESHOLD,
            ttl::PERSISTENT_TTL_LEDGERS,
        );

        let token_client = soroban_sdk::token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &to, &amount);

        env.events().publish(
            (symbol_short!("fee"), symbol_short!("withdraw")),
            (admin, to, amount, env.ledger().timestamp()),
        );

        true
    }

    pub fn get_pending_admin_proposed_at(env: Env) -> Option<u32> {
        let proposal: Option<PendingAdminProposal> =
            env.storage().persistent().get(&DataKey::PendingAdmin);
        proposal.map(|p| p.proposed_at_ledger)
    }

    pub(crate) fn read_protocol_fee_bps(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0)
    }

    pub fn calculate_protocol_fee(env: &Env, amount: i128, fee_bps: u32) -> i128 {
        if fee_bps == 0 {
            return 0;
        }
        let product = amount
            .checked_mul(fee_bps as i128)
            .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
        product / 10_000
    }

    pub(crate) fn require_initialized(env: &Env) {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::NotInitialized);
        }
    }

    fn is_initialized(env: &Env) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
    }

    pub fn raise_dispute(env: Env, contract_id: u32, caller: Address) -> bool {
        Self::require_initialized(&env);
        Self::require_not_paused(&env);
        caller.require_auth();

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);

        if caller != contract.client && caller != contract.freelancer {
            env.panic_with_error(Error::UnauthorizedRole);
        }

        if contract.arbiter.is_none() {
            env.panic_with_error(Error::ArbiterRequired);
        }

        match contract.status {
            ContractStatus::Funded | ContractStatus::PartiallyFunded => {}
            _ => env.panic_with_error(Error::InvalidState),
        }

        contract.status = ContractStatus::Disputed;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("dispute"), symbol_short!("opened")),
            (contract_id, caller),
        );

        true
    }

    pub fn resolve_dispute(
        env: Env,
        contract_id: u32,
        arbiter: Address,
        resolution: DisputeResolution,
    ) -> bool {
        Self::require_initialized(&env);
        Self::require_not_paused(&env);
        arbiter.require_auth();

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);

        if contract.status != ContractStatus::Disputed {
            env.panic_with_error(Error::InvalidStatusTransition);
        }

        match &contract.arbiter {
            Some(contract_arbiter) if *contract_arbiter == arbiter => {}
            _ => env.panic_with_error(Error::UnauthorizedRole),
        }

        let (client_payout, freelancer_payout) =
            dispute::resolution_payouts(&contract, &resolution)
                .unwrap_or_else(|e| env.panic_with_error(e));

        contract.refunded_amount += client_payout;
        contract.released_amount += freelancer_payout;

        contract.status = dispute::final_status_after_resolution(&contract);
        if contract.status == ContractStatus::Completed {
            Self::grant_pending_reputation_credit(&env, &contract.freelancer);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("dispute"), symbol_short!("resolved")),
            (contract_id, resolution.code()),
        );

        true
    }
}

#[cfg(test)]
mod test;
