//! TalentTrust Escrow — primary contract entry points.
//!
//! # Architecture
//!
//! All money-path validation is routed through [`amount_validation`]:
//! - [`create_contract`] → [`amount_validation::validate_milestone_amounts`]
//! - [`deposit_funds`]   → [`amount_validation::validate_deposit_amount`]
//!
//! This ensures a **single source of truth** for every stroop-precision check,
//! overflow guard, and cap enforcement across the contract lifecycle.
//!
//! # Error Mapping
//!
//! [`AmountValidationError`] variants are mapped to [`EscrowError`] at the
//! entry-point boundary so callers receive canonical contract error codes:
//!
//! | `AmountValidationError`       | `EscrowError`               |
//! |-------------------------------|-----------------------------|
//! | `NonPositiveAmount`           | `InvalidMilestoneAmount`    |
//! | `AmountExceedsMaximum`        | `TotalCapExceeded`          |
//! | `PotentialOverflow`           | `PotentialOverflow`         |
//! | `ExceedsContractMaximum`      | `TotalCapExceeded`          |
//! | `NonPositiveAmount` (deposit) | `InvalidDepositAmount`      |
//! | `ExceedsContractMaximum` (dep)| `DepositWouldExceedTotal`   |

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN,
    Env, Symbol, Vec,
};

mod ttl;

pub use ttl::{
    LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS,
    PENDING_MIGRATION_BUMP_THRESHOLD, PENDING_MIGRATION_TTL_LEDGERS,
};

use crate::types::ContractStatus;
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

mod approvals;
mod governance;
mod amount_validation;

pub use types::{
    Contract, ContractStatus, DataKey, Error, Milestone, MilestoneApprovals, ReadinessChecklist,
    ReleaseAuthorization,
};

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

// ─── Compile-time bounds (exported so tests can reference them) ───────────────

/// Maximum number of milestones allowed per contract.
pub const MAX_MILESTONES: u32 = 20;

/// Hard cap on the total stroop value of a single escrow contract (10 trillion stroops = 1 M XLM).
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = 1_000_000_0000000_i128;

// ─── Contract bounds query type ───────────────────────────────────────────────

/// Compile-time constants returned by [`Escrow::get_bounds`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractBounds {
    pub max_milestones: u32,
    pub max_total_escrow_stroops: i128,
}

// ─── Primary contract error enum ─────────────────────────────────────────────

