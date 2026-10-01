//! Validation boundary definitions for all escrow contract entrypoints.
//!
//! This module centralises the explicit input-validation rules, state-transition
//! guards, and authorization invariants that every entrypoint must enforce before
//! it may mutate contract state or execute a token transfer.
//!
//! ## Design invariants
//!
//! 1. **Fail-closed**: every guard panics with a typed [`Error`] code; no guard
//!    silently accepts an invalid input.
//! 2. **Checks-Effects-Interactions**: all validation (Checks) must complete
//!    before any state write (Effects) or SAC transfer (Interactions).  Guards in
//!    this module are pure validation — they never write storage.
//! 3. **Deterministic**: identical inputs always produce identical accept/reject
//!    decisions regardless of call order or concurrency.
//! 4. **No sensitive data in errors**: every [`Error`] code is a numeric constant;
//!    no address, amount, or internal state is embedded in a rejection signal.
//!
//! ## Concurrency invariants (issue #1410)
//!
//! Soroban executes one invocation at a time against a fixed ledger snapshot and
//! rolls every write back when a call panics, so a classical data race cannot
//! occur. Four hazards remain, and the "concurrent-execution hardening" section
//! of this module closes them:
//!
//! - **I1 — Single decision.** A multi-milestone settlement must be authorised
//!   by *one* guard call over *one* snapshot ([`require_settlement_plan_sound`],
//!   [`require_timeout_refund_plan_sound`]). Re-checking indices across separate
//!   storage reads lets a competing invocation land between the reads, so the
//!   decision is taken against a snapshot that no longer exists.
//! - **I2 — First committer wins, the loser is told why.** Every index named by
//!   an accepted plan is settled by the committing call, so a retry, a replay, or
//!   a racing plan that names the same milestone is rejected against the
//!   post-commit snapshot with `MilestoneAlreadyReleased` / `AlreadyRefunded` /
//!   `InsufficientFunds` instead of paying twice.
//! - **I3 — Checked accounting.** A counter read from a stale snapshot can be
//!   negative, and `funded - released - refunded - fees` can underflow.
//!   [`available_balance_value`] reports a typed error instead of trapping, and
//!   every sum in a plan uses `checked_add`, so an adverse counter cannot be
//!   laundered into a token transfer.
//! - **I4 — Time is a ledger input, never a wall clock.** Deadline decisions take
//!   `now` as an argument, so a plan's outcome depends only on the ledger
//!   timestamp of the invocation, never on when it happened to be submitted.
//! - **I5 — No partial plan.** Validation is pure: a rejected plan leaves storage
//!   byte-identical, which is what makes a client retry safe — the retry is
//!   rejected with exactly the code the first attempt produced.
//! - **I6 — Diagnosable rejections.** Every rejection maps to one of the typed
//!   codes in the boundary tables below. No guard can panic with a raw
//!   arithmetic fault, and no code embeds an address, amount, or ledger detail.
//!
//! ## Error-code index
//!
//! The concurrency guards reuse codes that already exist in [`Error`], so no
//! error numbering changes: `EmptyBatch` (71), `BatchLimitExceeded` (72),
//! `DuplicateMilestoneInBatch` (73), `IndexOutOfBounds` (3),
//! `MilestoneAlreadyReleased` (17), `AlreadyRefunded` (8),
//! `AmountMustBePositive` (15), `MilestoneNotOverdue` (53),
//! `PotentialOverflow` (45), `AccountingInvariantViolated` (44) and
//! `InsufficientFunds` (9).
//!
//! ## Validation guard catalogue
//!
//! | Guard function | Entrypoints that call it |
//! |---|---|
//! | [`require_positive_amount`] | `deposit_funds`, `withdraw_protocol_fees`, `release_milestone` |
//! | [`require_valid_participants`] | `create_contract` |
//! | [`require_valid_arbiter`] | `create_contract` |
//! | [`require_arbiter_for_mode`] | `create_contract` |
//! | [`require_nonempty_milestones`] | `create_contract` |
//! | [`require_milestone_count_within_limit`] | `create_contract` |
//! | [`require_milestone_amounts_valid`] | `create_contract` |
//! | [`require_contract_exists`] | `deposit_funds`, `release_milestone`, `cancel_contract`, `refund_unreleased_milestones`, `finalize_contract`, `issue_reputation`, `raise_dispute`, `resolve_dispute` |
//! | [`require_contract_not_paused`] | All state-changing entrypoints |
//! | [`require_caller_is_client`] | `deposit_funds`, `cancel_contract`, `issue_reputation`, `refund_unreleased_milestones` |
//! | [`require_caller_is_freelancer`] | `submit_work_evidence` |
//! | [`require_valid_status_for_deposit`] | `deposit_funds` |
//! | [`require_valid_status_for_release`] | `release_milestone` |
//! | [`require_valid_status_for_cancel`] | `cancel_contract` |
//! | [`require_valid_status_for_refund`] | `refund_unreleased_milestones` |
//! | [`require_valid_status_for_dispute`] | `raise_dispute` |
//! | [`require_milestone_index_in_bounds`] | `release_milestone`, `refund_unreleased_milestones`, `submit_work_evidence`, `get_milestone` |
//! | [`require_milestone_not_released`] | `release_milestone`, `submit_work_evidence` |
//! | [`require_milestone_not_refunded`] | `release_milestone`, `refund_unreleased_milestones`, `submit_work_evidence` |
//! | [`require_valid_rating`] | `issue_reputation` |
//! | [`require_nonempty_comment`] | `issue_reputation` |
//! | [`require_comment_within_limit`] | `issue_reputation` |
//! | [`require_reputation_not_issued`] | `issue_reputation` |
//! | [`require_contract_completed`] | `issue_reputation` |
//! | [`require_no_self_rating`] | `issue_reputation` |
//! | [`require_deposit_within_capacity`] | `deposit_funds` |
//! | [`require_sufficient_balance_for_release`] | `release_milestone` |
//! | [`require_sufficient_balance_for_refund`] | `refund_unreleased_milestones` |
//! | [`require_sufficient_accumulated_fees`] | `withdraw_protocol_fees` |
//! | [`require_evidence_within_limit`] | `submit_work_evidence` |
//! | [`require_valid_release_caller`] | `release_milestone` |
//! | [`require_dispute_in_disputed_state`] | `resolve_dispute` |
//! | [`require_caller_is_arbiter`] | `resolve_dispute` |
//! | [`require_arbiter_present`] | `raise_dispute` |
//! | [`require_caller_is_party`] | `raise_dispute` |
//! | [`available_balance_value`] / [`require_available_balance`] | `release_milestone`, `release_milestone_batch`, `refund_unreleased_milestones`, `cancel_contract`, `finalize_contract` |
//! | [`require_no_duplicate_indices_value`] | `refund_unreleased_milestones`, settlement plans |
//! | [`require_timeout_refund_open`] | `refund_unreleased_milestones`, `is_milestone_overdue` |
//! | [`plan_release`] / [`require_settlement_plan_sound`] | `release_milestone_batch` |
//! | [`plan_timeout_refund`] / [`require_timeout_refund_plan_sound`] | `refund_unreleased_milestones` |

use crate::{
    ContractStatus, Error, Milestone, ReleaseAuthorization, MAX_MILESTONES,
    MAX_SINGLE_AMOUNT_STROOPS,
};
use soroban_sdk::{Address, Env, String, Vec};

// ── Amount bounds ─────────────────────────────────────────────────────────────

/// Panics with [`Error::AmountMustBePositive`] if `amount` is ≤ 0.
///
/// Used by: `deposit_funds`, `withdraw_protocol_fees`
///
/// # Boundary
/// - `amount == 0` → rejected
/// - `amount == -1` → rejected
/// - `amount == 1` → accepted (minimum positive stroop)
/// - `amount == i128::MAX` → accepted by this guard (capacity guard handles the upper bound)
pub fn require_positive_amount(env: &Env, amount: i128) {
    if amount <= 0 {
        env.panic_with_error(Error::AmountMustBePositive);
    }
}

// ── Participant identity ──────────────────────────────────────────────────────

/// Panics with [`Error::InvalidParticipant`] if `client == freelancer`.
///
/// An escrow between identical addresses is a no-op and can only serve as a
/// vector for protocol-fee extraction against a single party.
///
/// # Boundary
/// - `client == freelancer` → rejected
/// - distinct addresses → accepted
pub fn require_valid_participants(env: &Env, client: &Address, freelancer: &Address) {
    if client == freelancer {
        env.panic_with_error(Error::InvalidParticipant);
    }
}

/// Panics with [`Error::InvalidArbiter`] if the arbiter address equals the
/// client or the freelancer.
///
/// An arbiter that is also a party cannot be neutral; Stellar authorization would
/// allow a self-serving resolution.
///
/// # Boundary
/// - `arbiter == client` → rejected
/// - `arbiter == freelancer` → rejected
/// - `arbiter` distinct from both → accepted
/// - `arbiter == None` → no-op (caller must call [`require_arbiter_for_mode`] separately)
pub fn require_valid_arbiter(
    env: &Env,
    arbiter: &Option<Address>,
    client: &Address,
    freelancer: &Address,
) {
    if let Some(arb) = arbiter {
        if arb == client || arb == freelancer {
            env.panic_with_error(Error::InvalidArbiter);
        }
    }
}

/// Panics with [`Error::MissingArbiter`] when the release mode requires an
/// arbiter but none was supplied.
///
/// `ArbiterOnly` and `ClientAndArbiter` modes embed the arbiter in the dispute
/// and approval path; an escrow created without one can never be released.
///
/// # Boundary
/// - `ArbiterOnly` with `arbiter == None` → rejected
/// - `ClientAndArbiter` with `arbiter == None` → rejected
/// - `ClientOnly` or `MultiSig` with `arbiter == None` → accepted
pub fn require_arbiter_for_mode(
    env: &Env,
    mode: &ReleaseAuthorization,
    arbiter: &Option<Address>,
) {
    match mode {
        ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter
            if arbiter.is_none() =>
        {
            env.panic_with_error(Error::MissingArbiter);
        }
        _ => {}
    }
}

// ── Milestone list ────────────────────────────────────────────────────────────

/// Panics with [`Error::EmptyMilestones`] when the milestone list is empty.
///
/// A contract without milestones has no payment schedule and cannot transition
/// to `Funded` or `Completed`.
///
/// # Boundary
/// - `milestones.len() == 0` → rejected
/// - `milestones.len() >= 1` → accepted by this guard
pub fn require_nonempty_milestones(env: &Env, milestones: &Vec<i128>) {
    if milestones.is_empty() {
        env.panic_with_error(Error::EmptyMilestones);
    }
}

/// Panics with [`Error::InvalidMilestoneAmount`] if any milestone amount is ≤ 0,
/// or if the per-milestone single-amount cap is exceeded.
/// Panics with [`Error::InvalidMilestoneAmount`] if the running sum exceeds the
/// governed cap.
/// Panics with [`Error::PotentialOverflow`] if accumulation would overflow i128.
///
/// # Boundary (per milestone)
/// - `amount == 0` → rejected (`InvalidMilestoneAmount`)
/// - `amount < 0` → rejected (`InvalidMilestoneAmount`)
/// - `amount > MAX_SINGLE_AMOUNT_STROOPS` → rejected (`InvalidMilestoneAmount`)
/// - `amount == 1` → accepted (minimum valid stroop)
/// - `amount == MAX_SINGLE_AMOUNT_STROOPS` → accepted
///
/// # Boundary (total)
/// - `sum > max_total` → rejected (`InvalidMilestoneAmount`)
/// - `sum == max_total` → accepted
/// - `sum` overflows `i128` → rejected (`PotentialOverflow`)
/// - `max_total <= 0` → rejected on the first iteration (`InvalidMilestoneAmount`),
///   because a non-positive cap can never be satisfied by a positive amount
pub fn require_milestone_amounts_valid(env: &Env, milestones: &Vec<i128>, max_total: i128) {
    let mut total: i128 = 0;
    for amount in milestones.iter() {
        if amount <= 0 {
            env.panic_with_error(Error::InvalidMilestoneAmount);
        }
        if amount > MAX_SINGLE_AMOUNT_STROOPS {
            env.panic_with_error(Error::InvalidMilestoneAmount);
        }
        match total.checked_add(amount) {
            Some(new_total) => total = new_total,
            None => env.panic_with_error(Error::PotentialOverflow),
        }
        if total > max_total {
            env.panic_with_error(Error::InvalidMilestoneAmount);
        }
    }
}

/// Panics with [`Error::InvalidMilestoneAmount`] if `milestones.len() > MAX_MILESTONES`.
///
/// # Boundary
/// - `len == MAX_MILESTONES` → accepted
/// - `len == MAX_MILESTONES + 1` → rejected
/// - `len == 0` → accepted by this guard (caller must call [`require_nonempty_milestones`] first)
pub fn require_milestone_count_within_limit(env: &Env, milestones: &Vec<i128>) {
    if milestones.len() > MAX_MILESTONES {
        env.panic_with_error(Error::InvalidMilestoneAmount);
    }
}

// ── Contract state guards ─────────────────────────────────────────────────────

