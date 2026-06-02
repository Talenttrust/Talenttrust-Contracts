//! TalentTrust Escrow — primary contract entry points.
//!
//! # Architecture
//!
//! All money-path validation is routed through [`amount_validation`]:
//! - [`Escrow::create_contract`] → [`amount_validation::validate_milestone_amounts`]
//! - [`Escrow::deposit_funds`]   → [`amount_validation::validate_deposit_amount`]
//!
//! This ensures a **single source of truth** for every stroop-precision check,
//! overflow guard, and cap enforcement across the contract lifecycle.
//!
//! # Access Control
//!
//! `deposit_funds` requires an explicit `depositor: Address` parameter.
//! The call fails if:
//!   1. `depositor.require_auth()` is not satisfied by the Soroban host, **or**
//!   2. `depositor != contract.client` (role enforcement).
//!
//! Both checks execute **before** any state mutation, preserving the
//! fail-closed design across the entire deposit path.
//!
//! # Error Mapping
//!
//! [`AmountValidationError`] variants are mapped to [`EscrowError`] at the
//! entry-point boundary so callers receive canonical contract error codes:
//!
//! | `AmountValidationError`        | `EscrowError`              |
//! |--------------------------------|----------------------------|
//! | `NonPositiveAmount`            | `InvalidMilestoneAmount`   |
//! | `AmountExceedsMaximum`         | `TotalCapExceeded`         |
//! | `PotentialOverflow`            | `PotentialOverflow`        |
//! | `ExceedsContractMaximum`       | `TotalCapExceeded`         |
//! | `NonPositiveAmount` (deposit)  | `InvalidDepositAmount`     |
//! | `ExceedsContractMaximum` (dep) | `DepositWouldExceedTotal`  |

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

mod types;
mod ttl;
mod approvals;
pub mod amount_validation;

pub use types::{
    Contract, ContractStatus, DataKey, Error, Milestone, MilestoneApprovals,
    ReleaseAuthorization, DepositMode,
};
pub use amount_validation::{
    AmountValidationError, safe_add_amounts, safe_subtract_amounts,
    validate_single_amount, validate_milestone_amounts, validate_deposit_amount,
};

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short,
    Address, Env, Symbol, Vec,
};

// ─── Compile-time bounds ──────────────────────────────────────────────────────

/// Maximum number of milestones permitted per escrow contract.
pub const MAX_MILESTONES: u32 = 20;

/// Hard cap on the sum of all milestone stroops for a single contract
/// (1 000 000 XLM × 10 000 000 stroops/XLM = 10 000 000 000 000 stroops).
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = 1_000_000_0000000_i128;

// ─── Bounds query type ────────────────────────────────────────────────────────

/// Compile-time constants returned by [`Escrow::get_bounds`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractBounds {
    pub max_milestones: u32,
    pub max_total_escrow_stroops: i128,
}

// ─── Primary contract error enum ─────────────────────────────────────────────