/// Canonical error codes surfaced to callers of all contract entry points.
///
/// # Design invariant
///
/// Every validation path in the contract **must** terminate by mapping its
/// internal result into one of these variants before calling
/// `env.panic_with_error(...)`. No raw integer codes should escape to callers.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    // ── Participant / identity ─────────────────────────────────────────────
    /// `client` and `freelancer` must be distinct addresses.
    InvalidParticipant = 1,
    /// `arbiter` address overlaps with `client` or `freelancer`.
    InvalidArbiter = 2,
    /// An arbiter-requiring `ReleaseAuthorization` mode was selected but no arbiter was provided.
    MissingArbiter = 3,
    /// A contract participant address failed a role check.
    UnauthorizedRole = 4,

    // ── Milestone amount validation ────────────────────────────────────────
    /// Milestone list is empty.
    EmptyMilestones = 5,
    /// Too many milestones (exceeds [`MAX_MILESTONES`]).
    TooManyMilestones = 6,
    /// A milestone amount is zero or negative.
    InvalidMilestoneAmount = 7,
    /// The sum of all milestone amounts exceeds [`MAX_TOTAL_ESCROW_STROOPS`].
    TotalCapExceeded = 8,
    /// Checked arithmetic detected a potential i128 overflow.
    PotentialOverflow = 9,

    // ── Deposit validation ────────────────────────────────────────────────
    /// The deposit amount is zero or negative.
    InvalidDepositAmount = 10,
    /// Depositing this amount would push `total_deposited` above the contract total.
    DepositWouldExceedTotal = 11,

    // ── State machine ─────────────────────────────────────────────────────
    /// The referenced contract ID does not exist.
    ContractNotFound = 12,
    /// The contract is not in the required state for this operation.
    InvalidState = 13,

    // ── Milestone lifecycle ───────────────────────────────────────────────
    /// The milestone index is out of bounds.
    InvalidMilestone = 14,
    /// The milestone was already released.
    AlreadyReleased = 15,
    /// The milestone was already refunded.
    AlreadyRefunded = 16,
    /// The contract does not have enough funded balance.
    InsufficientFunds = 17,

    // ── Refund ───────────────────────────────────────────────────────────
    /// Refund request contains no milestone indices.
    EmptyRefundRequest = 18,
    /// The same milestone index appears more than once in a single refund request.
    DuplicateMilestoneInRefund = 19,

    // ── Approvals ─────────────────────────────────────────────────────────
    /// The required approval(s) are missing or were never submitted.
    InsufficientApprovals = 20,
    /// The approval record in temporary storage has expired (TTL elapsed).
    ApprovalExpired = 21,
    /// The caller already submitted an approval for this milestone.
    AlreadyApproved = 22,
    /// The milestone was already released (approval-time check).
    MilestoneAlreadyReleased = 23,

    // ── Misc ──────────────────────────────────────────────────────────────
    /// The amount supplied must be a positive value (> 0 stroops).
    AmountMustBePositive = 24,
    /// Accounting invariant violated (internal consistency check).
    AccountingInvariantViolated = 25,

    // ── Reputation ───────────────────────────────────────────────────────
    /// Rating value is outside the allowed range.
    InvalidRating = 26,
    /// Reputation token was already issued for this contract.
    ReputationAlreadyIssued = 27,
    /// The supplied freelancer address does not match the stored one.
    FreelancerMismatch = 28,
}
#[contractimpl]
impl Escrow {
    /// Initialize the contract with an admin.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `admin` - The address of the contract admin
    ///
    /// # Errors
    /// * `AlreadyInitialized` - If the contract is already initialized
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().persistent().get::<_, bool>(&DataKey::Initialized).unwrap_or(false) {
            env.panic_with_error(Error::AlreadyInitialized);
        }

        admin.require_auth();