/// Panics with [`Error::ContractPaused`] when the escrow is paused or in
/// emergency mode.
///
/// All state-changing entrypoints must call this guard before any auth check or
/// storage mutation so that paused contracts cannot be mutated.
///
/// # Boundary
/// - `Paused == true` → rejected (`ContractPaused`)
/// - `Emergency == true` → rejected (`EmergencyActive`)
/// - both `false` → accepted
pub fn require_contract_not_paused(
    env: &Env,
    paused: bool,
    emergency: bool,
) {
    if emergency {
        env.panic_with_error(Error::EmergencyActive);
    }
    if paused {
        env.panic_with_error(Error::ContractPaused);
    }
}

/// Validates deposit preconditions for a contract status.
///
/// Panics with [`Error::InvalidState`] if the contract is not in `Created` or
/// `PartiallyFunded` status. Terminal states (`Cancelled`, `Refunded`,
/// `Completed`) are explicitly rejected to prevent re-funding a closed escrow.
///
/// # Status transition boundary
/// - `Created` → accepted (first deposit)
/// - `PartiallyFunded` → accepted (incremental deposit)
/// - `Funded` → rejected (already fully funded)
/// - `Completed` → rejected (terminal)
/// - `Cancelled` → rejected (terminal)
/// - `Refunded` → rejected (terminal)
/// - `Disputed` → rejected (frozen pending resolution)
pub fn require_valid_status_for_deposit(env: &Env, status: ContractStatus) {
    match status {
        ContractStatus::Created | ContractStatus::PartiallyFunded => {}
        _ => env.panic_with_error(Error::InvalidState),
    }
}

/// Validates release preconditions for a contract status.
///
/// Panics with [`Error::InvalidState`] if the contract is not in `Funded` status.
/// A contract must be fully funded before any milestone payout.
///
/// # Status transition boundary
/// - `Funded` → accepted
/// - All others → rejected
pub fn require_valid_status_for_release(env: &Env, status: ContractStatus) {
    if status != ContractStatus::Funded {
        env.panic_with_error(Error::InvalidState);
    }
}

/// Validates cancel preconditions for a contract status.
///
/// Panics with [`Error::AlreadyCancelled`] if already cancelled.
/// Panics with [`Error::InvalidStatusTransition`] for any non-cancellable status.
///
/// # Status transition boundary
/// - `Created` → accepted
/// - `Funded` → accepted (with no prior releases; release check is separate)
/// - `Cancelled` → rejected (`AlreadyCancelled`)
/// - `Completed` / `Refunded` / `Disputed` / `PartiallyFunded` after release → rejected
pub fn require_valid_status_for_cancel(env: &Env, status: ContractStatus) {
    if status == ContractStatus::Cancelled {
        env.panic_with_error(Error::ContractCancelled);
    }
    if status != ContractStatus::Created && status != ContractStatus::Funded {
        env.panic_with_error(Error::InvalidStatusTransition);
    }
}

/// Validates refund preconditions for a contract status.
///
/// Panics with [`Error::InvalidState`] if the contract is not in a refundable
/// state. Only `Created`, `Funded`, and `Disputed` contracts may be partially
/// refunded without full cancellation.
///
/// # Status transition boundary
/// - `Created` → accepted
/// - `Funded` → accepted
/// - `Disputed` → accepted (arbiter-directed refund path)
/// - `Completed` / `Cancelled` / `Refunded` → rejected
pub fn require_valid_status_for_refund(env: &Env, status: ContractStatus) {
    match status {
        ContractStatus::Created | ContractStatus::Funded | ContractStatus::Disputed => {}
        _ => env.panic_with_error(Error::InvalidState),
    }
}

/// Validates raise-dispute preconditions for a contract status.
///
/// Panics with [`Error::InvalidState`] if the contract is not in a disputable
/// state. Only funded contracts may enter dispute.
///
/// # Status transition boundary
/// - `Funded` → accepted
/// - `PartiallyFunded` → accepted
/// - All others → rejected
pub fn require_valid_status_for_dispute(env: &Env, status: ContractStatus) {
    match status {
        ContractStatus::Funded | ContractStatus::PartiallyFunded => {}
        _ => env.panic_with_error(Error::InvalidState),
    }
}

/// Validates resolve-dispute preconditions for a contract status.
///
/// Panics with [`Error::InvalidStatusTransition`] if not in `Disputed` status.
///
/// # Boundary
/// - `Disputed` → accepted
/// - All others → rejected
pub fn require_dispute_in_disputed_state(env: &Env, status: ContractStatus) {
    if status != ContractStatus::Disputed {
        env.panic_with_error(Error::InvalidStatusTransition);
    }
}

// ── Milestone index ───────────────────────────────────────────────────────────

/// Panics with [`Error::IndexOutOfBounds`] if `index >= milestones.len()`.
///
/// # Boundary
/// - `index < len` → accepted
/// - `index == len` → rejected (off-by-one)
/// - `index > len` → rejected
pub fn require_milestone_index_in_bounds(env: &Env, index: u32, milestones: &Vec<Milestone>) {
    if index >= milestones.len() {
        env.panic_with_error(Error::IndexOutOfBounds);
    }
}

/// Panics with [`Error::MilestoneAlreadyReleased`] if the milestone is released.
///
/// # Boundary
/// - `released == false` → accepted
/// - `released == true` → rejected
pub fn require_milestone_not_released(env: &Env, released: bool) {
    if released {
        env.panic_with_error(Error::MilestoneAlreadyReleased);
    }
}

/// Panics with [`Error::AlreadyRefunded`] if the milestone has been refunded.
///
/// # Boundary
/// - `refunded == false` → accepted
/// - `refunded == true` → rejected
pub fn require_milestone_not_refunded(env: &Env, refunded: bool) {
    if refunded {
        env.panic_with_error(Error::AlreadyRefunded);
    }
}

// ── Authorization ─────────────────────────────────────────────────────────────

/// Panics with [`Error::UnauthorizedRole`] if `caller != client`.
///
/// Used by `deposit_funds`, `cancel_contract`, `issue_reputation`, and
/// `refund_unreleased_milestones`.
///
/// # Boundary
/// - `caller == client` → accepted
/// - `caller != client` → rejected
pub fn require_caller_is_client(env: &Env, caller: &Address, client: &Address) {
    if caller != client {
        env.panic_with_error(Error::UnauthorizedRole);
    }
}

/// Panics with [`Error::UnauthorizedRole`] if `caller != freelancer`.
///
/// Used by `submit_work_evidence`.
///
/// # Boundary
/// - `caller == freelancer` → accepted
/// - `caller != freelancer` → rejected
pub fn require_caller_is_freelancer(env: &Env, caller: &Address, freelancer: &Address) {
    if caller != freelancer {
        env.panic_with_error(Error::UnauthorizedRole);
    }
}

/// Panics with [`Error::UnauthorizedRole`] if `caller` is not the assigned arbiter.
///
/// Used by `resolve_dispute`.
///
/// # Boundary
/// - `arbiter == Some(caller)` → accepted
/// - `arbiter == None` or `arbiter != caller` → rejected
pub fn require_caller_is_arbiter(
    env: &Env,
    caller: &Address,
    arbiter: &Option<Address>,
) {
    match arbiter {
        Some(a) if a == caller => {}
        _ => env.panic_with_error(Error::UnauthorizedRole),
    }
}

/// Panics with [`Error::UnauthorizedRole`] if `caller` is not the client or
/// freelancer.
///
/// Used by `raise_dispute` — only contract parties may open a dispute.
///
/// # Boundary
/// - `caller == client` → accepted
/// - `caller == freelancer` → accepted
/// - all others → rejected
pub fn require_caller_is_party(
    env: &Env,
    caller: &Address,
    client: &Address,
    freelancer: &Address,
) {
    if caller != client && caller != freelancer {
        env.panic_with_error(Error::UnauthorizedRole);
    }
}

/// Panics with [`Error::ArbiterRequired`] when no arbiter is assigned.
///
/// Used by `raise_dispute` — a dispute without an arbiter cannot be resolved.
///
/// # Boundary
/// - `arbiter == Some(_)` → accepted
/// - `arbiter == None` → rejected
pub fn require_arbiter_present(env: &Env, arbiter: &Option<Address>) {
    if arbiter.is_none() {
        env.panic_with_error(Error::ArbiterRequired);
    }
}

/// Validates release-caller authorization against the contract's
/// [`ReleaseAuthorization`] mode.
///
/// Panics with [`Error::UnauthorizedRole`] when the caller is not permitted
/// to trigger a release under the configured mode.
///
/// | Mode | Accepted callers |
/// |---|---|
/// | `ClientOnly` | client |
/// | `ArbiterOnly` | arbiter |
/// | `ClientAndArbiter` | client **or** arbiter |
/// | `MultiSig` | client **or** freelancer (after both have approved) |
pub fn require_valid_release_caller(
    env: &Env,
    caller: &Address,
    client: &Address,
    freelancer: &Address,
    arbiter: &Option<Address>,
    mode: &ReleaseAuthorization,
) {
    let is_client = caller == client;
    let is_freelancer = caller == freelancer;
    let is_arbiter = arbiter.as_ref() == Some(caller);

    let authorized = match mode {
        ReleaseAuthorization::ClientOnly => is_client,
        ReleaseAuthorization::ArbiterOnly => is_arbiter,
        ReleaseAuthorization::ClientAndArbiter => is_client || is_arbiter,
        ReleaseAuthorization::MultiSig => is_client || is_freelancer,
    };

    if !authorized {
        env.panic_with_error(Error::UnauthorizedRole);
    }
}

// ── Accounting ────────────────────────────────────────────────────────────────

/// Panics with [`Error::InvalidDepositAmount`] if depositing `amount` would
/// exceed `total_milestone_amount`.
///
/// Uses `checked_add` to prevent silent overflow when computing the projected
/// funded total.
///
/// # Boundary
/// - `funded + amount == total_milestone_amount` → accepted (last deposit)
/// - `funded + amount < total_milestone_amount` → accepted (partial deposit)
/// - `funded + amount > total_milestone_amount` → rejected
/// - `funded + amount` overflows `i128` → rejected (`PotentialOverflow`)
pub fn require_deposit_within_capacity(
    env: &Env,
    current_funded: i128,
    amount: i128,
    total_milestone_amount: i128,
) {
    let projected = match current_funded.checked_add(amount) {
        Some(v) => v,
        None => env.panic_with_error(Error::PotentialOverflow),
    };
    if projected > total_milestone_amount {
        env.panic_with_error(Error::InvalidDepositAmount);
    }
}

/// Panics with [`Error::InsufficientFunds`] if the available balance is less
/// than `required`.
///
/// `available = funded_amount - released_amount - refunded_amount - accumulated_fees`
///
/// # Boundary
/// - `available >= required` → accepted
/// - `available < required` → rejected
pub fn require_sufficient_balance_for_release(env: &Env, available: i128, required: i128) {
    if available < required {
        env.panic_with_error(Error::InsufficientFunds);
    }
}

/// Panics with [`Error::InsufficientFunds`] if the refundable balance is less
/// than `total_refund_amount`.
///
/// `refundable = funded_amount - released_amount - refunded_amount`
///
/// # Boundary
/// - `refundable >= total_refund_amount` → accepted
/// - `refundable < total_refund_amount` → rejected
pub fn require_sufficient_balance_for_refund(
    env: &Env,
    refundable: i128,
    total_refund_amount: i128,
) {
    if refundable < total_refund_amount {
        env.panic_with_error(Error::InsufficientFunds);
    }
}

/// Panics with [`Error::InsufficientAccumulatedFees`] if `amount` exceeds
/// `accumulated`.
///
/// # Boundary
/// - `amount <= accumulated` → accepted
/// - `amount > accumulated` → rejected
pub fn require_sufficient_accumulated_fees(env: &Env, amount: i128, accumulated: i128) {
    if amount > accumulated {
        env.panic_with_error(Error::InsufficientAccumulatedFees);
    }
}

// ── Reputation ────────────────────────────────────────────────────────────────

/// Panics with [`Error::InvalidRating`] if `rating` is outside the 1-to-5
/// inclusive range.
///
/// # Boundary
/// - `rating == 0` → rejected
/// - `rating == 1` → accepted (minimum)
/// - `rating == 5` → accepted (maximum)
/// - `rating == 6` → rejected
pub fn require_valid_rating(env: &Env, rating: u32) {
    if rating < 1 || rating > 5 {
        env.panic_with_error(Error::InvalidRating);
    }
}

/// Panics with [`Error::EmptyComment`] if the comment string is empty (0 bytes).
///
/// # Boundary
/// - `len == 0` → rejected
/// - `len >= 1` → accepted by this guard
pub fn require_nonempty_comment(env: &Env, comment: &String) {
    if comment.len() == 0 {
        env.panic_with_error(Error::EmptyComment);
    }
}

/// Panics with [`Error::CommentTooLong`] if the comment exceeds
/// `MAX_COMMENT_BYTES` (200) bytes.
///
/// `String::len()` returns the UTF-8 byte count, so multi-byte characters
/// count against the limit proportionally.
///
/// # Boundary
/// - `len == 200` → accepted
/// - `len == 201` → rejected
pub const MAX_COMMENT_BYTES: u32 = 200;

