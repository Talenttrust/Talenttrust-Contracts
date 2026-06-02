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
    Contract, ContractStatus, DataKey, Error, Milestone, MilestoneApprovals, ReleaseAuthorization,
};
pub use amount_validation::{
    AmountValidationError, safe_add_amounts, safe_subtract_amounts,
    validate_single_amount, validate_milestone_amounts, validate_deposit_amount,
};

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

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