        env.storage().persistent().set(&DataKey::Initialized, &true);
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::ProtocolFeeBps, &0u32);
        env.storage().persistent().set(&DataKey::AccumulatedProtocolFees, &0i128);
    }

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Initializes the escrow contract with the operational admin.
    ///
    /// This call is single-use and stores the admin address for future
    /// admin-gated entrypoints such as `withdraw_protocol_fees`.
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
        env.storage().persistent().set(&DataKey::Initialized, &true);
        env.storage().persistent().set(&DataKey::Admin, &admin);

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

    /// Returns the stored governance admin address, if one has been initialized.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    /// Returns the current mainnet readiness checklist.
    pub fn get_mainnet_readiness_info(env: Env) -> ReadinessChecklist {
        env.storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default()
    }

    /// Creates a new escrow contract with the specified client, freelancer, and milestone amounts.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `client` - The address of the client funding the contract
    /// * `freelancer` - The address of the freelancer performing the work
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestones` - Vector of milestone amounts (in stroops)
    /// * `release_authorization` - Authorization mode for milestone releases
    ///
    /// # Returns
    /// The unique contract ID
    ///
    /// # Errors
    /// * `InvalidParticipants` - If client and freelancer are the same address
    /// * `EmptyMilestones` - If no milestones are provided
    /// * `InvalidMilestoneAmount` - If any milestone amount is <= 0
    /// * `MissingArbiter` - If arbiter is required but not provided
    /// * `InvalidArbiter` - If arbiter is same as client or freelancer
    /// * `ContractIdOverflow` - If the next id would exceed `u32::MAX`
    /// * `ContractIdCollision` - If the allocated id slot is already occupied
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        milestones: Vec<i128>,
        release_authorization: ReleaseAuthorization,
    ) -> u32 {
        client.require_auth();

        if client == freelancer {
            env.panic_with_error(Error::InvalidParticipants);
        }

        // Validate arbiter requirements
        match release_authorization {
            ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter
                if arbiter.is_none() =>
            {
                env.panic_with_error(Error::MissingArbiter);
            }
            _ => {}
        }

        // Validate arbiter is not client or freelancer
        if let Some(ref arb) = arbiter {
            if arb == &client || arb == &freelancer {
                env.panic_with_error(Error::InvalidArbiter);
            }
        }

        if milestones.is_empty() {
            env.panic_with_error(Error::EmptyMilestones);
        }

        for amount in milestones.iter() {
            if amount <= 0 {
                env.panic_with_error(Error::InvalidMilestoneAmount);
            }
        }

        let id: u32 = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::NextContractId)
            .unwrap_or(1);

        // Store contract metadata
        let freelancer_addr = freelancer.clone();
        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            milestones: milestone_amounts,
            status: ContractStatus::Created,
            funded_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &contract);

        // Store milestones
        let mut milestone_vec: Vec<Milestone> = Vec::new(&env);
        for amount in milestones.iter() {
            milestone_vec.push_back(Milestone {
                amount,
                funded_amount: 0,
                released: false,
                refunded: false,
                work_evidence: None,
                refunded_amount: 0,
            });
        }
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(id), milestone_key), &milestone_vec);

        env.storage().persistent().set(&DataKey::Contract(id), &data);
        env.storage().persistent().set(&DataKey::ContractCount, &(id + 1));
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(id + 1));

        env.events().publish(
            (symbol_short!("created"), id),
            (client, freelancer_addr, env.ledger().timestamp()),
        );
        id
    }

    /// Assign an arbiter on a contract that was created without one.
    ///
    /// Only the client or freelancer can assign the arbiter. The arbiter must be
    /// distinct from both contract parties, can only be assigned once, and may
    /// only be assigned while the contract is in `Created` or `Funded` state.
    pub fn assign_arbiter(
        env: Env,
        contract_id: u32,
        caller: Address,
        arbiter: Address,
    ) -> bool {
        caller.require_auth();

        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::ContractPaused);
        }

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, ContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;

        if !is_client && !is_freelancer {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        if contract.arbiter.is_some() {
            env.panic_with_error(EscrowError::ArbiterAlreadyAssigned);
        }

        if arbiter == contract.client || arbiter == contract.freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }

        match contract.status {
            ContractStatus::Created | ContractStatus::Funded => {}
            _ => env.panic_with_error(EscrowError::InvalidStatusTransition),
        }

        contract.arbiter = Some(arbiter);
        env.storage().persistent().set(&contract_key, &contract);

        true
    }

    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128, caller: Address) -> bool {
        caller.require_auth();

    /// Deposits funds into the contract. Transitions to Funded status when fully funded.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `caller` - The address of the caller (must be the client)
    /// * `amount` - The amount to deposit (in stroops)
    ///
    /// # Returns
    /// `true` if deposit was successful
    ///
    /// # Errors
    /// * `AmountMustBePositive` - If amount is <= 0
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `InvalidState` - If contract is not in Created state
    /// * `UnauthorizedRole` - If caller is not the client
    pub fn deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool {
        if amount <= 0 {
            env.panic_with_error(Error::AmountMustBePositive);
        }

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        // Extend TTL on contract read
        ttl::extend_contract_ttl(&env, contract_id);

        // Only client can deposit
        if caller != contract.client {
            env.panic_with_error(Error::UnauthorizedRole);
        }

        caller.require_auth();

        // Can only deposit in Created state
        if contract.status != ContractStatus::Created {
            env.panic_with_error(Error::InvalidState);
        }

        contract.funded_amount += amount;

        // Calculate total milestone amount
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap();

        // Extend TTL on milestone read
        ttl::extend_milestone_ttl(&env, contract_id);

        let total_amount: i128 = milestones.iter().map(|m| m.amount).sum();

        // Transition to Funded if fully funded
        if contract.funded_amount >= total_amount && contract.status == ContractStatus::Created {
            contract.status = ContractStatus::Funded;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        // Extend TTL on contract write
        ttl::extend_contract_ttl(&env, contract_id);

        true
    }

    /// Approves a milestone for release.
    ///
    /// Records the approval in temporary storage with TTL expiry.
    /// Approvals automatically expire after PENDING_APPROVAL_TTL_LEDGERS.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `caller` - The address of the caller (must be authorized)
    /// * `milestone_index` - The index of the milestone to approve
    ///
    /// # Returns
    /// `true` if approval was recorded successfully
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `InvalidState` - If contract is not in Funded state
    /// * `IndexOutOfBounds` - If milestone index is invalid
    /// * `MilestoneAlreadyReleased` - If milestone was already released
    /// * `UnauthorizedRole` - If caller is not authorized to approve
    /// * `AlreadyApproved` - If caller has already approved this milestone
    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        approvals::approve_milestone(&env, contract_id, milestone_index, &caller)
            .unwrap_or_else(|e| env.panic_with_error(e))
    }

    /// Releases a specific milestone, transferring funds to the freelancer (minus protocol fees).
    ///
    /// Requires valid, non-expired approvals based on the contract's ReleaseAuthorization mode.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `caller` - The address of the caller (must be authorized)
    /// * `milestone_index` - The index of the milestone to release
    ///
    /// # Returns
    /// `true` if release was successful
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `InvalidState` - If contract is not in Funded state
    /// * `IndexOutOfBounds` - If milestone index is out of bounds
    /// * `AlreadyReleased` - If milestone was already released
    /// * `AlreadyRefunded` - If milestone was already refunded
    /// * `InsufficientFunds` - If contract doesn't have enough funded balance
    /// * `InsufficientApprovals` - If required approvals are missing
    /// * `ApprovalExpired` - If approvals have expired
    /// * `UnauthorizedRole` - If caller is not authorized to release
    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        if !is_client && !is_freelancer {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        // Mark this milestone as released
        let milestone_key = DataKey::MilestoneReleased(contract_id, milestone_index);
        env.storage().persistent().set(&milestone_key, &true);
        // Verify contract is in Funded state
        if contract.status != ContractStatus::Funded {
            env.panic_with_error(Error::InvalidState);
        }

        // Check authorization for release
        let is_client = caller == contract.client;
        let is_arbiter = contract.arbiter.as_ref() == Some(&caller);

        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if !is_client {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if !is_client && !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::MultiSig => {
                if !is_client && !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
        }

        caller.require_auth();

        // Check for valid approvals
        approvals::check_approvals(&env, &contract, contract_id, milestone_index)
            .unwrap_or_else(|e| env.panic_with_error(e));

// ─── Helper: map AmountValidationError → EscrowError at entry-point boundary ──

/// Map a [`AmountValidationError`] raised during **milestone** validation into
/// the corresponding [`EscrowError`] and immediately panic the host.
///
/// # Invariants
/// - `NonPositiveAmount`      → [`EscrowError::InvalidMilestoneAmount`]
/// - `AmountExceedsMaximum`   → [`EscrowError::TotalCapExceeded`]
/// - `PotentialOverflow`      → [`EscrowError::PotentialOverflow`]
/// - `ExceedsContractMaximum` → [`EscrowError::TotalCapExceeded`]
/// - `InvalidStroopPrecision` → [`EscrowError::InvalidMilestoneAmount`]
#[inline]
fn map_milestone_validation_error(env: &Env, e: AmountValidationError) -> ! {
    match e {
        AmountValidationError::NonPositiveAmount => {
            env.panic_with_error(EscrowError::InvalidMilestoneAmount)
        }
        AmountValidationError::AmountExceedsMaximum => {
            env.panic_with_error(EscrowError::TotalCapExceeded)
        // Extend TTL on milestone read
        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap().clone();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }
        AmountValidationError::PotentialOverflow => {
            env.panic_with_error(EscrowError::PotentialOverflow)
        }
        AmountValidationError::ExceedsContractMaximum => {
            env.panic_with_error(EscrowError::TotalCapExceeded)
        }
        AmountValidationError::InvalidStroopPrecision => {
            env.panic_with_error(EscrowError::InvalidMilestoneAmount)
        }

        // Calculate protocol fee
        let protocol_fee_bps = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0u32);
        let fee = (milestone.amount * protocol_fee_bps as i128) / 10_000;
        let amount_to_freelancer = milestone.amount - fee;

        // Accumulate protocol fees
        let mut accumulated = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0i128);
        accumulated += fee;
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &accumulated);

        milestone.released = true;
        milestones.set(milestone_index, milestone);
        contract.released_amount += amount_to_freelancer;

        // Accumulate protocol fees if initialized with a fee rate
        if Self::is_initialized(env) {
            let fee_bps = Self::get_protocol_fee_bps(env);
            if fee_bps > 0 {
                let fee = Self::calculate_protocol_fee(milestone.amount, fee_bps);
                let current_accumulated: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AccumulatedProtocolFees)
                    .unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&DataKey::AccumulatedProtocolFees, &(current_accumulated + fee));
            }
        }

        // Clear approvals after successful release
        approvals::clear_approvals(&env, contract_id, milestone_index);

        // Check if all milestones are released
        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
        }

        env.storage().persistent().set(
            &(DataKey::Contract(contract_id), milestone_key),
            &milestones,
        );
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        // Extend TTL on contract and milestone writes
        ttl::extend_contract_and_milestones_ttl(&env, contract_id);

        true
    }
}

/// Map a [`AmountValidationError`] raised during **deposit** validation into
/// the corresponding [`EscrowError`] and immediately panic the host.
///
/// # Invariants
/// - `NonPositiveAmount`      → [`EscrowError::InvalidDepositAmount`]
/// - `AmountExceedsMaximum`   → [`EscrowError::DepositWouldExceedTotal`]
/// - `PotentialOverflow`      → [`EscrowError::PotentialOverflow`]
/// - `ExceedsContractMaximum` → [`EscrowError::DepositWouldExceedTotal`]
/// - `InvalidStroopPrecision` → [`EscrowError::InvalidDepositAmount`]
#[inline]
fn map_deposit_validation_error(env: &Env, e: AmountValidationError) -> ! {
    match e {
        AmountValidationError::NonPositiveAmount => {
            env.panic_with_error(EscrowError::InvalidDepositAmount)
        }
        AmountValidationError::AmountExceedsMaximum => {
            env.panic_with_error(EscrowError::DepositWouldExceedTotal)
        }
        AmountValidationError::PotentialOverflow => {
            env.panic_with_error(EscrowError::PotentialOverflow)
        }
        AmountValidationError::ExceedsContractMaximum => {
            env.panic_with_error(EscrowError::DepositWouldExceedTotal)
        }
        AmountValidationError::InvalidStroopPrecision => {
            env.panic_with_error(EscrowError::InvalidDepositAmount)
        }
    }
}

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct Escrow;
    /// Refunds unreleased milestones back to the client.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_indices` - Vector of milestone indices to refund
    ///
    /// # Returns
    /// The total amount refunded
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `EmptyRefundRequest` - If milestone_indices is empty
    /// * `DuplicateMilestoneInRefund` - If the same milestone appears multiple times
    /// * `IndexOutOfBounds` - If any milestone index is out of bounds
    /// * `AlreadyReleased` - If any milestone was already released
    /// * `AlreadyRefunded` - If any milestone was already refunded
    /// * `InsufficientFunds` - If contract doesn't have enough balance to refund
    pub fn refund_unreleased_milestones(
        env: Env,
        contract_id: u32,
        milestone_indices: Vec<u32>,
    ) -> i128 {
        // Validate non-empty request
        if milestone_indices.is_empty() {
            env.panic_with_error(Error::EmptyRefundRequest);
        }

        // Check for duplicates
        for i in 0..milestone_indices.len() {
            for j in (i + 1)..milestone_indices.len() {
                if milestone_indices.get(i).unwrap() == milestone_indices.get(j).unwrap() {
                    env.panic_with_error(Error::DuplicateMilestoneInRefund);
                }
            }
        }

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        // Extend TTL on contract read
        ttl::extend_contract_ttl(&env, contract_id);

        contract.client.require_auth();

        let milestone_key = Symbol::new(&env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        // Extend TTL on milestone read
        ttl::extend_milestone_ttl(&env, contract_id);

        let mut total_refund_amount: i128 = 0;

        // Validate all milestones first
        for idx in milestone_indices.iter() {
            if idx >= milestones.len() {
                env.panic_with_error(Error::IndexOutOfBounds);
            }

            let milestone = milestones.get(idx).unwrap();

            if milestone.released {
                env.panic_with_error(Error::AlreadyReleased);
            }

            if milestone.refunded {
                env.panic_with_error(Error::AlreadyRefunded);
            }

            total_refund_amount += milestone.amount;
        }

        // Check if there's enough balance
        let available_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available_balance < total_refund_amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        // Mark milestones as refunded
        for idx in milestone_indices.iter() {
            let mut milestone = milestones.get(idx).unwrap();
            milestone.refunded = true;
            milestones.set(idx, milestone);
        }

        contract.refunded_amount += total_refund_amount;

        // Check if all unreleased milestones are refunded
        let all_released_or_refunded = milestones.iter().all(|m| m.released || m.refunded);
        if all_released_or_refunded {
            let all_refunded = milestones.iter().all(|m| m.refunded);
            if all_refunded {
                contract.status = ContractStatus::Refunded;
            } else {
                // Some released, some refunded
                contract.status = ContractStatus::Completed;
            }
        }

        env.storage().persistent().set(
            &(DataKey::Contract(contract_id), milestone_key),
            &milestones,
        );
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        // Extend TTL on contract and milestone writes
        ttl::extend_contract_and_milestones_ttl(&env, contract_id);

        total_refund_amount
    }

    /// Retrieves contract information.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    ///
    /// # Returns
    /// The contract data
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    pub fn get_contract(env: Env, contract_id: u32) -> Contract {
        let contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        
        // Extend TTL on contract read
        ttl::extend_contract_ttl(&env, contract_id);
        
        contract
    }

    /// Retrieves all milestones for a contract.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    ///
    /// # Returns
    /// Vector of milestones
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<Milestone> {
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        
        // Extend TTL on milestone read
        ttl::extend_milestone_ttl(&env, contract_id);
        
        milestones
    }

    /// Calculates the refundable balance (funded but not released or refunded).
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    ///
    /// # Returns
    /// The refundable balance amount
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    pub fn get_refundable_balance(env: Env, contract_id: u32) -> i128 {
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        // Extend TTL on contract read
        ttl::extend_contract_ttl(&env, contract_id);

        contract.funded_amount - contract.released_amount - contract.refunded_amount
    }

    /// Retrieves approval status for a milestone.
    ///
    /// Returns None if approvals have expired or don't exist.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_index` - The milestone index
    ///
    /// # Returns
    /// Optional MilestoneApprovals struct
    pub fn get_milestone_approvals(
        env: Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<MilestoneApprovals> {
        let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
        env.storage().temporary().get(&approval_key)
    }
}

#[cfg(test)]
mod test;