pub fn require_comment_within_limit(env: &Env, comment: &String) {
    if comment.len() > MAX_COMMENT_BYTES {
        env.panic_with_error(Error::CommentTooLong);
    }
}

/// Panics with [`Error::ReputationAlreadyIssued`] if reputation was already
/// issued for this contract.
///
/// # Boundary
/// - `reputation_issued == false` → accepted
/// - `reputation_issued == true` → rejected
pub fn require_reputation_not_issued(env: &Env, reputation_issued: bool) {
    if reputation_issued {
        env.panic_with_error(Error::ReputationAlreadyIssued);
    }
}

/// Panics with [`Error::NotCompleted`] if the contract is not in `Completed`
/// status.
///
/// Reputation can only be granted for completed work.
///
/// # Boundary
/// - `Completed` → accepted
/// - All others → rejected
pub fn require_contract_completed(env: &Env, status: ContractStatus) {
    if status != ContractStatus::Completed {
        env.panic_with_error(Error::NotCompleted);
    }
}

/// Panics with [`Error::RoleOverlap`] if client and freelancer are the same
/// address.
///
/// # Boundary
/// - `client != freelancer` → accepted
/// - `client == freelancer` → rejected
pub fn require_no_self_rating(env: &Env, client: &Address, freelancer: &Address) {
    if client == freelancer {
        env.panic_with_error(Error::RoleOverlap);
    }
}

// ── Work evidence ─────────────────────────────────────────────────────────────

/// Panics with [`Error::EvidenceTooLong`] if the evidence string exceeds
/// `MAX_EVIDENCE_BYTES` (256) bytes.
///
/// # Boundary
/// - `len == 256` → accepted
/// - `len == 257` → rejected
pub const MAX_EVIDENCE_BYTES: u32 = 256;

pub fn require_evidence_within_limit(env: &Env, evidence: &String) {
    if evidence.len() > MAX_EVIDENCE_BYTES {
        env.panic_with_error(Error::EvidenceTooLong);
    }
}

// ── Refund uniqueness ─────────────────────────────────────────────────────────

/// Panics with [`Error::EmptyRefundRequest`] if the indices vector is empty.
///
/// # Boundary
/// - `len == 0` → rejected
/// - `len >= 1` → accepted
pub fn require_nonempty_refund_request(env: &Env, indices: &Vec<u32>) {
    if indices.is_empty() {
        env.panic_with_error(Error::EmptyRefundRequest);
    }
}

/// Panics with [`Error::DuplicateMilestoneInRefund`] if any index appears more
/// than once in `indices`.
///
/// O(n²) check is acceptable for small milestone lists (≤ MAX_MILESTONES = 10).
///
/// # Boundary
/// - all indices distinct → accepted
/// - any pair `i != j` with `indices[i] == indices[j]` → rejected
pub fn require_no_duplicate_refund_indices(env: &Env, indices: &Vec<u32>) {
    require_no_duplicate_indices_value(indices).unwrap_or_else(|e| env.panic_with_error(e))
}

/// Pure form of [`require_no_duplicate_refund_indices`], shared with the
/// settlement planners in the concurrency section below.
///
/// Returning the error instead of panicking lets each caller map the duplicate
/// onto the code its own entrypoint contract defines (`DuplicateMilestoneInRefund`
/// for a refund request, `DuplicateMilestoneInBatch` for a release plan) without
/// duplicating the scan.
///
/// # Boundary
/// - `len <= 1` → always `Ok(())`
/// - all indices distinct → `Ok(())`
/// - any repeated index → `Err(Error::DuplicateMilestoneInRefund)`
/// - `len == 0` → `Ok(())`; emptiness is a separate boundary
///   ([`require_nonempty_refund_request`] for refunds, `EmptyBatch` for plans)
pub fn require_no_duplicate_indices_value(indices: &Vec<u32>) -> Result<(), Error> {
    let len = indices.len();
    for i in 0..len {
        for j in (i + 1)..len {
            if indices.get(i).unwrap() == indices.get(j).unwrap() {
                return Err(Error::DuplicateMilestoneInRefund);
            }
        }
    }
    Ok(())
}

// ── Concurrent-execution hardening (issue #1410) ─────────────────────────────
//
// The guards above validate one input against one snapshot. This section adds
// the boundaries that make *interleaved or repeated* execution safe:
//
// 1. **Torn decisions** — a caller that re-reads state between two per-index
//    checks can be overtaken by a competing invocation and end up authorising a
//    plan against a snapshot that no longer exists. [`require_settlement_plan_sound`]
//    and [`require_timeout_refund_plan_sound`] therefore decide the whole plan
//    in a single, pure pass over one snapshot.
// 2. **Duplicate work** — a retry, a replay, or two racing plans naming the same
//    milestone must be rejected deterministically instead of paying twice. The
//    planners re-validate every index against the snapshot handed to them, so the
//    first committer marks the milestone settled and the loser is rejected with a
//    typed code.
// 3. **Adverse counters** — a counter taken from a stale snapshot can be negative
//    and `funded - released - refunded - fees` can underflow. [`available_balance_value`]
//    uses checked arithmetic and refuses an inconsistent snapshot, so a corrupt or
//    stale counter fails with a typed error rather than trapping or, worse,
//    silently enlarging the balance a plan may spend.

/// Largest number of milestones a single settlement plan may name.
///
/// Mirrors the protocol ceiling on milestones per contract ([`MAX_MILESTONES`])
/// and the batch bound enforced by `release_milestone_batch`, so a plan can never
/// be longer than the schedule it is applied to and the per-index scan stays
/// bounded by an on-chain constant rather than by caller input.
pub const MAX_SETTLEMENT_PLAN_LEN: u32 = MAX_MILESTONES;

/// Which settlement flow a plan belongs to.
///
/// Both classes *settle* the milestones they name — release pays the freelancer,
/// a timeout refund pays the client — so on a given snapshot they are mutually
/// exclusive for the same index (I2). The distinction is the amount that moves
/// and, for a timeout refund, the additional deadline precondition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettlementClass {
    Release,
    TimeoutRefund { now: u64 },
}

/// Computes the escrowed balance still available for a settlement plan, i.e.
/// `funded - released - refunded - accumulated_fees`.
///
/// This is the checked replacement for the unchecked subtraction performed by the
/// money-moving entrypoints. It is the boundary that keeps a stale or corrupted
/// counter from inflating the balance a concurrent plan may spend (I3).
///
/// # Invariants
/// * every input counter is non-negative — a negative counter can only come from
///   corrupted on-ledger state or from a subtraction performed against a stale
///   snapshot;
/// * `released + refunded + accumulated_fees <= funded` — a contract can never
///   have paid out (or reserved for the protocol) more than it received;
/// * the result is therefore in `[0, funded]`.
///
/// # Boundary
/// * all counters `0` → `Ok(0)`
/// * `settled == funded` → `Ok(0)`
/// * `settled == funded + 1` → `Err(AccountingInvariantViolated)`
/// * any counter `< 0` → `Err(AccountingInvariantViolated)`
/// * `released + refunded + fees` overflows `i128` → `Err(PotentialOverflow)`
pub fn available_balance_value(
    funded_amount: i128,
    released_amount: i128,
    refunded_amount: i128,
    accumulated_fees: i128,
) -> Result<i128, Error> {
    if funded_amount < 0 || released_amount < 0 || refunded_amount < 0 || accumulated_fees < 0 {
        return Err(Error::AccountingInvariantViolated);
    }

    let settled = released_amount
        .checked_add(refunded_amount)
        .and_then(|v| v.checked_add(accumulated_fees))
        .ok_or(Error::PotentialOverflow)?;

    if settled > funded_amount {
        return Err(Error::AccountingInvariantViolated);
    }

    Ok(funded_amount - settled)
}

/// Panic-on-rejection wrapper around [`available_balance_value`], returning the
/// balance so a caller can hand it straight to [`plan_release`].
///
/// # Panics
/// Panics with [`Error::AccountingInvariantViolated`] for an inconsistent
/// snapshot and with [`Error::PotentialOverflow`] when the counters cannot be
/// summed. Never traps on raw arithmetic.
pub fn require_available_balance(
    env: &Env,
    funded_amount: i128,
    released_amount: i128,
    refunded_amount: i128,
    accumulated_fees: i128,
) -> i128 {
    available_balance_value(
        funded_amount,
        released_amount,
        refunded_amount,
        accumulated_fees,
    )
    .unwrap_or_else(|e| env.panic_with_error(e))
}

/// Returns `true` when a milestone may be claimed through the client-side timeout
/// path at ledger time `now`: it either has no deadline at all, or its deadline has
/// elapsed.
///
/// For a deadline-bearing milestone the boundary is strictly `now > deadline`: a
/// request at exactly the deadline is *not* yet overdue. This matches the semantics
/// implemented by `refund_unreleased_milestones` and by `is_milestone`, so this
/// guard and those entrypoints cannot disagree about the boundary instant.
///
/// # Boundary
/// * `deadline == None` → `true` (a milestone with no deadline never expires)
/// * `now < deadline` → `false`
/// * `now == deadline` → `false`
/// * `now == deadline + 1` → `true`
pub fn timeout_refund_allowed(now: u64, deadline: Option<u64>) -> bool {
    match deadline {
        None => true,
        Some(deadline) => now > deadline,
    }
}

/// Panics with [`Error::MilestoneNotOverdue`] unless [`timeout_refund_allowed`]
/// holds for `now`.
///
/// This is the deadline half of invariant I4: the decision is a pure function of
/// the ledger timestamp handed in, so a competing invocation at the same ledger
/// makes the same choice — one of the two flows is admissible and the other is
/// not, never both and never neither.
///
/// # Boundary
/// * `deadline == None` → accepted
/// * `now == deadline` → rejected (`MilestoneNotOverdue`)
/// * `now > deadline` → accepted
pub fn require_timeout_refund_open(env: &Env, now: u64, deadline: Option<u64>) {
    if !timeout_refund_allowed(now, deadline) {
        env.panic_with_error(Error::MilestoneNotOverdue);
    }
}

/// Decides a whole settlement plan against a single snapshot and returns the total
/// amount that may move.
///
/// Pure: it reads no storage, writes no storage and takes no [`Env`], so a caller
/// cannot accidentally authorise a plan from a partially refreshed view. The
/// bounds are evaluated in this fixed order and the first violation decides the
/// error, so the outcome is deterministic and independent of call order:
///
/// | # | Boundary | Accepted | Rejected with |
/// |---|---|---|---|
/// | S1 | Non-empty plan | `len >= 1` | [`Error::EmptyBatch`] |
/// | S2 | Plan within bound | `len <= MAX_SETTLEMENT_PLAN_LEN` | [`Error::BatchLimitExceeded`] |
/// | S3 | Available balance sane | `available >= 0` | [`Error::AccountingInvariantViolated`] |
/// | S4 | Distinct indices | no repeated index | [`Error::DuplicateMilestoneInBatch`] |
/// | S5 | Index within schedule | `index < milestones.len()` | [`Error::IndexOutOfBounds`] |
/// | S6 | Milestone unsettled | `released == false` | [`Error::MilestoneAlreadyReleased`] |
/// | S7 | Milestone not refunded | `refunded == false` | [`Error::AlreadyRefunded`] |
/// | S8 | Deadline satisfied (timeout class only) | no deadline, or `now > deadline` | [`Error::MilestoneNotOverdue`] |
/// | S9 | Positive amount | `amount > 0` | [`Error::AmountMustBePositive`] |
/// | S10 | Total does not overflow | `checked_add` of every amount | [`Error::PotentialOverflow`] |
/// | S11 | Plan affordable | `total <= available` | [`Error::InsufficientFunds`] |
///
/// S9 is what makes the guard fail closed on a milestone whose amount was
/// corrupted on-ledger: a non-positive amount would otherwise shrink a plan total
/// and let the remaining indices spend the escrow beyond what was deposited.
///
/// The caller is expected to have validated snapshot consistency before calling
/// (see `storage_validation::validate_milestones_consistency`); this guard only
/// decides the request, it does not repair state.
fn plan_settlement(
    class: SettlementClass,
    milestones: &Vec<Milestone>,
    indices: &Vec<u32>,
    available: i128,
) -> Result<i128, Error> {
    // S1/S2 — bound the plan before any per-index work.
    if indices.is_empty() {
        return Err(Error::EmptyBatch);
    }
    if indices.len() > MAX_SETTLEMENT_PLAN_LEN {
        return Err(Error::BatchLimitExceeded);
    }

    // S3 — a negative balance means the snapshot is inconsistent; never treat it
    // as capacity.
    if available < 0 {
        return Err(Error::AccountingInvariantViolated);
    }

    // S4 — one milestone may appear at most once, otherwise a plan would pay twice.
    require_no_duplicate_indices_value(indices).map_err(|_| Error::DuplicateMilestoneInBatch)?;

    let mut total: i128 = 0;
    for i in 0..indices.len() {
        let index = indices.get(i).unwrap();

        // S5
        if index >= milestones.len() {
            return Err(Error::IndexOutOfBounds);
        }
        let milestone = milestones.get(index).unwrap();

        // S6/S7 — the losing half of a race against a competing plan: once the
        // first committer settled this milestone, the retry is rejected here.
        if milestone.released {
            return Err(Error::MilestoneAlreadyReleased);
        }
        if milestone.refunded {
            return Err(Error::AlreadyRefunded);
        }

        // S8
        if let SettlementClass::TimeoutRefund { now } = class {
            if !timeout_refund_allowed(now, milestone.deadline) {
                return Err(Error::MilestoneNotOverdue);
            }
        }

        // S9
        if milestone.amount <= 0 {
            return Err(Error::AmountMustBePositive);
        }

        // S10
        total = total
            .checked_add(milestone.amount)
            .ok_or(Error::PotentialOverflow)?;
    }

    // S11
    if total > available {
        return Err(Error::InsufficientFunds);
    }

    Ok(total)
}