/// Canonical error codes surfaced to all contract callers.
///
/// # Invariant
///
/// Every internal validation path **must** terminate by mapping its result
/// into one of these variants via `env.panic_with_error(EscrowError::*)`.
/// No raw error codes or `AmountValidationError` variants should escape
/// past an entry-point boundary.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    // ── Participant / identity ────────────────────────────────────────────
    /// `client` and `freelancer` must be distinct addresses.
    InvalidParticipant = 1,
    /// `arbiter` overlaps with `client` or `freelancer`.
    InvalidArbiter = 2,
    /// Arbiter-requiring `ReleaseAuthorization` mode but no arbiter provided.
    MissingArbiter = 3,
    /// Caller failed a role check (not the expected participant).
    UnauthorizedRole = 4,

    // ── Milestone amount validation ───────────────────────────────────────
    /// Milestone list is empty.
    EmptyMilestones = 5,
    /// Milestone count exceeds [`MAX_MILESTONES`].
    TooManyMilestones = 6,
    /// A milestone amount is zero or negative.
    InvalidMilestoneAmount = 7,
    /// Sum of milestone amounts exceeds [`MAX_TOTAL_ESCROW_STROOPS`].
    TotalCapExceeded = 8,
    /// Checked arithmetic would overflow `i128`.
    PotentialOverflow = 9,

    // ── Deposit validation ────────────────────────────────────────────────
    /// Deposit amount is zero or negative.
    InvalidDepositAmount = 10,
    /// Depositing this amount would exceed the milestone total.
    DepositWouldExceedTotal = 11,

    // ── State machine ─────────────────────────────────────────────────────
    /// No contract exists for the given `contract_id`.
    ContractNotFound = 12,
    /// Contract is not in the required state for this operation.
    InvalidState = 13,

    // ── Milestone lifecycle ───────────────────────────────────────────────
    /// Milestone index is out of bounds.
    InvalidMilestone = 14,
    /// Milestone was already released.
    AlreadyReleased = 15,
    /// Milestone was already refunded.
    AlreadyRefunded = 16,
    /// Insufficient funded balance to cover the operation.
    InsufficientFunds = 17,

    // ── Refund ────────────────────────────────────────────────────────────
    /// Refund request contains no milestone indices.
    EmptyRefundRequest = 18,
    /// The same milestone index appears twice in one refund request.
    DuplicateMilestoneInRefund = 19,

    // ── Approvals ─────────────────────────────────────────────────────────
    /// Required approval(s) missing or never submitted.
    InsufficientApprovals = 20,
    /// Approval record expired (temporary-storage TTL elapsed).
    ApprovalExpired = 21,
    /// Caller already submitted an approval for this milestone.
    AlreadyApproved = 22,
    /// Milestone already released (checked at approval time).
    MilestoneAlreadyReleased = 23,

    // ── Misc ──────────────────────────────────────────────────────────────
    /// Amount must be > 0 stroops.
    AmountMustBePositive = 24,
    /// Internal accounting invariant was violated.
    AccountingInvariantViolated = 25,

    // ── Reputation ───────────────────────────────────────────────────────
    /// Rating is outside the allowed range.
    InvalidRating = 26,
    /// Reputation token already issued for this contract.
    ReputationAlreadyIssued = 27,
    /// Supplied freelancer address does not match the stored one.
    FreelancerMismatch = 28,
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Map an [`AmountValidationError`] from **milestone** validation to the
/// canonical [`EscrowError`] and abort the host via `panic_with_error`.
///
/// # Mapping table
/// | `AmountValidationError`   | `EscrowError`           |
/// |---------------------------|-------------------------|
/// | `NonPositiveAmount`       | `InvalidMilestoneAmount`|
/// | `AmountExceedsMaximum`    | `TotalCapExceeded`      |
/// | `PotentialOverflow`       | `PotentialOverflow`     |
/// | `ExceedsContractMaximum`  | `TotalCapExceeded`      |
/// | `InvalidStroopPrecision`  | `InvalidMilestoneAmount`|
#[inline]
fn map_milestone_err(env: &Env, e: AmountValidationError) -> ! {
    match e {
        AmountValidationError::NonPositiveAmount =>
            env.panic_with_error(EscrowError::InvalidMilestoneAmount),
        AmountValidationError::AmountExceedsMaximum =>
            env.panic_with_error(EscrowError::TotalCapExceeded),
        AmountValidationError::PotentialOverflow =>
            env.panic_with_error(EscrowError::PotentialOverflow),
        AmountValidationError::ExceedsContractMaximum =>
            env.panic_with_error(EscrowError::TotalCapExceeded),
        AmountValidationError::InvalidStroopPrecision =>
            env.panic_with_error(EscrowError::InvalidMilestoneAmount),
    }
}