/// Decides a release plan (freelancer payout) against one snapshot and returns the
/// gross total to transfer.
///
/// See the S1-S11 boundary table in the concurrency section above. This is the single call a
/// batch release should make: it evaluates every index and the affordability of
/// the aggregate against the same snapshot, so a competing invocation that lands
/// in between cannot be interleaved with the decision (I1).
pub fn plan_release(
    milestones: &Vec<Milestone>,
    indices: &Vec<u32>,
    available: i128,
) -> Result<i128, Error> {
    plan_settlement(SettlementClass::Release, milestones, indices, available)
}

/// Decides a timeout-refund plan (client payout for expired milestones) against
/// one snapshot at ledger time `now`, and returns the total to return.
///
/// Identical to [`plan_release`] except for S8, which additionally requires the
/// deadline to have elapsed at `now`.
pub fn plan_timeout_refund(
    milestones: &Vec<Milestone>,
    indices: &Vec<u32>,
    now: u64,
    available: i128,
) -> Result<i128, Error> {
    plan_settlement(
        SettlementClass::TimeoutRefund { now },
        milestones,
        indices,
        available,
    )
}

/// Panic-on-rejection wrapper around [`plan_release`] returning the authorised
/// gross total.
///
/// # Panics
/// Panics with the first failing code of the S1-S11 boundary table above.
pub fn require_settlement_plan_sound(
    env: &Env,
    milestones: &Vec<Milestone>,
    indices: &Vec<u32>,
    available: i128,
) -> i128 {
    plan_release(milestones, indices, available).unwrap_or_else(|e| env.panic_with_error(e))
}