/// Map an [`AmountValidationError`] from **deposit** validation to the
/// canonical [`EscrowError`] and abort the host via `panic_with_error`.
///
/// # Mapping table
/// | `AmountValidationError`   | `EscrowError`            |
/// |---------------------------|--------------------------|
/// | `NonPositiveAmount`       | `InvalidDepositAmount`   |
/// | `AmountExceedsMaximum`    | `DepositWouldExceedTotal`|
/// | `PotentialOverflow`       | `PotentialOverflow`      |
/// | `ExceedsContractMaximum`  | `DepositWouldExceedTotal`|
/// | `InvalidStroopPrecision`  | `InvalidDepositAmount`   |
#[inline]
fn map_deposit_err(env: &Env, e: AmountValidationError) -> ! {
    match e {
        AmountValidationError::NonPositiveAmount =>
            env.panic_with_error(EscrowError::InvalidDepositAmount),
        AmountValidationError::AmountExceedsMaximum =>
            env.panic_with_error(EscrowError::DepositWouldExceedTotal),
        AmountValidationError::PotentialOverflow =>
            env.panic_with_error(EscrowError::PotentialOverflow),
        AmountValidationError::ExceedsContractMaximum =>
            env.panic_with_error(EscrowError::DepositWouldExceedTotal),
        AmountValidationError::InvalidStroopPrecision =>
            env.panic_with_error(EscrowError::InvalidDepositAmount),
    }
}

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    // ── Utility ──────────────────────────────────────────────────────────

    /// Hello-world entry point used by CI smoke tests.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Return compile-time safety bounds for off-chain consumers.
    pub fn get_bounds(_env: Env) -> ContractBounds {
        ContractBounds {
            max_milestones: MAX_MILESTONES,
            max_total_escrow_stroops: MAX_TOTAL_ESCROW_STROOPS,
        }
    }

    // ── Contract creation ─────────────────────────────────────────────────

    /// Create a new escrow contract.
    ///
    /// Validates that all milestone amounts are positive, that their sum does
    /// not exceed [`MAX_TOTAL_ESCROW_STROOPS`], and that participant addresses
    /// are distinct and internally consistent with the chosen
    /// [`ReleaseAuthorization`] mode.
    ///
    /// # Preconditions
    /// - `client.require_auth()` must be satisfied by the host.
    /// - `client != freelancer`.
    /// - `milestones` must be non-empty and contain at most [`MAX_MILESTONES`]
    ///   entries, each > 0 stroops.
    /// - Sum of milestones ≤ [`MAX_TOTAL_ESCROW_STROOPS`].
    /// - When `release_authorization` is `ArbiterOnly` or `ClientAndArbiter`,
    ///   `arbiter` must be `Some` and must differ from both `client` and
    ///   `freelancer`.
    ///
    /// # Errors
    /// - [`EscrowError::InvalidParticipant`] — `client == freelancer`.
    /// - [`EscrowError::MissingArbiter`] — arbiter required but absent.
    /// - [`EscrowError::InvalidArbiter`] — arbiter overlaps a participant.
    /// - [`EscrowError::EmptyMilestones`] — no milestones provided.
    /// - [`EscrowError::TooManyMilestones`] — count > [`MAX_MILESTONES`].
    /// - [`EscrowError::InvalidMilestoneAmount`] — a milestone ≤ 0 stroops.
    /// - [`EscrowError::TotalCapExceeded`] — sum > [`MAX_TOTAL_ESCROW_STROOPS`].
    /// - [`EscrowError::PotentialOverflow`] — sum would overflow `i128`.
    ///
    /// # Returns
    /// A monotonically increasing `contract_id` starting at 1.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestones: Vec<i128>,
        release_authorization: ReleaseAuthorization,
    ) -> u32 {
        // ── 1. Authentication ─────────────────────────────────────────────
        client.require_auth();

        // ── 2. Participant validation ─────────────────────────────────────
        if client == freelancer {
            env.panic_with_error(Error::InvalidParticipants);
        }

        match release_authorization {
            ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter => {
                if arbiter.is_none() {
                    env.panic_with_error(Error::MissingArbiter);
                }
            }
            _ => {}
        }

        if let Some(ref arb) = arbiter {
            if arb == &client || arb == &freelancer {
                env.panic_with_error(Error::InvalidArbiter);
            }
        }

        // ── 3. Milestone list shape ───────────────────────────────────────
        if milestones.is_empty() {
            env.panic_with_error(Error::EmptyMilestones);
        }
        if milestones.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        // ── 4. Amount validation (single source of truth) ─────────────────
        // Collect into a native slice so validate_milestone_amounts can work
        // over a plain &[i128] without pulling in alloc.
        let mut raw: [i128; 20] = [0i128; 20];
        let count = milestones.len() as usize;
        for i in 0..count {
            raw[i] = milestones.get(i as u32).unwrap();
        }
        validate_milestone_amounts(&raw[..count], MAX_TOTAL_ESCROW_STROOPS)
            .unwrap_or_else(|e| map_milestone_err(&env, e));

        // ── 5. Allocate contract ID ───────────────────────────────────────
        let id: u32 = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::NextContractId)
            .unwrap_or(1);

        // ── 6. Persist contract metadata ──────────────────────────────────
        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: arbiter.clone(),
            status: ContractStatus::Created,
            funded_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &contract);

        // ── 7. Persist milestones ─────────────────────────────────────────
        let mut milestone_vec: Vec<Milestone> = Vec::new(&env);
        for i in 0..milestones.len() {
            let amount = milestones.get(i).unwrap();
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

        // ── 8. Advance counter ────────────────────────────────────────────
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(id + 1));

        // ── 9. Event ──────────────────────────────────────────────────────
        env.events().publish(
            (symbol_short!("created"), id),
            (client, freelancer, env.ledger().timestamp()),
        );

        id
    }

    // ── Deposit ───────────────────────────────────────────────────────────

    /// Deposit funds into an existing escrow contract.
    ///
    /// # Security invariant
    ///
    /// **Only the designated `client` may fund the contract.**  
    /// This function calls `depositor.require_auth()` unconditionally, then
    /// checks `depositor == contract.client` before any state mutation.
    /// Both checks execute in the *read-only pre-mutation phase*, so a failed
    /// auth or role mismatch leaves the contract state completely unchanged
    /// (fail-closed).
    ///
    /// Without this guard an unauthenticated actor could:
    /// - Forge a deposit to advance the state machine (`Created → Funded`).
    /// - Trigger misleading on-chain events that downstream indexers trust.
    /// - Unblock milestone-release paths that require `Funded` status.
    ///
    /// # State transitions
    /// ```text
    /// Created        →  Created        (amount < milestone total)
    /// Created        →  Funded         (amount == milestone total, ExactTotal)
    /// Created        →  PartiallyFunded (amount < total, Incremental — if that
    ///                                   status is enabled on your build)
    /// ```
    ///
    /// # Arguments
    /// * `contract_id` — ID returned by [`create_contract`].
    /// * `depositor`   — Address initiating the deposit; **must equal the
    ///                   stored `client`** and must satisfy `require_auth`.
    /// * `amount`      — Stroop amount to record as deposited (> 0).
    ///
    /// # Preconditions
    /// - `depositor.require_auth()` is satisfied by the Soroban host.
    /// - `depositor == contract.client`.
    /// - Contract is in `Created` state.
    /// - `amount > 0`.
    /// - `funded_amount + amount ≤ milestone total`.
    ///
    /// # Errors
    /// - [`EscrowError::ContractNotFound`]    — unknown `contract_id`.
    /// - [`EscrowError::UnauthorizedRole`]    — `depositor != client`.
    /// - [`EscrowError::InvalidState`]        — contract not in `Created`.
    /// - [`EscrowError::InvalidDepositAmount`]— `amount ≤ 0`.
    /// - [`EscrowError::DepositWouldExceedTotal`] — would exceed milestone sum.
    /// - [`EscrowError::PotentialOverflow`]   — arithmetic overflow detected.
    ///
    /// # Returns
    /// `true` on success.
    ///
    /// # Panics
    /// Panics via `env.panic_with_error` on any of the errors above; never
    /// returns silently on a validation failure.
    pub fn deposit_funds(
        env: Env,
        contract_id: u32,
        depositor: Address,
        amount: i128,
    ) -> bool {
        // ── 1. Authentication — must happen before ANY state reads that
        //       feed role-sensitive branches to prevent auth-bypass attacks.
        depositor.require_auth();

        // ── 2. Load contract (fail-closed if not found) ───────────────────
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // ── 3. Role enforcement — depositor MUST be the stored client ──────
        //
        //    This is a critical invariant: only the client who originally
        //    signed the contract can fund it.  A mismatch here means someone
        //    is trying to impersonate the client or inject a fraudulent
        //    deposit, both of which must be rejected.
        if depositor != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        // ── 4. State-machine guard ────────────────────────────────────────
        if contract.status != ContractStatus::Created {
            env.panic_with_error(EscrowError::InvalidState);
        }

        // ── 5. Amount validation (single source of truth) ─────────────────
        //    Compute the milestone total so validate_deposit_amount can
        //    enforce that funded_amount + amount ≤ total.
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        let mut milestone_total: i128 = 0;
        for i in 0..milestones.len() {
            let m = milestones.get(i).unwrap();
            milestone_total = safe_add_amounts(milestone_total, m.amount)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        }

        // Routes through amount_validation::validate_deposit_amount —
        // the single canonical path for all deposit amount checks.
        validate_deposit_amount(amount, contract.funded_amount, milestone_total)
            .unwrap_or_else(|e| map_deposit_err(&env, e));

        // ── 6. Mutate state ───────────────────────────────────────────────
        contract.funded_amount = safe_add_amounts(contract.funded_amount, amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        if contract.funded_amount >= milestone_total {
            contract.status = ContractStatus::Funded;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        // ── 7. Event ──────────────────────────────────────────────────────
        env.events().publish(
            (symbol_short!("deposited"), contract_id),
            (depositor, amount, contract.funded_amount, env.ledger().timestamp()),
        );

        true
    }

    // ── Approvals ─────────────────────────────────────────────────────────

    /// Record an approval for milestone release.
    ///
    /// Approvals are stored in *temporary* storage with a TTL defined by
    /// [`ttl::PENDING_APPROVAL_TTL_LEDGERS`].  Once expired, the approval is
    /// automatically evicted and treated as absent (fail-closed).
    ///
    /// # Errors
    /// - [`Error::ContractNotFound`]       — unknown `contract_id`.
    /// - [`Error::InvalidState`]           — contract not `Funded`.
    /// - [`Error::IndexOutOfBounds`]       — `milestone_index` out of range.
    /// - [`Error::MilestoneAlreadyReleased`] — milestone already released.
    /// - [`Error::UnauthorizedRole`]       — `caller` not authorised to approve.
    /// - [`Error::AlreadyApproved`]        — `caller` already approved.
    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        approvals::approve_milestone(&env, contract_id, milestone_index, &caller)
            .unwrap_or_else(|e| env.panic_with_error(e))
    }

    /// Release a funded milestone, transferring value to the freelancer.
    ///
    /// Requires valid, non-expired approvals consistent with the contract's
    /// [`ReleaseAuthorization`] mode (checked via [`approvals::check_approvals`]).
    /// Approvals are cleared after a successful release to prevent reuse.
    ///
    /// # Errors
    /// - [`Error::ContractNotFound`]        — unknown `contract_id`.
    /// - [`Error::InvalidState`]            — contract not `Funded`.
    /// - [`Error::IndexOutOfBounds`]        — `milestone_index` out of range.
    /// - [`Error::MilestoneAlreadyReleased`]— already released.
    /// - [`Error::AlreadyRefunded`]         — already refunded.
    /// - [`Error::InsufficientFunds`]       — not enough funded balance.
    /// - [`Error::InsufficientApprovals`]   — approvals missing.
    /// - [`Error::UnauthorizedRole`]        — `caller` not authorised.
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

        if contract.status != ContractStatus::Funded {
            env.panic_with_error(Error::InvalidState);
        }

        let is_client  = caller == contract.client;
        let is_arbiter = contract.arbiter.as_ref().map_or(false, |a| &caller == a);

        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if !is_client { env.panic_with_error(Error::UnauthorizedRole); }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if !is_arbiter { env.panic_with_error(Error::UnauthorizedRole); }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if !is_client && !is_arbiter { env.panic_with_error(Error::UnauthorizedRole); }
            }
            ReleaseAuthorization::MultiSig => {
                if !is_client && !is_arbiter { env.panic_with_error(Error::UnauthorizedRole); }
            }
        }

        caller.require_auth();

        approvals::check_approvals(&env, &contract, contract_id, milestone_index)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let milestone_key = Symbol::new(&env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap();

        if milestone.released { env.panic_with_error(Error::MilestoneAlreadyReleased); }
        if milestone.refunded  { env.panic_with_error(Error::AlreadyRefunded); }

        let available =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available < milestone.amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        milestone.released = true;
        milestones.set(milestone_index, milestone);
        contract.released_amount += milestone.amount;

        approvals::clear_approvals(&env, contract_id, milestone_index);

        let all_done = milestones.iter().all(|m| m.released || m.refunded);
        if all_done { contract.status = ContractStatus::Completed; }

        env.storage()
            .persistent()
            .set(&(DataKey::Contract(contract_id), milestone_key), &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        true
    }

    // ── Refund ────────────────────────────────────────────────────────────

    /// Refund one or more unreleased milestones back to the client.
    ///
    /// All validations occur before any state change (atomic read-validate-write).
    ///
    /// # Errors
    /// - [`Error::ContractNotFound`]          — unknown `contract_id`.
    /// - [`Error::EmptyRefundRequest`]        — empty index list.
    /// - [`Error::DuplicateMilestoneInRefund`]— duplicate index in request.
    /// - [`Error::IndexOutOfBounds`]          — index out of range.
    /// - [`Error::AlreadyReleased`]           — milestone already released.
    /// - [`Error::AlreadyRefunded`]           — milestone already refunded.
    /// - [`Error::InsufficientFunds`]         — contract balance too low.
    pub fn refund_unreleased_milestones(
        env: Env,
        contract_id: u32,
        milestone_indices: Vec<u32>,
    ) -> i128 {
        if milestone_indices.is_empty() {
            env.panic_with_error(Error::EmptyRefundRequest);
        }

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

        contract.client.require_auth();

        let milestone_key = Symbol::new(&env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        let mut total_refund: i128 = 0;
        for idx in milestone_indices.iter() {
            if idx >= milestones.len() { env.panic_with_error(Error::IndexOutOfBounds); }
            let m = milestones.get(idx).unwrap();
            if m.released  { env.panic_with_error(Error::AlreadyReleased); }
            if m.refunded  { env.panic_with_error(Error::AlreadyRefunded); }
            total_refund += m.amount;
        }

        let available =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available < total_refund {
            env.panic_with_error(Error::InsufficientFunds);
        }

        for idx in milestone_indices.iter() {
            let mut m = milestones.get(idx).unwrap();
            m.refunded = true;
            milestones.set(idx, m);
        }
        contract.refunded_amount += total_refund;

        let all_done = milestones.iter().all(|m| m.released || m.refunded);
        if all_done {
            if milestones.iter().all(|m| m.refunded) {
                contract.status = ContractStatus::Refunded;
            } else {
                contract.status = ContractStatus::Completed;
            }
        }

        env.storage()
            .persistent()
            .set(&(DataKey::Contract(contract_id), milestone_key), &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        total_refund
    }

    // ── Read-only queries ─────────────────────────────────────────────────

    /// Return contract metadata for the given ID.
    ///
    /// # Errors
    /// - [`Error::ContractNotFound`] — unknown `contract_id`.
    pub fn get_contract(env: Env, contract_id: u32) -> Contract {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
    }

    /// Return all milestones for the given contract.
    ///
    /// # Errors
    /// - [`Error::ContractNotFound`] — unknown `contract_id` or missing milestone key.
    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<Milestone> {
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
    }

    /// Return funded balance not yet released or refunded.
    ///
    /// # Errors
    /// - [`Error::ContractNotFound`] — unknown `contract_id`.
    pub fn get_refundable_balance(env: Env, contract_id: u32) -> i128 {
        let c: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        c.funded_amount - c.released_amount - c.refunded_amount
    }

    /// Return current approval record for a milestone, or `None` if absent / expired.
    pub fn get_milestone_approvals(
        env: Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<MilestoneApprovals> {
        let key = DataKey::MilestoneApprovals(contract_id, milestone_index);
        env.storage().temporary().get(&key)
    }
}

// ─── Test modules ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod proptest;
#[cfg(test)]
mod simple_amount_test;
#[cfg(test)]
mod test;