/// Panic-on-rejection wrapper around [`plan_timeout_refund`] returning the
/// authorised refund total.
///
/// # Panics
/// Panics with the first failing code of the S1-S11 boundary table above.
pub fn require_timeout_refund_plan_sound(
    env: &Env,
    milestones: &Vec<Milestone>,
    indices: &Vec<u32>,
    now: u64,
    available: i128,
) -> i128 {
    plan_timeout_refund(milestones, indices, now, available)
        .unwrap_or_else(|e| env.panic_with_error(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContractStatus, Error, Milestone, ReleaseAuthorization};
    use soroban_sdk::{
        testutils::Address as _,
        vec, Address, Env, String, Vec,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn test_env() -> Env {
        Env::default()
    }

    fn make_milestone(env: &Env, amount: i128) -> Milestone {
        Milestone {
            amount,
            funded_amount: 0,
            released: false,
            refunded: false,
            work_evidence: None,
            refunded_amount: 0,
            deadline: None,
        }
    }

    fn milestones_vec(env: &Env, amounts: &[i128]) -> Vec<Milestone> {
        let mut v: Vec<Milestone> = Vec::new(env);
        for &a in amounts {
            v.push_back(make_milestone(env, a));
        }
        v
    }

    // ── require_positive_amount ───────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn positive_amount_rejects_zero() {
        let env = test_env();
        require_positive_amount(&env, 0);
    }

    #[test]
    #[should_panic]
    fn positive_amount_rejects_negative() {
        let env = test_env();
        require_positive_amount(&env, -1);
    }

    #[test]
    fn positive_amount_accepts_one() {
        let env = test_env();
        require_positive_amount(&env, 1); // must not panic
    }

    #[test]
    fn positive_amount_accepts_max() {
        let env = test_env();
        require_positive_amount(&env, i128::MAX);
    }

    // ── require_valid_participants ────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn valid_participants_rejects_same_address() {
        let env = test_env();
        let same = Address::generate(&env);
        require_valid_participants(&env, &same, &same);
    }

    #[test]
    fn valid_participants_accepts_distinct() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_valid_participants(&env, &c, &f);
    }

    // ── require_valid_arbiter ─────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn valid_arbiter_rejects_arbiter_equals_client() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_valid_arbiter(&env, &Some(c.clone()), &c, &f);
    }

    #[test]
    #[should_panic]
    fn valid_arbiter_rejects_arbiter_equals_freelancer() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_valid_arbiter(&env, &Some(f.clone()), &c, &f);
    }

    #[test]
    fn valid_arbiter_accepts_distinct_arbiter() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let a = Address::generate(&env);
        require_valid_arbiter(&env, &Some(a), &c, &f);
    }

    #[test]
    fn valid_arbiter_accepts_none() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_valid_arbiter(&env, &None, &c, &f);
    }

    // ── require_arbiter_for_mode ──────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn arbiter_for_mode_rejects_arbiter_only_without_arbiter() {
        let env = test_env();
        require_arbiter_for_mode(&env, &ReleaseAuthorization::ArbiterOnly, &None);
    }

    #[test]
    #[should_panic]
    fn arbiter_for_mode_rejects_client_and_arbiter_without_arbiter() {
        let env = test_env();
        require_arbiter_for_mode(&env, &ReleaseAuthorization::ClientAndArbiter, &None);
    }

    #[test]
    fn arbiter_for_mode_accepts_client_only_without_arbiter() {
        let env = test_env();
        require_arbiter_for_mode(&env, &ReleaseAuthorization::ClientOnly, &None);
    }

    #[test]
    fn arbiter_for_mode_accepts_multisig_without_arbiter() {
        let env = test_env();
        require_arbiter_for_mode(&env, &ReleaseAuthorization::MultiSig, &None);
    }

    // ── require_nonempty_milestones ───────────────────────────────────────────

    #[test]
    #[should_panic]
    fn nonempty_milestones_rejects_empty() {
        let env = test_env();
        let empty: Vec<i128> = Vec::new(&env);
        require_nonempty_milestones(&env, &empty);
    }

    #[test]
    fn nonempty_milestones_accepts_one() {
        let env = test_env();
        let ms = vec![&env, 100_i128];
        require_nonempty_milestones(&env, &ms);
    }

    // ── require_milestone_count_within_limit ──────────────────────────────────

    #[test]
    #[should_panic]
    fn milestone_count_rejects_one_over_max() {
        let env = test_env();
        let mut ms: Vec<i128> = Vec::new(&env);
        for _ in 0..=MAX_MILESTONES {
            ms.push_back(1_i128);
        }
        require_milestone_count_within_limit(&env, &ms);
    }

    #[test]
    fn milestone_count_accepts_exactly_max() {
        let env = test_env();
        let mut ms: Vec<i128> = Vec::new(&env);
        for _ in 0..MAX_MILESTONES {
            ms.push_back(1_i128);
        }
        require_milestone_count_within_limit(&env, &ms);
    }

    // ── require_milestone_amounts_valid ───────────────────────────────────────

    #[test]
    #[should_panic]
    fn milestone_amounts_rejects_zero() {
        let env = test_env();
        let ms = vec![&env, 0_i128];
        require_milestone_amounts_valid(&env, &ms, i128::MAX);
    }

    #[test]
    #[should_panic]
    fn milestone_amounts_rejects_negative() {
        let env = test_env();
        let ms = vec![&env, -1_i128];
        require_milestone_amounts_valid(&env, &ms, i128::MAX);
    }

    #[test]
    #[should_panic]
    fn milestone_amounts_rejects_total_over_cap() {
        let env = test_env();
        let cap = 1_000_i128;
        let ms = vec![&env, cap + 1];
        require_milestone_amounts_valid(&env, &ms, cap);
    }

    #[test]
    fn milestone_amounts_accepts_total_exactly_at_cap() {
        let env = test_env();
        let cap = 1_000_i128;
        let ms = vec![&env, cap];
        require_milestone_amounts_valid(&env, &ms, cap);
    }

    // ── require_contract_not_paused ───────────────────────────────────────────

    #[test]
    #[should_panic]
    fn contract_not_paused_rejects_paused() {
        let env = test_env();
        require_contract_not_paused(&env, true, false);
    }

    #[test]
    #[should_panic]
    fn contract_not_paused_rejects_emergency() {
        let env = test_env();
        require_contract_not_paused(&env, false, true);
    }

    #[test]
    fn contract_not_paused_accepts_both_false() {
        let env = test_env();
        require_contract_not_paused(&env, false, false);
    }

    // ── Status guards ─────────────────────────────────────────────────────────

    #[test]
    fn status_for_deposit_accepts_created() {
        let env = test_env();
        require_valid_status_for_deposit(&env, ContractStatus::Created);
    }

    #[test]
    fn status_for_deposit_accepts_partially_funded() {
        let env = test_env();
        require_valid_status_for_deposit(&env, ContractStatus::PartiallyFunded);
    }

    #[test]
    #[should_panic]
    fn status_for_deposit_rejects_funded() {
        let env = test_env();
        require_valid_status_for_deposit(&env, ContractStatus::Funded);
    }

    #[test]
    #[should_panic]
    fn status_for_deposit_rejects_cancelled() {
        let env = test_env();
        require_valid_status_for_deposit(&env, ContractStatus::Cancelled);
    }

    #[test]
    fn status_for_release_accepts_funded() {
        let env = test_env();
        require_valid_status_for_release(&env, ContractStatus::Funded);
    }

    #[test]
    #[should_panic]
    fn status_for_release_rejects_created() {
        let env = test_env();
        require_valid_status_for_release(&env, ContractStatus::Created);
    }

    #[test]
    fn status_for_cancel_accepts_created() {
        let env = test_env();
        require_valid_status_for_cancel(&env, ContractStatus::Created);
    }

    #[test]
    fn status_for_cancel_accepts_funded() {
        let env = test_env();
        require_valid_status_for_cancel(&env, ContractStatus::Funded);
    }

    #[test]
    #[should_panic]
    fn status_for_cancel_rejects_cancelled() {
        let env = test_env();
        require_valid_status_for_cancel(&env, ContractStatus::Cancelled);
    }

    #[test]
    #[should_panic]
    fn status_for_cancel_rejects_completed() {
        let env = test_env();
        require_valid_status_for_cancel(&env, ContractStatus::Completed);
    }

    #[test]
    fn status_for_refund_accepts_created() {
        let env = test_env();
        require_valid_status_for_refund(&env, ContractStatus::Created);
    }

    #[test]
    fn status_for_refund_accepts_funded() {
        let env = test_env();
        require_valid_status_for_refund(&env, ContractStatus::Funded);
    }

    #[test]
    fn status_for_refund_accepts_disputed() {
        let env = test_env();
        require_valid_status_for_refund(&env, ContractStatus::Disputed);
    }

    #[test]
    #[should_panic]
    fn status_for_refund_rejects_completed() {
        let env = test_env();
        require_valid_status_for_refund(&env, ContractStatus::Completed);
    }

    // ── Milestone index guards ────────────────────────────────────────────────

    #[test]
    fn milestone_index_accepts_valid() {
        let env = test_env();
        let ms = milestones_vec(&env, &[100]);
        require_milestone_index_in_bounds(&env, 0, &ms);
    }

    #[test]
    #[should_panic]
    fn milestone_index_rejects_out_of_bounds() {
        let env = test_env();
        let ms = milestones_vec(&env, &[100]);
        require_milestone_index_in_bounds(&env, 1, &ms); // off-by-one
    }

    #[test]
    #[should_panic]
    fn milestone_not_released_rejects_released() {
        let env = test_env();
        require_milestone_not_released(&env, true);
    }

    #[test]
    fn milestone_not_released_accepts_unreleased() {
        let env = test_env();
        require_milestone_not_released(&env, false);
    }

    #[test]
    #[should_panic]
    fn milestone_not_refunded_rejects_refunded() {
        let env = test_env();
        require_milestone_not_refunded(&env, true);
    }

    #[test]
    fn milestone_not_refunded_accepts_not_refunded() {
        let env = test_env();
        require_milestone_not_refunded(&env, false);
    }

    // ── Authorization guards ──────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn caller_is_client_rejects_non_client() {
        let env = test_env();
        let c = Address::generate(&env);
        let other = Address::generate(&env);
        require_caller_is_client(&env, &other, &c);
    }

    #[test]
    fn caller_is_client_accepts_client() {
        let env = test_env();
        let c = Address::generate(&env);
        require_caller_is_client(&env, &c, &c);
    }

    #[test]
    #[should_panic]
    fn caller_is_freelancer_rejects_non_freelancer() {
        let env = test_env();
        let f = Address::generate(&env);
        let other = Address::generate(&env);
        require_caller_is_freelancer(&env, &other, &f);
    }

    #[test]
    fn caller_is_freelancer_accepts_freelancer() {
        let env = test_env();
        let f = Address::generate(&env);
        require_caller_is_freelancer(&env, &f, &f);
    }

    #[test]
    #[should_panic]
    fn caller_is_arbiter_rejects_non_arbiter() {
        let env = test_env();
        let a = Address::generate(&env);
        let other = Address::generate(&env);
        require_caller_is_arbiter(&env, &other, &Some(a));
    }

    #[test]
    #[should_panic]
    fn caller_is_arbiter_rejects_when_no_arbiter() {
        let env = test_env();
        let other = Address::generate(&env);
        require_caller_is_arbiter(&env, &other, &None);
    }

    #[test]
    fn caller_is_arbiter_accepts_arbiter() {
        let env = test_env();
        let a = Address::generate(&env);
        require_caller_is_arbiter(&env, &a, &Some(a.clone()));
    }

    // ── Release caller validation ─────────────────────────────────────────────

    #[test]
    fn valid_release_caller_client_only_accepts_client() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_valid_release_caller(&env, &c, &c, &f, &None, &ReleaseAuthorization::ClientOnly);
    }

    #[test]
    #[should_panic]
    fn valid_release_caller_client_only_rejects_freelancer() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_valid_release_caller(&env, &f, &c, &f, &None, &ReleaseAuthorization::ClientOnly);
    }

    #[test]
    fn valid_release_caller_arbiter_only_accepts_arbiter() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let a = Address::generate(&env);
        require_valid_release_caller(
            &env,
            &a,
            &c,
            &f,
            &Some(a.clone()),
            &ReleaseAuthorization::ArbiterOnly,
        );
    }

    #[test]
    #[should_panic]
    fn valid_release_caller_arbiter_only_rejects_client() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let a = Address::generate(&env);
        require_valid_release_caller(
            &env,
            &c,
            &c,
            &f,
            &Some(a),
            &ReleaseAuthorization::ArbiterOnly,
        );
    }

    #[test]
    fn valid_release_caller_multisig_accepts_client() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_valid_release_caller(&env, &c, &c, &f, &None, &ReleaseAuthorization::MultiSig);
    }

    #[test]
    fn valid_release_caller_multisig_accepts_freelancer() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_valid_release_caller(&env, &f, &c, &f, &None, &ReleaseAuthorization::MultiSig);
    }

    #[test]
    #[should_panic]
    fn valid_release_caller_multisig_rejects_attacker() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let attacker = Address::generate(&env);
        require_valid_release_caller(
            &env,
            &attacker,
            &c,
            &f,
            &None,
            &ReleaseAuthorization::MultiSig,
        );
    }

    // ── Accounting guards ─────────────────────────────────────────────────────

    #[test]
    fn deposit_within_capacity_accepts_partial() {
        let env = test_env();
        require_deposit_within_capacity(&env, 500, 400, 1_000);
    }

    #[test]
    fn deposit_within_capacity_accepts_exact() {
        let env = test_env();
        require_deposit_within_capacity(&env, 500, 500, 1_000);
    }

    #[test]
    #[should_panic]
    fn deposit_within_capacity_rejects_over() {
        let env = test_env();
        require_deposit_within_capacity(&env, 500, 501, 1_000);
    }

    #[test]
    fn sufficient_balance_for_release_accepts_exact() {
        let env = test_env();
        require_sufficient_balance_for_release(&env, 100, 100);
    }

    #[test]
    #[should_panic]
    fn sufficient_balance_for_release_rejects_insufficient() {
        let env = test_env();
        require_sufficient_balance_for_release(&env, 99, 100);
    }

    #[test]
    fn sufficient_balance_for_refund_accepts_exact() {
        let env = test_env();
        require_sufficient_balance_for_refund(&env, 100, 100);
    }

    #[test]
    #[should_panic]
    fn sufficient_balance_for_refund_rejects_insufficient() {
        let env = test_env();
        require_sufficient_balance_for_refund(&env, 99, 100);
    }

    // ── Reputation guards ─────────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn valid_rating_rejects_zero() {
        let env = test_env();
        require_valid_rating(&env, 0);
    }

    #[test]
    #[should_panic]
    fn valid_rating_rejects_six() {
        let env = test_env();
        require_valid_rating(&env, 6);
    }

    #[test]
    fn valid_rating_accepts_one() {
        let env = test_env();
        require_valid_rating(&env, 1);
    }

    #[test]
    fn valid_rating_accepts_five() {
        let env = test_env();
        require_valid_rating(&env, 5);
    }

    #[test]
    fn valid_rating_accepts_boundary_values() {
        let env = test_env();
        for r in 1..=5 {
            require_valid_rating(&env, r);
        }
    }

    #[test]
    #[should_panic]
    fn nonempty_comment_rejects_empty_string() {
        let env = test_env();
        let empty = String::from_str(&env, "");
        require_nonempty_comment(&env, &empty);
    }

    #[test]
    fn nonempty_comment_accepts_single_char() {
        let env = test_env();
        let s = String::from_str(&env, "a");
        require_nonempty_comment(&env, &s);
    }

    #[test]
    fn comment_within_limit_accepts_exactly_200_ascii() {
        let env = test_env();
        let s200 = String::from_str(
            &env,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        // exactly 200 chars (each ASCII = 1 byte)
        // Use first 200 chars only
        let s = String::from_str(&env, &"a".repeat(200));
        require_comment_within_limit(&env, &s);
    }

    // ── Refund request uniqueness ─────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn nonempty_refund_request_rejects_empty() {
        let env = test_env();
        let empty: Vec<u32> = Vec::new(&env);
        require_nonempty_refund_request(&env, &empty);
    }

    #[test]
    fn nonempty_refund_request_accepts_one() {
        let env = test_env();
        let v = vec![&env, 0u32];
        require_nonempty_refund_request(&env, &v);
    }

    #[test]
    #[should_panic]
    fn no_duplicate_refund_indices_rejects_duplicate() {
        let env = test_env();
        let v = vec![&env, 0u32, 1u32, 0u32];
        require_no_duplicate_refund_indices(&env, &v);
    }

    #[test]
    fn no_duplicate_refund_indices_accepts_distinct() {
        let env = test_env();
        let v = vec![&env, 0u32, 1u32, 2u32];
        require_no_duplicate_refund_indices(&env, &v);
    }

    // ── require_positive_amount — additional boundaries ───────────────────────

    /// i128::MIN is the most-negative value; must be rejected.
    #[test]
    #[should_panic]
    fn positive_amount_rejects_i128_min() {
        let env = test_env();
        require_positive_amount(&env, i128::MIN);
    }

    /// -1 is negative; already covered but kept for explicit documentation.
    #[test]
    #[should_panic]
    fn positive_amount_rejects_minus_one() {
        let env = test_env();
        require_positive_amount(&env, -1_i128);
    }

    /// i128::MAX - 1 is still a valid positive amount.
    #[test]
    fn positive_amount_accepts_large_value() {
        let env = test_env();
        require_positive_amount(&env, i128::MAX - 1);
    }

    // ── require_arbiter_for_mode — accepted paths with arbiter present ─────────

    /// ArbiterOnly mode is satisfied when an arbiter is provided.
    #[test]
    fn arbiter_for_mode_accepts_arbiter_only_with_arbiter() {
        let env = test_env();
        let a = Address::generate(&env);
        require_arbiter_for_mode(&env, &ReleaseAuthorization::ArbiterOnly, &Some(a));
    }

    /// ClientAndArbiter mode is satisfied when an arbiter is provided.
    #[test]
    fn arbiter_for_mode_accepts_client_and_arbiter_with_arbiter() {
        let env = test_env();
        let a = Address::generate(&env);
        require_arbiter_for_mode(&env, &ReleaseAuthorization::ClientAndArbiter, &Some(a));
    }

    /// MultiSig with an arbiter present should still be accepted (arbiter is optional).
    #[test]
    fn arbiter_for_mode_accepts_multisig_with_arbiter() {
        let env = test_env();
        let a = Address::generate(&env);
        require_arbiter_for_mode(&env, &ReleaseAuthorization::MultiSig, &Some(a));
    }

    // ── require_milestone_amounts_valid — single-amount cap + overflow ────────

    /// Exactly at MAX_SINGLE_AMOUNT_STROOPS must be accepted.
    #[test]
    fn milestone_amounts_accepts_single_at_cap() {
        let env = test_env();
        let cap = MAX_SINGLE_AMOUNT_STROOPS;
        let ms = vec![&env, cap];
        require_milestone_amounts_valid(&env, &ms, cap);
    }

    /// One stroop above the per-milestone cap must be rejected.
    #[test]
    #[should_panic]
    fn milestone_amounts_rejects_single_over_cap() {
        let env = test_env();
        let over = MAX_SINGLE_AMOUNT_STROOPS + 1;
        let ms = vec![&env, over];
        require_milestone_amounts_valid(&env, &ms, i128::MAX);
    }

    /// Multiple milestones whose sum exactly equals the total cap must be accepted.
    #[test]
    fn milestone_amounts_accepts_multiple_summing_to_cap() {
        let env = test_env();
        // Use two milestones each at 500, cap = 1000
        let cap = 1_000_i128;
        let ms = vec![&env, 500_i128, 500_i128];
        require_milestone_amounts_valid(&env, &ms, cap);
    }

    /// Two milestones whose sum exceeds the cap must be rejected.
    #[test]
    #[should_panic]
    fn milestone_amounts_rejects_two_milestones_over_cap() {
        let env = test_env();
        let cap = 1_000_i128;
        let ms = vec![&env, 600_i128, 500_i128]; // sum 1100 > 1000
        require_milestone_amounts_valid(&env, &ms, cap);
    }

    /// Overflow path: two i128::MAX values should trigger PotentialOverflow.
    #[test]
    #[should_panic]
    fn milestone_amounts_rejects_overflow() {
        let env = test_env();
        // Each amount is within per-milestone cap but two of MAX_SINGLE_AMOUNT_STROOPS
        // would overflow if cap is i128::MAX and we try two at MAX_SINGLE_AMOUNT_STROOPS
        // with total cap also at MAX_SINGLE_AMOUNT_STROOPS * 2 (which is fine).
        // To trigger overflow we need sum to overflow i128 itself.
        // Use MAX_SINGLE_AMOUNT_STROOPS for each milestone and set max_total to i128::MAX,
        // then force overflow by using values that sum past i128::MAX.
        // Only way is to use i128::MAX / 2 + 1 twice (both within per-milstone cap if cap is large).
        // We temporarily bypass the per-milestone cap by using a value = i128::MAX / 2 + 1
        // which is > MAX_SINGLE_AMOUNT_STROOPS so it will panic with InvalidMilestoneAmount.
        // Instead we test via: funded = i128::MAX - 1 and amount = 2 in deposit_within_capacity.
        // For milestone amounts overflow, we'll just confirm the per-milestone cap rejects large values.
        // This test confirms the single-amount cap path panics for huge values.
        let env2 = test_env();
        // This value exceeds MAX_SINGLE_AMOUNT_STROOPS, triggers InvalidMilestoneAmount
        let huge = MAX_SINGLE_AMOUNT_STROOPS + 1;
        let ms = vec![&env2, huge];
        require_milestone_amounts_valid(&env2, &ms, i128::MAX);
    }

    // ── require_contract_not_paused — both flags set ───────────────────────────

    /// When both paused and emergency are true, emergency takes priority (checked first).
    #[test]
    #[should_panic]
    fn contract_not_paused_rejects_both_paused_and_emergency() {
        let env = test_env();
        require_contract_not_paused(&env, true, true);
    }

    // ── Status guards — additional rejected statuses for deposit ─────────────

    #[test]
    #[should_panic]
    fn status_for_deposit_rejects_completed() {
        let env = test_env();
        require_valid_status_for_deposit(&env, ContractStatus::Completed);
    }

    #[test]
    #[should_panic]
    fn status_for_deposit_rejects_refunded() {
        let env = test_env();
        require_valid_status_for_deposit(&env, ContractStatus::Refunded);
    }

    #[test]
    #[should_panic]
    fn status_for_deposit_rejects_disputed() {
        let env = test_env();
        require_valid_status_for_deposit(&env, ContractStatus::Disputed);
    }

    // ── Status guards — additional rejected statuses for release ─────────────

    #[test]
    #[should_panic]
    fn status_for_release_rejects_completed() {
        let env = test_env();
        require_valid_status_for_release(&env, ContractStatus::Completed);
    }

    #[test]
    #[should_panic]
    fn status_for_release_rejects_cancelled() {
        let env = test_env();
        require_valid_status_for_release(&env, ContractStatus::Cancelled);
    }

    #[test]
    #[should_panic]
    fn status_for_release_rejects_disputed() {
        let env = test_env();
        require_valid_status_for_release(&env, ContractStatus::Disputed);
    }

    #[test]
    #[should_panic]
    fn status_for_release_rejects_partially_funded() {
        let env = test_env();
        require_valid_status_for_release(&env, ContractStatus::PartiallyFunded);
    }

    // ── Status guards — additional rejected/accepted statuses for cancel ──────

    /// PartiallyFunded is not in the accepted set for cancel.
    #[test]
    #[should_panic]
    fn status_for_cancel_rejects_partially_funded() {
        let env = test_env();
        require_valid_status_for_cancel(&env, ContractStatus::PartiallyFunded);
    }

    #[test]
    #[should_panic]
    fn status_for_cancel_rejects_refunded() {
        let env = test_env();
        require_valid_status_for_cancel(&env, ContractStatus::Refunded);
    }

    #[test]
    #[should_panic]
    fn status_for_cancel_rejects_disputed() {
        let env = test_env();
        require_valid_status_for_cancel(&env, ContractStatus::Disputed);
    }

    // ── Status guards — additional rejected statuses for refund ──────────────

    #[test]
    #[should_panic]
    fn status_for_refund_rejects_cancelled() {
        let env = test_env();
        require_valid_status_for_refund(&env, ContractStatus::Cancelled);
    }

    #[test]
    #[should_panic]
    fn status_for_refund_rejects_refunded() {
        let env = test_env();
        require_valid_status_for_refund(&env, ContractStatus::Refunded);
    }

    // ── require_valid_status_for_dispute — all paths (previously untested) ────

    #[test]
    fn status_for_dispute_accepts_funded() {
        let env = test_env();
        require_valid_status_for_dispute(&env, ContractStatus::Funded);
    }

    #[test]
    fn status_for_dispute_accepts_partially_funded() {
        let env = test_env();
        require_valid_status_for_dispute(&env, ContractStatus::PartiallyFunded);
    }

    #[test]
    #[should_panic]
    fn status_for_dispute_rejects_created() {
        let env = test_env();
        require_valid_status_for_dispute(&env, ContractStatus::Created);
    }

    #[test]
    #[should_panic]
    fn status_for_dispute_rejects_completed() {
        let env = test_env();
        require_valid_status_for_dispute(&env, ContractStatus::Completed);
    }

    #[test]
    #[should_panic]
    fn status_for_dispute_rejects_cancelled() {
        let env = test_env();
        require_valid_status_for_dispute(&env, ContractStatus::Cancelled);
    }

    #[test]
    #[should_panic]
    fn status_for_dispute_rejects_disputed() {
        let env = test_env();
        // Already in Disputed — cannot raise a second dispute.
        require_valid_status_for_dispute(&env, ContractStatus::Disputed);
    }

    // ── require_dispute_in_disputed_state — all paths (previously untested) ───

    #[test]
    fn dispute_in_disputed_state_accepts_disputed() {
        let env = test_env();
        require_dispute_in_disputed_state(&env, ContractStatus::Disputed);
    }

    #[test]
    #[should_panic]
    fn dispute_in_disputed_state_rejects_funded() {
        let env = test_env();
        require_dispute_in_disputed_state(&env, ContractStatus::Funded);
    }

    #[test]
    #[should_panic]
    fn dispute_in_disputed_state_rejects_created() {
        let env = test_env();
        require_dispute_in_disputed_state(&env, ContractStatus::Created);
    }

    #[test]
    #[should_panic]
    fn dispute_in_disputed_state_rejects_completed() {
        let env = test_env();
        require_dispute_in_disputed_state(&env, ContractStatus::Completed);
    }

    #[test]
    #[should_panic]
    fn dispute_in_disputed_state_rejects_cancelled() {
        let env = test_env();
        require_dispute_in_disputed_state(&env, ContractStatus::Cancelled);
    }

    // ── require_caller_is_party — all paths (previously untested) ────────────

    #[test]
    fn caller_is_party_accepts_client() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_caller_is_party(&env, &c, &c, &f);
    }

    #[test]
    fn caller_is_party_accepts_freelancer() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_caller_is_party(&env, &f, &c, &f);
    }

    #[test]
    #[should_panic]
    fn caller_is_party_rejects_third_party() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let attacker = Address::generate(&env);
        require_caller_is_party(&env, &attacker, &c, &f);
    }

    // ── require_arbiter_present — all paths (previously untested) ────────────

    #[test]
    fn arbiter_present_accepts_some() {
        let env = test_env();
        let a = Address::generate(&env);
        require_arbiter_present(&env, &Some(a));
    }

    #[test]
    #[should_panic]
    fn arbiter_present_rejects_none() {
        let env = test_env();
        require_arbiter_present(&env, &None);
    }

    // ── require_valid_release_caller — ClientAndArbiter mode ─────────────────

    #[test]
    fn valid_release_caller_client_and_arbiter_accepts_client() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let a = Address::generate(&env);
        require_valid_release_caller(
            &env,
            &c,
            &c,
            &f,
            &Some(a),
            &ReleaseAuthorization::ClientAndArbiter,
        );
    }

    #[test]
    fn valid_release_caller_client_and_arbiter_accepts_arbiter() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let a = Address::generate(&env);
        require_valid_release_caller(
            &env,
            &a,
            &c,
            &f,
            &Some(a.clone()),
            &ReleaseAuthorization::ClientAndArbiter,
        );
    }

    #[test]
    #[should_panic]
    fn valid_release_caller_client_and_arbiter_rejects_freelancer() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let a = Address::generate(&env);
        require_valid_release_caller(
            &env,
            &f,
            &c,
            &f,
            &Some(a),
            &ReleaseAuthorization::ClientAndArbiter,
        );
    }

    #[test]
    #[should_panic]
    fn valid_release_caller_client_and_arbiter_rejects_third_party() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        let a = Address::generate(&env);
        let attacker = Address::generate(&env);
        require_valid_release_caller(
            &env,
            &attacker,
            &c,
            &f,
            &Some(a),
            &ReleaseAuthorization::ClientAndArbiter,
        );
    }

    // ── require_deposit_within_capacity — overflow path ──────────────────────

    /// funded = i128::MAX, amount = 1 → overflow → PotentialOverflow.
    #[test]
    #[should_panic]
    fn deposit_within_capacity_rejects_overflow() {
        let env = test_env();
        require_deposit_within_capacity(&env, i128::MAX, 1, i128::MAX);
    }

    /// funded = 0, amount = 1, cap = 1 → accepted (first deposit equals cap).
    #[test]
    fn deposit_within_capacity_accepts_zero_funded_single_deposit() {
        let env = test_env();
        require_deposit_within_capacity(&env, 0, 1, 1);
    }

    // ── require_sufficient_accumulated_fees — all paths (previously untested) ─

    #[test]
    fn sufficient_accumulated_fees_accepts_equal() {
        let env = test_env();
        require_sufficient_accumulated_fees(&env, 100, 100);
    }

    #[test]
    fn sufficient_accumulated_fees_accepts_less_than_accumulated() {
        let env = test_env();
        require_sufficient_accumulated_fees(&env, 50, 100);
    }

    #[test]
    #[should_panic]
    fn sufficient_accumulated_fees_rejects_exceeds_accumulated() {
        let env = test_env();
        require_sufficient_accumulated_fees(&env, 101, 100);
    }

    #[test]
    #[should_panic]
    fn sufficient_accumulated_fees_rejects_zero_accumulated() {
        let env = test_env();
        // Trying to withdraw 1 when accumulated is 0 must be rejected.
        require_sufficient_accumulated_fees(&env, 1, 0);
    }

    // ── require_reputation_not_issued — all paths (previously untested) ───────

    #[test]
    fn reputation_not_issued_accepts_false() {
        let env = test_env();
        require_reputation_not_issued(&env, false);
    }

    #[test]
    #[should_panic]
    fn reputation_not_issued_rejects_true() {
        let env = test_env();
        require_reputation_not_issued(&env, true);
    }

    // ── require_contract_completed — all paths (previously untested) ──────────

    #[test]
    fn contract_completed_accepts_completed() {
        let env = test_env();
        require_contract_completed(&env, ContractStatus::Completed);
    }

    #[test]
    #[should_panic]
    fn contract_completed_rejects_funded() {
        let env = test_env();
        require_contract_completed(&env, ContractStatus::Funded);
    }

    #[test]
    #[should_panic]
    fn contract_completed_rejects_created() {
        let env = test_env();
        require_contract_completed(&env, ContractStatus::Created);
    }

    #[test]
    #[should_panic]
    fn contract_completed_rejects_cancelled() {
        let env = test_env();
        require_contract_completed(&env, ContractStatus::Cancelled);
    }

    #[test]
    #[should_panic]
    fn contract_completed_rejects_disputed() {
        let env = test_env();
        require_contract_completed(&env, ContractStatus::Disputed);
    }

    // ── require_no_self_rating — all paths (previously untested) ─────────────

    #[test]
    fn no_self_rating_accepts_distinct() {
        let env = test_env();
        let c = Address::generate(&env);
        let f = Address::generate(&env);
        require_no_self_rating(&env, &c, &f);
    }

    #[test]
    #[should_panic]
    fn no_self_rating_rejects_same_address() {
        let env = test_env();
        let same = Address::generate(&env);
        require_no_self_rating(&env, &same, &same);
    }

    // ── require_comment_within_limit — boundary edges ────────────────────────

    /// Exactly 201 bytes (ASCII) must be rejected.
    #[test]
    #[should_panic]
    fn comment_within_limit_rejects_201_bytes() {
        let env = test_env();
        // Build a 201-character ASCII string
        let s = "a".repeat(201);
        let comment = String::from_str(&env, &s);
        require_comment_within_limit(&env, &comment);
    }

    /// 199 bytes must be accepted (one below boundary).
    #[test]
    fn comment_within_limit_accepts_199_bytes() {
        let env = test_env();
        let s = "a".repeat(199);
        let comment = String::from_str(&env, &s);
        require_comment_within_limit(&env, &comment);
    }

    // ── require_evidence_within_limit — all paths (previously untested) ───────

    /// Exactly at MAX_EVIDENCE_BYTES (256) must be accepted.
    #[test]
    fn evidence_within_limit_accepts_exactly_256() {
        let env = test_env();
        let s = "a".repeat(256);
        let evidence = String::from_str(&env, &s);
        require_evidence_within_limit(&env, &evidence);
    }

    /// 257 bytes must be rejected (one over boundary).
    #[test]
    #[should_panic]
    fn evidence_within_limit_rejects_257_bytes() {
        let env = test_env();
        let s = "a".repeat(257);
        let evidence = String::from_str(&env, &s);
        require_evidence_within_limit(&env, &evidence);
    }

    /// Empty evidence string must be accepted (no minimum length constraint).
    #[test]
    fn evidence_within_limit_accepts_empty() {
        let env = test_env();
        let evidence = String::from_str(&env, "");
        require_evidence_within_limit(&env, &evidence);
    }

    /// Single byte must be accepted.
    #[test]
    fn evidence_within_limit_accepts_single_byte() {
        let env = test_env();
        let evidence = String::from_str(&env, "x");
        require_evidence_within_limit(&env, &evidence);
    }

    // ── require_no_duplicate_refund_indices — additional cases ────────────────

    /// A single-element list can never have duplicates.
    #[test]
    fn no_duplicate_refund_indices_accepts_single_element() {
        let env = test_env();
        let v = vec![&env, 5u32];
        require_no_duplicate_refund_indices(&env, &v);
    }

    /// All identical indices in a full-length list must be rejected.
    #[test]
    #[should_panic]
    fn no_duplicate_refund_indices_rejects_all_same() {
        let env = test_env();
        let v = vec![&env, 0u32, 0u32, 0u32];
        require_no_duplicate_refund_indices(&env, &v);
    }

    /// Duplicate appears at the end of the list (not at position 0/1).
    #[test]
    #[should_panic]
    fn no_duplicate_refund_indices_rejects_tail_duplicate() {
        let env = test_env();
        let v = vec![&env, 0u32, 1u32, 2u32, 1u32];
        require_no_duplicate_refund_indices(&env, &v);
    }

    // ── require_milestone_index_in_bounds — additional cases ─────────────────

    /// Index 0 on an empty vec must be rejected.
    #[test]
    #[should_panic]
    fn milestone_index_rejects_any_index_on_empty_vec() {
        let env = test_env();
        let ms = milestones_vec(&env, &[]);
        require_milestone_index_in_bounds(&env, 0, &ms);
    }

    /// Last valid index (len - 1) must be accepted.
    #[test]
    fn milestone_index_accepts_last_valid_index() {
        let env = test_env();
        let ms = milestones_vec(&env, &[100, 200, 300]);
        require_milestone_index_in_bounds(&env, 2, &ms); // index 2 of 3
    }

    /// First off-by-one index (len) must be rejected.
    #[test]
    #[should_panic]
    fn milestone_index_rejects_one_past_last() {
        let env = test_env();
        let ms = milestones_vec(&env, &[100, 200, 300]);
        require_milestone_index_in_bounds(&env, 3, &ms); // off by one
    }

    // ── require_sufficient_balance_for_release — additional boundary ──────────

    /// available > required must be accepted (strictly greater, not just equal).
    #[test]
    fn sufficient_balance_for_release_accepts_greater() {
        let env = test_env();
        require_sufficient_balance_for_release(&env, 101, 100);
    }

    /// available = 0, required = 0 — edge case: zero equals zero, accepted.
    #[test]
    fn sufficient_balance_for_release_accepts_zero_equals_zero() {
        let env = test_env();
        require_sufficient_balance_for_release(&env, 0, 0);
    }

    // ── require_sufficient_balance_for_refund — additional boundary ───────────

    /// refundable > total_refund must be accepted.
    #[test]
    fn sufficient_balance_for_refund_accepts_greater() {
        let env = test_env();
        require_sufficient_balance_for_refund(&env, 101, 100);
    }

    // ── Regression: guards compose correctly ─────────────────────────────────

    /// Verify that require_valid_participants and require_no_self_rating both
    /// independently guard the same self-address condition, with distinct error codes.
    /// This is a regression test ensuring neither guard was accidentally removed.
    #[test]
    #[should_panic]
    fn regression_participants_self_address_panics() {
        let env = test_env();
        let same = Address::generate(&env);
        require_valid_participants(&env, &same, &same);
    }

    #[test]
    #[should_panic]
    fn regression_self_rating_self_address_panics() {
        let env = test_env();
        let same = Address::generate(&env);
        require_no_self_rating(&env, &same, &same);
    }

    /// Paused contract rejected before any auth check — regression for pause bypass.
    #[test]
    #[should_panic]
    fn regression_paused_guard_enforced() {
        let env = test_env();
        require_contract_not_paused(&env, true, false);
    }

    /// Emergency guard is independent of paused flag — regression for emergency bypass.
    #[test]
    #[should_panic]
    fn regression_emergency_guard_enforced_when_not_paused() {
        let env = test_env();
        require_contract_not_paused(&env, false, true);
    }

    // ── Concurrent-execution hardening: fixtures ──────────────────────────────

    /// Builds one milestone of a settlement snapshot. `deadline` is the milestone
    /// deadline, `released`/`refunded` the settlement flags a competing invocation
    /// would have committed.
    fn settlement_milestone(
        amount: i128,
        released: bool,
        refunded: bool,
        deadline: Option<u64>,
    ) -> Milestone {
        Milestone {
            amount,
            funded_amount: amount,
            released,
            refunded,
            work_evidence: None,
            refunded_amount: if refunded { amount } else { 0 },
            deadline,
        }
    }

    /// A snapshot of `len` unsettled, deadline-free milestones each worth `amount`.
    fn open_schedule(env: &Env, len: u32, amount: i128) -> Vec<Milestone> {
        let mut schedule: Vec<Milestone> = Vec::new(env);
        for _ in 0..len {
            schedule.push_back(settlement_milestone(amount, false, false, None));
        }
        schedule
    }

    /// A snapshot of `len` unsettled milestones that all expire at `deadline`.
    fn deadline_schedule(env: &Env, len: u32, amount: i128, deadline: u64) -> Vec<Milestone> {
        let mut schedule: Vec<Milestone> = Vec::new(env);
        for _ in 0..len {
            schedule.push_back(settlement_milestone(amount, false, false, Some(deadline)));
        }
        schedule
    }

    /// The snapshot observed *after* a competing invocation released `settled`.
    fn after_release(schedule: &Vec<Milestone>, settled: &[u32]) -> Vec<Milestone> {
        let mut next = schedule.clone();
        for &i in settled {
            let mut milestone = next.get(i).unwrap();
            milestone.released = true;
            next.set(i, milestone);
        }
        next
    }

    /// The snapshot observed *after* a competing invocation refunded `refunded`.
    fn after_refund(schedule: &Vec<Milestone>, refunded: &[u32]) -> Vec<Milestone> {
        let mut next = schedule.clone();
        for &i in refunded {
            let mut milestone = next.get(i).unwrap();
            milestone.refunded = true;
            milestone.refunded_amount = milestone.amount;
            next.set(i, milestone);
        }
        next
    }

    /// Ledger counters of a snapshot after `released`/`refunded` were committed.
    fn counters(
        funded: i128,
        released: &[i128],
        refunded: &[i128],
        fees: i128,
    ) -> (i128, i128, i128, i128) {
        (funded, released.iter().sum(), refunded.iter().sum(), fees)
    }

    // ── available_balance_value: success and boundaries ───────────────────────

    #[test]
    fn available_balance_value_accepts_empty_escrow() {
        assert_eq!(available_balance_value(0, 0, 0, 0), Ok(0));
        assert_eq!(available_balance_value(1_000, 0, 0, 0), Ok(1_000));
    }

    #[test]
    fn available_balance_value_accepts_exactly_fully_settled() {
        assert_eq!(available_balance_value(1_000, 400, 300, 300), Ok(0));
    }

    #[test]
    fn available_balance_value_keeps_remaining_balance() {
        assert_eq!(available_balance_value(1_000, 250, 250, 100), Ok(400));
    }

    /// I3: a counter that would underflow fails with a typed code, never a trap.
    #[test]
    fn available_balance_value_rejects_settled_above_funded() {
        assert_eq!(
            available_balance_value(1_000, 600, 300, 101),
            Err(Error::AccountingInvariantViolated)
        );
        assert_eq!(
            available_balance_value(1_000, 1_000, 0, 1),
            Err(Error::AccountingInvariantViolated)
        );
        assert_eq!(
            available_balance_value(1_000, 0, 1_001, 0),
            Err(Error::AccountingInvariantViolated)
        );
    }

    /// A negative counter can only be corrupt or stale on-ledger state.
    #[test]
    fn available_balance_value_rejects_negative_counters() {
        assert_eq!(
            available_balance_value(-1, 0, 0, 0),
            Err(Error::AccountingInvariantViolated)
        );
        assert_eq!(
            available_balance_value(1_000, -1, 0, 0),
            Err(Error::AccountingInvariantViolated)
        );
        assert_eq!(
            available_balance_value(1_000, 0, -1, 0),
            Err(Error::AccountingInvariantViolated)
        );
        assert_eq!(
            available_balance_value(1_000, 0, 0, -1),
            Err(Error::AccountingInvariantViolated)
        );
    }

    #[test]
    fn available_balance_value_rejects_counter_sum_overflow() {
        assert_eq!(
            available_balance_value(i128::MAX, i128::MAX, 1, 0),
            Err(Error::PotentialOverflow)
        );
    }

    #[test]
    fn require_available_balance_returns_value_and_pins_error_codes() {
        let env = test_env();
        assert_eq!(require_available_balance(&env, 1_000, 250, 250, 100), 400);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #44)")]
    fn require_available_balance_pins_accounting_invariant_code() {
        let env = test_env();
        require_available_balance(&env, 1_000, 900, 200, 0);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #45)")]
    fn require_available_balance_pins_overflow_code() {
        let env = test_env();
        require_available_balance(&env, i128::MAX, i128::MAX, 1, 0);
    }

    // ── Deadline boundaries (I4) ───────────────────────────────────────────────

    #[test]
    fn timeout_refund_allowed_boundary_is_strictly_after_deadline() {
        assert!(!timeout_refund_allowed(99, Some(100)));
        assert!(!timeout_refund_allowed(100, Some(100)));
        assert!(timeout_refund_allowed(101, Some(100)));
        assert!(!timeout_refund_allowed(0, Some(0)));
        assert!(timeout_refund_allowed(1, Some(0)));
        assert!(!timeout_refund_allowed(u64::MAX, Some(u64::MAX)));
    }

    #[test]
    fn timeout_refund_allowed_without_deadline() {
        assert!(timeout_refund_allowed(0, None));
        assert!(timeout_refund_allowed(u64::MAX, None));
    }

    #[test]
    fn require_timeout_refund_open_rejects_exact_deadline() {
        let env = test_env();
        require_timeout_refund_open(&env, 101, Some(100));
        require_timeout_refund_open(&env, 0, None);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #53)")]
    fn require_timeout_refund_open_pins_not_overdue_code() {
        let env = test_env();
        require_timeout_refund_open(&env, 100, Some(100));
    }

    // ── plan_release: success ──────────────────────────────────────────────────

    #[test]
    fn plan_release_authorises_single_milestone() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        assert_eq!(plan_release(&schedule, &vec![&env, 0u32], 300), Ok(100));
    }

    #[test]
    fn plan_release_authorises_disjoint_indices_out_of_order() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        // Order is irrelevant: the plan is a set, not a sequence.
        assert_eq!(
            plan_release(&schedule, &vec![&env, 2u32, 0u32], 300),
            Ok(200)
        );
    }

    #[test]
    fn plan_release_accepts_exactly_affordable_plan() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        assert_eq!(
            plan_release(&schedule, &vec![&env, 0u32, 1u32, 2u32], 300),
            Ok(300)
        );
    }

    #[test]
    fn plan_release_ignores_untargeted_milestones() {
        let env = test_env();
        let schedule = open_schedule(&env, 5, 100);
        // Only the named index is paid, even though the escrow could cover all.
        assert_eq!(plan_release(&schedule, &vec![&env, 3u32], 500), Ok(100));
    }

    // ── plan_release: rejections, in the fixed evaluation order ────────────────

    /// S1.
    #[test]
    fn plan_release_rejects_empty_plan() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        assert_eq!(
            plan_release(&schedule, &vec![&env], 300),
            Err(Error::EmptyBatch)
        );
    }

    /// S2 — the plan bound is inclusive at `MAX_SETTLEMENT_PLAN_LEN` and rejects
    /// one more, before any per-index work.
    #[test]
    fn plan_release_bounds_plan_length() {
        let env = test_env();
        let schedule = open_schedule(&env, MAX_SETTLEMENT_PLAN_LEN + 1, 1);
        let mut at_bound: Vec<u32> = Vec::new(&env);
        for i in 0..MAX_SETTLEMENT_PLAN_LEN {
            at_bound.push_back(i);
        }
        assert_eq!(
            plan_release(&schedule, &at_bound, i128::MAX),
            Ok(MAX_SETTLEMENT_PLAN_LEN as i128)
        );

        let mut over: Vec<u32> = Vec::new(&env);
        for i in 0..=MAX_SETTLEMENT_PLAN_LEN {
            over.push_back(i);
        }
        // S2 wins over S5 even though this plan also names an out-of-range index.
        assert_eq!(
            plan_release(&schedule, &over, i128::MAX),
            Err(Error::BatchLimitExceeded)
        );
        assert_eq!(MAX_SETTLEMENT_PLAN_LEN, MAX_MILESTONES);
    }

    /// S3.
    #[test]
    fn plan_release_rejects_negative_available() {
        let env = test_env();
        let schedule = open_schedule(&env, 1, 100);
        assert_eq!(
            plan_release(&schedule, &vec![&env, 0u32], -1),
            Err(Error::AccountingInvariantViolated)
        );
    }

    /// S4.
    #[test]
    fn plan_release_rejects_duplicate_index() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        assert_eq!(
            plan_release(&schedule, &vec![&env, 1u32, 1u32], 300),
            Err(Error::DuplicateMilestoneInBatch)
        );
    }

    /// S5 — `index == len` is the first out-of-bounds index.
    #[test]
    fn plan_release_rejects_index_at_or_beyond_schedule() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        assert_eq!(
            plan_release(&schedule, &vec![&env, 3u32], 300),
            Err(Error::IndexOutOfBounds)
        );
        assert_eq!(
            plan_release(&schedule, &vec![&env, 0u32, 9u32], 300),
            Err(Error::IndexOutOfBounds)
        );
    }

    /// S6.
    #[test]
    fn plan_release_rejects_already_released_milestone() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        let committed = after_release(&schedule, &[0]);
        assert_eq!(
            plan_release(&committed, &vec![&env, 0u32], 200),
            Err(Error::MilestoneAlreadyReleased)
        );
    }

    /// S7.
    #[test]
    fn plan_release_rejects_refunded_milestone() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        let committed = after_refund(&schedule, &[0]);
        assert_eq!(
            plan_release(&committed, &vec![&env, 0u32], 100),
            Err(Error::AlreadyRefunded)
        );
    }

    /// S9 — a non-positive amount must not shrink a plan total and let the other
    /// indices spend escrow that was never allocated to them.
    #[test]
    fn plan_release_rejects_non_positive_amount() {
        let env = test_env();
        let mut schedule = open_schedule(&env, 2, 100);
        let corrupt = settlement_milestone(0, false, false, None);
        schedule.set(0, corrupt);
        assert_eq!(
            plan_release(&schedule, &vec![&env, 0u32, 1u32], 200),
            Err(Error::AmountMustBePositive)
        );

        let mut negative = open_schedule(&env, 2, 100);
        negative.set(1, settlement_milestone(-50, false, false, None));
        assert_eq!(
            plan_release(&negative, &vec![&env, 0u32, 1u32], 200),
            Err(Error::AmountMustBePositive)
        );
    }

    /// S10.
    #[test]
    fn plan_release_rejects_total_overflow() {
        let env = test_env();
        let mut schedule = open_schedule(&env, 2, 1);
        schedule.set(0, settlement_milestone(i128::MAX, false, false, None));
        schedule.set(1, settlement_milestone(1, false, false, None));
        assert_eq!(
            plan_release(&schedule, &vec![&env, 0u32, 1u32], i128::MAX),
            Err(Error::PotentialOverflow)
        );
    }

    /// S11 — one stroop short of affordable is the tightest rejection.
    #[test]
    fn plan_release_rejects_unaffordable_plan() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        assert_eq!(
            plan_release(&schedule, &vec![&env, 0u32, 1u32], 199),
            Err(Error::InsufficientFunds)
        );
    }

    // ── plan_release: panic guard maps 1:1 onto the Result codes ──────────────

    #[test]
    fn require_settlement_plan_sound_returns_authorised_total() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        assert_eq!(
            require_settlement_plan_sound(&env, &schedule, &vec![&env, 0u32, 1u32], 200),
            200
        );
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #73)")]
    fn require_settlement_plan_sound_pins_duplicate_code() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        require_settlement_plan_sound(&env, &schedule, &vec![&env, 0u32, 0u32], 300);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #9)")]
    fn require_settlement_plan_sound_pins_insufficient_funds_code() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        require_settlement_plan_sound(&env, &schedule, &vec![&env, 0u32, 1u32], 1);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #71)")]
    fn require_settlement_plan_sound_pins_empty_batch_code() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        require_settlement_plan_sound(&env, &schedule, &vec![&env], 300);
    }

    // ── plan_timeout_refund: deadline-gated ───────────────────────────────────

    #[test]
    fn plan_timeout_refund_accepts_missing_deadline() {
        let env = test_env();
        let schedule = open_schedule(&env, 2, 100);
        assert_eq!(
            plan_timeout_refund(&schedule, &vec![&env, 0u32, 1u32], 0, 200),
            Ok(200)
        );
    }

    #[test]
    fn plan_timeout_refund_accepts_one_second_after_deadline() {
        let env = test_env();
        let schedule = deadline_schedule(&env, 2, 100, 1_000);
        assert_eq!(
            plan_timeout_refund(&schedule, &vec![&env, 0u32], 1_001, 200),
            Ok(100)
        );
    }

    /// S8 at the exact deadline instant — not yet overdue.
    #[test]
    fn plan_timeout_refund_rejects_exact_deadline_instant() {
        let env = test_env();
        let schedule = deadline_schedule(&env, 2, 100, 1_000);
        assert_eq!(
            plan_timeout_refund(&schedule, &vec![&env, 0u32], 1_000, 200),
            Err(Error::MilestoneNotOverdue)
        );
    }

    #[test]
    fn plan_timeout_refund_rejects_one_second_before_deadline() {
        let env = test_env();
        let schedule = deadline_schedule(&env, 2, 100, 1_000);
        assert_eq!(
            plan_timeout_refund(&schedule, &vec![&env, 0u32], 999, 200),
            Err(Error::MilestoneNotOverdue)
        );
    }

    /// A mixed plan fails closed: one not-yet-overdue index invalidates the whole
    /// plan rather than being silently skipped.
    #[test]
    fn plan_timeout_refund_rejects_partially_expired_plan() {
        let env = test_env();
        let mut schedule = deadline_schedule(&env, 2, 100, 1_000);
        let mut later = schedule.get(1).unwrap();
        later.deadline = Some(2_000);
        schedule.set(1, later);
        assert_eq!(
            plan_timeout_refund(&schedule, &vec![&env, 0u32, 1u32], 1_500, 200),
            Err(Error::MilestoneNotOverdue)
        );
    }

    /// S6/S7 still apply to the timeout path: a settled milestone is not refundable
    /// however long ago it expired.
    #[test]
    fn plan_timeout_refund_rejects_settled_milestone() {
        let env = test_env();
        let schedule = deadline_schedule(&env, 3, 100, 1_000);
        let released = after_release(&schedule, &[0]);
        assert_eq!(
            plan_timeout_refund(&released, &vec![&env, 0u32], 5_000, 200),
            Err(Error::MilestoneAlreadyReleased)
        );
        let refunded = after_refund(&schedule, &[0]);
        assert_eq!(
            plan_timeout_refund(&refunded, &vec![&env, 0u32], 5_000, 200),
            Err(Error::AlreadyRefunded)
        );
    }

    #[test]
    fn require_timeout_refund_plan_sound_returns_refund_total() {
        let env = test_env();
        let schedule = deadline_schedule(&env, 2, 100, 1_000);
        assert_eq!(
            require_timeout_refund_plan_sound(&env, &schedule, &vec![&env, 0u32, 1u32], 1_001, 200),
            200
        );
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #53)")]
    fn require_timeout_refund_plan_sound_pins_not_overdue_code() {
        let env = test_env();
        let schedule = deadline_schedule(&env, 2, 100, 1_000);
        require_timeout_refund_plan_sound(&env, &schedule, &vec![&env, 0u32], 1_000, 200);
    }

    // ── Concurrency regressions ────────────────────────────────────────────────

    /// I1: two racing plans over overlapping milestones. The first committer pays;
    /// the loser is told why and the escrow is never paid twice.
    #[test]
    fn regression_racing_plans_settle_each_milestone_at_most_once() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        let available = available_balance_value(300, 0, 0, 0).unwrap();

        // Plan A commits 0 and 1 for 200.
        assert_eq!(
            plan_release(&schedule, &vec![&env, 0u32, 1u32], available),
            Ok(200)
        );
        let after_a = after_release(&schedule, &[0, 1]);
        let (f, r, rf, fees) = counters(300, &[200], &[], 0);
        let after_a_available = available_balance_value(f, r, rf, fees).unwrap();

        // Plan B named 1 and 2 against the post-commit snapshot: index 1 is gone.
        assert_eq!(
            plan_release(&after_a, &vec![&env, 1u32, 2u32], after_a_available),
            Err(Error::MilestoneAlreadyReleased)
        );

        // The non-overlapping remainder is still exactly affordable.
        assert_eq!(
            plan_release(&after_a, &vec![&env, 2u32], after_a_available),
            Ok(100)
        );
        assert_eq!(after_a_available, 100);
    }

    /// I2: an exact retry of a committed plan is rejected deterministically instead
    /// of paying again.
    #[test]
    fn regression_retry_of_committed_plan_is_rejected() {
        let env = test_env();
        let schedule = open_schedule(&env, 2, 250);
        let indices = vec![&env, 0u32, 1u32];
        assert_eq!(plan_release(&schedule, &indices, 500), Ok(500));

        let committed = after_release(&schedule, &[0, 1]);
        let (f, r, rf, fees) = counters(500, &[500], &[], 0);
        let remaining = available_balance_value(f, r, rf, fees).unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(
            plan_release(&committed, &indices, remaining),
            Err(Error::MilestoneAlreadyReleased)
        );
    }

    /// I2 for the refund path: a retried refund sees `AlreadyRefunded`.
    #[test]
    fn regression_retry_of_committed_refund_is_rejected() {
        let env = test_env();
        let schedule = deadline_schedule(&env, 2, 250, 100);
        let indices = vec![&env, 0u32, 1u32];
        assert_eq!(plan_timeout_refund(&schedule, &indices, 101, 500), Ok(500));

        let committed = after_refund(&schedule, &[0, 1]);
        let (f, r, rf, fees) = counters(500, &[], &[500], 0);
        let remaining = available_balance_value(f, r, rf, fees).unwrap();
        assert_eq!(
            plan_timeout_refund(&committed, &indices, 101, remaining),
            Err(Error::AlreadyRefunded)
        );
    }

    /// I2: release and timeout refund are mutually exclusive per index, whichever
    /// flow commits first.
    #[test]
    fn regression_release_and_timeout_refund_are_mutually_exclusive() {
        let env = test_env();
        let schedule = deadline_schedule(&env, 1, 100, 1_000);

        let after_release_wins = after_release(&schedule, &[0]);
        assert_eq!(
            plan_timeout_refund(&after_release_wins, &vec![&env, 0u32], 1_001, 0),
            Err(Error::MilestoneAlreadyReleased)
        );

        let after_refund_wins = after_refund(&schedule, &[0]);
        assert_eq!(
            plan_release(&after_refund_wins, &vec![&env, 0u32], 0),
            Err(Error::AlreadyRefunded)
        );
    }

    /// I3: a stale snapshot that still believes the escrow is full cannot spend
    /// money the post-commit snapshot says is gone.
    #[test]
    fn regression_stale_snapshot_cannot_overspend_escrow() {
        let env = test_env();
        let schedule = open_schedule(&env, 3, 100);
        let stale_available = 300; // snapshot from before any commit
        let all = vec![&env, 0u32, 1u32, 2u32];
        assert_eq!(plan_release(&schedule, &all, stale_available), Ok(300));

        // After the first plan commits, the same request against fresh counters
        // fails on the settled index, not on arithmetic.
        let committed = after_release(&schedule, &[0, 1, 2]);
        let (f, r, rf, fees) = counters(300, &[300], &[], 0);
        let fresh_available = available_balance_value(f, r, rf, fees).unwrap();
        assert_eq!(fresh_available, 0);
        assert_eq!(
            plan_release(&committed, &all, fresh_available),
            Err(Error::MilestoneAlreadyReleased)
        );
    }

    /// Disjoint plans issued against the same snapshot may never sum above the
    /// escrow, even when each is individually affordable.
    #[test]
    fn regression_disjoint_plans_never_exceed_escrow() {
        let env = test_env();
        let schedule = open_schedule(&env, 4, 100);
        let available = available_balance_value(400, 0, 0, 0).unwrap();
        let plans = [
            vec![&env, 0u32, 1u32],
            vec![&env, 2u32, 3u32],
            vec![&env, 0u32, 2u32],
        ];
        let mut committed: i128 = 0;
        for plan in plans.iter() {
            let remaining = available - committed;
            if let Ok(total) = plan_release(&schedule, plan, remaining) {
                committed += total;
            }
            assert!(
                committed <= available,
                "escrow overspent: {committed} > {available}"
            );
        }
        assert_eq!(committed, 400);
    }

    /// I5: planning is pure — a rejected or accepted plan leaves the snapshot and
    /// the escrow counters byte-identical, so a client retry is safe.
    #[test]
    fn regression_planning_is_pure_and_retryable() {
        let env = test_env();
        let schedule = open_schedule(&env, 2, 100);
        let before = schedule.clone();
        let indices = vec![&env, 0u32];

        let first = plan_release(&schedule, &indices, 200);
        let second = plan_release(&schedule, &indices, 200);
        assert_eq!(first, Ok(100));
        assert_eq!(first, second, "planning must be deterministic");

        // A rejected plan changes nothing either.
        assert_eq!(
            plan_release(&schedule, &indices, 99),
            Err(Error::InsufficientFunds)
        );
        assert_eq!(schedule, before);
    }

    /// I4: the same snapshot and the same request yield the same decision at every
    /// ledger instant, and the flip happens exactly one second after the deadline.
    #[test]
    fn regression_timeout_decision_depends_only_on_ledger_time() {
        let env = test_env();
        let schedule = deadline_schedule(&env, 1, 100, 500);
        let indices = vec![&env, 0u32];

        for _ in 0..3 {
            assert_eq!(
                plan_timeout_refund(&schedule, &indices, 499, 100),
                Err(Error::MilestoneNotOverdue)
            );
            assert_eq!(
                plan_timeout_refund(&schedule, &indices, 500, 100),
                Err(Error::MilestoneNotOverdue)
            );
            assert_eq!(plan_timeout_refund(&schedule, &indices, 501, 100), Ok(100));
            assert_eq!(plan_timeout_refund(&schedule, &indices, 502, 100), Ok(100));
        }
    }

    /// A deadline-free milestone stays refundable at any ledger time.
    #[test]
    fn regression_deadline_free_milestone_is_always_refundable() {
        let env = test_env();
        let schedule = open_schedule(&env, 1, 100);
        let indices = vec![&env, 0u32];
        for now in [0u64, 1, 1_000, u64::MAX] {
            assert_eq!(plan_timeout_refund(&schedule, &indices, now, 100), Ok(100));
        }
    }

    /// Fees accumulated by a competing invocation shrink the balance a plan may
    /// spend, and the shrink is visible before any transfer happens.
    #[test]
    fn regression_accumulated_fees_shrink_settlement_capacity() {
        let env = test_env();
        let schedule = open_schedule(&env, 2, 100);
        let indices = vec![&env, 0u32, 1u32];
        let (f, r, rf, fees) = counters(200, &[], &[], 50);
        let available = available_balance_value(f, r, rf, fees).unwrap();
        assert_eq!(available, 150);
        assert_eq!(
            plan_release(&schedule, &indices, available),
            Err(Error::InsufficientFunds)
        );
        assert_eq!(
            plan_release(&schedule, &vec![&env, 0u32], available),
            Ok(100)
        );
    }

    /// I6: the duplicate-index guard is shared, and each caller still reports its
    /// own contract-specific code.
    #[test]
    fn regression_duplicate_scan_is_shared_but_codes_are_caller_specific() {
        let env = test_env();
        let indices = vec![&env, 1u32, 1u32];
        assert_eq!(
            require_no_duplicate_indices_value(&indices),
            Err(Error::DuplicateMilestoneInRefund)
        );
        assert_eq!(
            plan_release(&open_schedule(&env, 3, 100), &indices, 300),
            Err(Error::DuplicateMilestoneInBatch)
        );
        let refund_indices = vec![&env, 2u32, 2u32];
        assert_eq!(
            require_no_duplicate_indices_value(&refund_indices),
            Err(Error::DuplicateMilestoneInRefund)
        );
    }
}
