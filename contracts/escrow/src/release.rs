//! Release validation boundary for `release_milestone` and
//! `release_milestone_batch`.
//!
//! This module is the **single definition** of which milestone releases are
//! accepted, which are rejected, and which error each rejection maps to. Both
//! the single-milestone path ([`Escrow::release_milestone_impl`]) and the batch
//! path ([`Escrow::release_milestone_batch_impl`]) run *these* functions, so the
//! two paths cannot drift apart on a boundary and a caller sees the same error
//! for the same input regardless of which entrypoint it used.
//!
//! # Why a `Result` API
//!
//! Every validator returns `Result<(), Error>` and never panics: each one is a
//! function of `(request, contract snapshot, milestone snapshot, fee
//! configuration)` and translates its verdict into a typed error. V1–V6 and
//! V8–V11 are pure — no [`soroban_sdk::Env`], no storage, no arithmetic that can
//! wrap — so they are unit-testable without a ledger. V7 is the one boundary
//! that must read the approval record, and it is a thin adapter over the
//! existing centralized guard rather than a second copy of the rule.
//!
//! The callers translate the outcome into their own contract:
//!
//! | Caller | `Err(error)` becomes |
//! | --- | --- |
//! | `release_milestone_impl` / `release_milestone_batch_impl` | `env.panic_with_error(error)` — the whole call reverts |
//! | unit tests in this module | the exact typed error value |
//!
//! Because the checks are total and side-effect free, every rejection is
//! diagnosable from the error code alone and a test needs no ledger.
//!
//! # Validation boundaries
//!
//! The boundaries are evaluated in the order below. **Order is part of the
//! interface**: the first failing boundary decides the error a caller observes,
//! so a request that is malformed *and* unaffordable reports the malformed part
//! and a caller that fixes it learns the next reason on the retry.
//!
//! | # | Boundary | Accepted | Rejected with |
//! | --- | --- | --- | --- |
//! | V1 | Batch shape | `1 <= len <= MAX_BATCH_MILESTONES` | `EmptyBatch` / `BatchLimitExceeded` |
//! | V2 | Batch duplicate-free | every index distinct | `DuplicateMilestoneInBatch` |
//! | V3 | Lifecycle state | `Funded` | `InvalidState` |
//! | V4 | Caller authority | the roles allowed by `ReleaseAuthorization` | `UnauthorizedRole` |
//! | V5 | Index within schedule | `index < milestones.len()` | `IndexOutOfBounds` |
//! | V6 | Milestone releasable | `released == false`, `refunded == false`, `amount > 0` | `MilestoneAlreadyReleased` / `AlreadyRefunded` / `InvalidMilestoneAmount` |
//! | V7 | Approval quorum | `approvals::check_approvals` succeeds | `InsufficientApprovals` |
//! | V8 | Gross total | `checked_total` of every amount | `PotentialOverflow` |
//! | V9 | Fee configuration | `fee_bps <= MAX_FEE_BPS`, `fee <= gross` | `InvalidProtocolParameters` / `AccountingInvariantViolated` |
//! | V10 | Solvable from custody | `available_balance >= gross_total` | `InsufficientFunds` |
//! | V11 | Post-state conservation | `released + refunded + fees <= funded` | `AccountingInvariantViolated` |
//!
//! V1 and V2 are request-shape checks and run before any storage read. V5/V6/V7
//! are per-target checks and report the *first* offending index in request
//! order. V8–V11 are money checks and run only once every target is known to be
//! individually releasable, so a batch is never rejected for affordability
//! after it has already been found malformed.
//!
//! # Invariants
//!
//! - **I1 — Single source of truth.** Both release paths call the validators
//!   here; there is no second copy of the boundary to drift.
//! - **I2 — Total.** Every input maps to `Ok` or to one of the typed [`Error`]
//!   codes in the table. No path panics with a raw arithmetic fault, so a
//!   failure is always diagnosable from the error code.
//! - **I3 — Atomic.** Nothing here mutates state, and both paths validate every
//!   target before writing anything (checks-effects-interactions). A rejected
//!   release leaves storage byte-identical, which is what makes a client retry
//!   of the same request deterministic and safe.
//! - **I4 — Deterministic.** The verdict depends only on the submitted request
//!   and the stored snapshot. Soroban executes one invocation at a time against
//!   a fixed snapshot, so validation and the mutation it authorises observe the
//!   same state; a duplicate submission is therefore rejected, never
//!   double-applied.
//! - **I5 — Authorization is separate and mandatory.** Validation answers *is
//!   this release well formed and affordable*; the paths still call
//!   `caller.require_auth()` before reading or writing any balance. Passing V4
//!   never substitutes for a signature.
//! - **I6 — Terminal milestones are terminal.** A milestone that is released or
//!   refunded is rejected by V6, and the settlement marker
//!   ([`DataKey::MilestoneReleased`]) is a second, independent witness of the
//!   same fact, so a corrupted `released` flag can still not be released twice.
//! - **I7 — Conservation.** An accepted release leaves
//!   `released_amount + refunded_amount + accumulated_fees <= funded_amount`
//!   (V11), i.e. custody is never over-committed and the protocol fee can only
//!   come out of the released milestone's own gross amount.
//!
//! # Failure recovery
//!
//! Every rejection is retryable and none of them requires a state change by
//! anyone else:
//!
//! | Rejection | Repair |
//! | --- | --- |
//! | `IndexOutOfBounds` | use an index inside `0..milestones.len()` |
//! | `EmptyBatch` / `BatchLimitExceeded` / `DuplicateMilestoneInBatch` | resubmit a well-formed batch of distinct indices |
//! | `InvalidState` | the contract is not `Funded` (disputed, completed, cancelled, refunded) — resolve the lifecycle first |
//! | `UnauthorizedRole` | the contract's `ReleaseAuthorization` mode does not admit this caller |
//! | `MilestoneAlreadyReleased` / `AlreadyRefunded` | already settled; read the contract to confirm the terminal state |
//! | `InsufficientApprovals` | collect the missing approvals (see `get_milestone_release_readiness`) |
//! | `InsufficientFunds` | the batch exceeds custody; split it into smaller batches |
//! | `InvalidProtocolParameters` / `AccountingInvariantViolated` / `PotentialOverflow` | the stored configuration or accounting is corrupt; the release is refused rather than allowed to move funds |
//!
//! Because a rejected call changes nothing, the identical retry produces the
//! identical rejection, and the same request succeeds as soon as the blocking
//! condition is removed. `get_milestone_release_readiness` answers "what is
//! blocking this release?" without attempting one.
//!
//! # Known divergence from the live entrypoints
//!
//! `Escrow::release_milestone` and `Escrow::release_milestone_batch` in
//! `lib.rs` currently carry their own inline copies of these rules instead of
//! calling this module, and those copies admit `PartiallyFunded` as well as
//! `Funded` at V3. V3 here keeps the stricter rule this module has always
//! applied, so the two definitions can disagree about a partially funded
//! schedule until `lib.rs` delegates here. V10 still bounds such a release by
//! actual custody, so the divergence cannot move money the contract does not
//! hold — but it must be settled before the two are unified.

use crate::milestones_consts::{MAX_BATCH_MILESTONES, MAX_FEE_BPS};
use crate::{
    approvals, keys, milestone_transitions, ttl, Contract, ContractStatus, DataKey, Error, Escrow,
    Milestone, ReleaseAuthorization,
};
use soroban_sdk::{Address, Env, Symbol, Vec};

// ── V1: request shape ─────────────────────────────────────────────────────────

/// V1 — the batch is a non-empty, bounded request.
///
/// Returns:
/// * `Err(EmptyBatch)` when no index was supplied,
/// * `Err(BatchLimitExceeded)` when more than [`MAX_BATCH_MILESTONES`]
///   indices were supplied.
///
/// The cap bounds every per-index scan below by a protocol constant instead of
/// by caller-controlled input, so a single call cannot be turned into
/// unbounded work.
pub fn validate_batch_shape(milestone_indices: &Vec<u32>) -> Result<(), Error> {
    if milestone_indices.is_empty() {
        return Err(Error::EmptyBatch);
    }

    if milestone_indices.len() > MAX_BATCH_MILESTONES {
        return Err(Error::BatchLimitExceeded);
    }

    Ok(())
}

// ── V2: duplicate-free batch ───────────────────────────────────────────────────

/// V2 — no milestone may appear twice in one batch.
///
/// A repeated index would otherwise release the same milestone twice in a
/// single call: the second application would observe a `released` flag it is
/// about to set itself and would be counted twice in `released_amount`.
///
/// The scan is `O(n^2)` over `n = len(milestone_indices)`, and V1 already caps
/// `n` at [`MAX_BATCH_MILESTONES`], so the worst accepted batch performs at most
/// 45 comparisons.
pub fn validate_batch_indices_distinct(milestone_indices: &Vec<u32>) -> Result<(), Error> {
    let len = milestone_indices.len();

    for i in 0..len {
        let current = milestone_indices.get(i).unwrap();
        for j in (i + 1)..len {
            if current == milestone_indices.get(j).unwrap() {
                return Err(Error::DuplicateMilestoneInBatch);
            }
        }
    }

    Ok(())
}

// ── V3: lifecycle ──────────────────────────────────────────────────────────────

/// V3 — only a `Funded` contract may release a milestone.
///
/// Every other status is rejected with `InvalidState`:
///
/// * `Disputed` is release-locked until the arbiter resolves it, so funds cannot
///   move while the release authorization is being contested;
/// * `Created` and `Accepted` have no funded milestone to pay out of custody;
/// * `Completed`, `Cancelled` and `Refunded` are terminal.
///
/// `PartiallyFunded` is refused as well. Incremental deposits are a real
/// lifecycle state, but releasing a milestone out of a partially funded
/// schedule needs its own policy, so this module does not silently inherit the
/// live entrypoints' more permissive rule — see the divergence note at the top
/// of this file. V10 still bounds any release by the custody actually held.
///
/// The paths additionally reject a finalized contract before reaching this
/// check, so a frozen contract reports `AlreadyFinalized` rather than
/// `InvalidState`.
pub fn validate_status(status: ContractStatus) -> Result<(), Error> {
    match status {
        ContractStatus::Funded => Ok(()),
        _ => Err(Error::InvalidState),
    }
}

// ── V4: caller authority ───────────────────────────────────────────────────────

/// V4 — the caller must be a role the contract's release mode admits.
///
/// The matrix mirrors [`ReleaseAuthorization`] exactly and is the same rule
/// [`approvals::check_approvals`] applies to the approval quorum, so a caller
/// can never be allowed to release while being unable to supply the approval
/// the mode requires:
///
/// | Mode | Accepted caller |
/// | --- | --- |
/// | `ClientOnly` | client |
/// | `ArbiterOnly` | arbiter |
/// | `ClientAndArbiter` | client **or** arbiter |
/// | `MultiSig` | client **or** freelancer (both approvals required by V7) |
///
/// Role resolution is *not* re-derived here: the caller passes the booleans it
/// already computed, so the two paths cannot disagree about who is who.
pub fn validate_release_authority(
    mode: ReleaseAuthorization,
    is_client: bool,
    is_freelancer: bool,
    is_arbiter: bool,
) -> Result<(), Error> {
    let permitted = match mode {
        ReleaseAuthorization::ClientOnly => is_client,
        ReleaseAuthorization::ArbiterOnly => is_arbiter,
        ReleaseAuthorization::ClientAndArbiter => is_client || is_arbiter,
        ReleaseAuthorization::MultiSig => is_client || is_freelancer,
    };

    if permitted {
        Ok(())
    } else {
        Err(Error::UnauthorizedRole)
    }
}

// ── V5: index bounds ───────────────────────────────────────────────────────────

/// V5 — the index must address a milestone that exists.
///
/// Accepted range is `0..schedule_len`, so the first and last milestone of the
/// schedule are both legal and `schedule_len` itself is the first rejected
/// value. `u32::MAX` is rejected by the same comparison, which is why callers
/// may pass a caller-supplied index through without pre-filtering it.
pub fn validate_index(index: u32, schedule_len: u32) -> Result<(), Error> {
    if index >= schedule_len {
        return Err(Error::IndexOutOfBounds);
    }

    Ok(())
}

// ── V6: per-milestone releasability ────────────────────────────────────────────

/// V6 — the milestone must be unsettled, and its amount must be positive.
///
/// `is_settled` is the second witness of settlement: the caller passes whether
/// the [`DataKey::MilestoneReleased`] marker is set for this index. Either
/// witness alone rejects the release, so a milestone whose `released` flag was
/// somehow cleared in storage still cannot be paid twice (I6).
///
/// Returns:
/// * `Err(MilestoneAlreadyReleased)` when the milestone is released or marked,
/// * `Err(AlreadyRefunded)` when the milestone was already refunded,
/// * `Err(InvalidMilestoneAmount)` when the stored amount is not strictly
///   positive.
///
/// The amount check is a storage-integrity guard, not a user-facing limit:
/// `create_contract` only accepts positive amounts, so a non-positive stored
/// amount can only come from corrupt state. Releasing it would *decrease*
/// `released_amount` and credit a negative payout, so the release is refused
/// instead of being allowed to move accounting backwards.
pub fn validate_milestone_releasable(milestone: &Milestone, is_settled: bool) -> Result<(), Error> {
    if milestone.released || is_settled {
        return Err(Error::MilestoneAlreadyReleased);
    }

    if milestone.refunded {
        return Err(Error::AlreadyRefunded);
    }

    if milestone.amount <= 0 {
        return Err(Error::InvalidMilestoneAmount);
    }

    Ok(())
}

// ── V7: approval quorum ────────────────────────────────────────────────────────

/// V7 — the approval quorum required by the contract's release mode is present.
///
/// Delegated to [`approvals::check_approvals`] so the rule — including
/// `MultiSig` needing both the client and the freelancer, and a missing or
/// TTL-evicted record counting as absent — is defined in exactly one place.
/// The error is [`Error::InsufficientApprovals`].
pub fn validate_approvals(
    env: &Env,
    contract: &Contract,
    contract_id: u32,
    milestone_index: u32,
) -> Result<(), Error> {
    approvals::check_approvals(env, contract, contract_id, milestone_index).map(|_| ())
}

// ── V8: gross total ────────────────────────────────────────────────────────────

/// V8 — the sum of every amount in the request, computed without wrapping.
///
/// `checked_add` turns an unrepresentable schedule into
/// [`Error::PotentialOverflow`] instead of an abort that a caller cannot
/// translate into a contract error. An empty vector sums to `0`, which is never
/// a valid release total but is reported as-is so the function stays total.
pub fn checked_total(amounts: &Vec<i128>) -> Result<i128, Error> {
    let mut total: i128 = 0;

    for amount in amounts.iter() {
        total = total.checked_add(amount).ok_or(Error::PotentialOverflow)?;
    }

    Ok(total)
}

// ── V9: protocol fee ────────────────────────────────────────────────────────────

/// V9a — the stored fee configuration must be a real fee rate.
///
/// `0` (fees disabled) through [`MAX_FEE_BPS`] (100 %) are accepted. A larger
/// stored value is refused with [`Error::InvalidProtocolParameters`]: the
/// governance entrypoint already rejects such a value, so seeing one here means
/// the configuration is corrupt, and paying a fee above 100 % would make the
/// net payout negative.
pub fn validate_fee_bps(fee_bps: u32) -> Result<(), Error> {
    if fee_bps > MAX_FEE_BPS {
        return Err(Error::InvalidProtocolParameters);
    }

    Ok(())
}

/// V9b — the net payout must be representable and non-negative.
///
/// With V9a enforced and a positive gross amount, `fee <= gross` always holds
/// (the fee is `amount * fee_bps / 10_000`, rounded down), so this is the
/// boundary that makes that precondition explicit rather than assumed. A
/// violation is refused with [`Error::AccountingInvariantViolated`] rather than
/// being allowed to shrink `released_amount`.
pub fn net_payout(gross_amount: i128, protocol_fee: i128) -> Result<i128, Error> {
    let net_amount = gross_amount
        .checked_sub(protocol_fee)
        .ok_or(Error::AccountingInvariantViolated)?;

    if net_amount < 0 {
        return Err(Error::AccountingInvariantViolated);
    }

    Ok(net_amount)
}

// ── V10: custody ───────────────────────────────────────────────────────────────

/// V10a — the contract's spendable balance: `funded - released - refunded - fees`.
///
/// Protocol fees accrued by earlier releases are still held in custody, so they
/// are subtracted too: the result is what may still be paid out to a
/// freelancer. A release of gross `G` therefore consumes exactly `G` of it
/// (`net` to the freelancer plus `fee` retained), which is what keeps the books
/// and custody in agreement.
///
/// Checked subtraction turns accounting that cannot be expressed as an `i128`
/// difference into [`Error::PotentialOverflow`]. A *negative* result is
/// representable and returned as-is: it means the stored accounting is
/// over-committed, and [`ensure_available_balance`] then rejects the release
/// with [`Error::InsufficientFunds`].
pub fn available_balance(contract: &Contract, accumulated_fees: i128) -> Result<i128, Error> {
    contract
        .funded_amount
        .checked_sub(contract.released_amount)
        .and_then(|available| available.checked_sub(contract.refunded_amount))
        .and_then(|available| available.checked_sub(accumulated_fees))
        .ok_or(Error::PotentialOverflow)
}

/// V10b — the release total must fit inside the spendable balance.
///
/// Equality is accepted: releasing exactly the remaining balance is valid and
/// is what drives the last milestone of a contract to settlement.
pub fn ensure_available_balance(
    contract: &Contract,
    accumulated_fees: i128,
    gross_total: i128,
) -> Result<(), Error> {
    if available_balance(contract, accumulated_fees)? < gross_total {
        return Err(Error::InsufficientFunds);
    }

    Ok(())
}

// ── V11: post-state conservation ───────────────────────────────────────────────

/// V11 — the release leaves the books conservative.
///
/// Evaluated on the *post-state* figures: the sum of everything the contract has
/// paid out, refunded and retained must not exceed what was funded. This is the
/// same conservation the balance check (V10) protects before the mutation; the
/// post-state form additionally catches a fee that did not come out of the
/// milestone it was charged on.
///
/// An accepted call proves `available_balance >= 0`, so a healthy contract can
/// never trip this — reaching it means the stored accounting was already
/// corrupt, and the release is refused rather than compounding the drift. Both
/// a sum that exceeds `funded_amount` and a sum that is not representable as an
/// `i128` report [`Error::AccountingInvariantViolated`], which is the code the
/// contract's accounting-invariant guard has always used for this condition.
pub fn ensure_settlement_invariant(
    funded_amount: i128,
    released_amount: i128,
    refunded_amount: i128,
    accumulated_fees: i128,
) -> Result<(), Error> {
    let committed = released_amount
        .checked_add(refunded_amount)
        .and_then(|committed| committed.checked_add(accumulated_fees))
        .ok_or(Error::AccountingInvariantViolated)?;

    if committed > funded_amount {
        return Err(Error::AccountingInvariantViolated);
    }

    Ok(())
}

impl Escrow {
    /// Reads the effective protocol fee rate, validating it at the point of use.
    ///
    /// Returns `0` when the contract is not initialized (no fee configuration
    /// exists yet) and otherwise the stored rate after V9a. Reading the rate
    /// exactly once per release is what guarantees the fee charged, the fee
    /// accrued, and the net payout are all derived from the same value.
    fn effective_protocol_fee_bps(env: &Env) -> Result<u32, Error> {
        let fee_bps = if Self::is_initialized(env) {
            Self::read_protocol_fee_bps(env)
        } else {
            0
        };

        validate_fee_bps(fee_bps)?;

        Ok(fee_bps)
    }

    /// Applies the protocol fee for `gross_amount` at an already validated rate.
    ///
    /// A rate of `0` means fees are disabled and no fee is computed, which also
    /// keeps `accumulated_fees` untouched for the common no-fee case.
    fn protocol_fee_for(env: &Env, gross_amount: i128, fee_bps: u32) -> i128 {
        if fee_bps == 0 {
            return 0;
        }

        Self::calculate_protocol_fee(env, gross_amount, fee_bps)
    }

    /// Returns `true` when the per-contract settlement marker for
    /// `milestone_index` is set.
    ///
    /// The marker is written in the same invocation that sets
    /// `milestone.released`, and V6 treats either witness as proof of
    /// settlement (I6).
    fn is_settlement_marked(env: &Env, contract_id: u32, milestone_index: u32) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::MilestoneReleased(contract_id, milestone_index))
            .unwrap_or(false)
    }

    /// Resolves the caller's role booleans for V4.
    fn release_roles(contract: &Contract, caller: &Address) -> (bool, bool, bool) {
        (
            caller == &contract.client,
            caller == &contract.freelancer,
            contract.arbiter.as_ref() == Some(caller),
        )
    }

    /// Core logic for releasing a milestone, recording the release in the
    /// contract's accounting.
    ///
    /// The entrypoint in `lib.rs` runs the initialization and pause guards and
    /// authenticates the caller before delegating here; every release boundary
    /// (V3–V11) is then enforced in the order documented at the top of this
    /// module, and all of it happens before the first write, so a rejected
    /// release leaves storage untouched.
    ///
    /// The milestone status change is additionally routed through the
    /// centralized transition validator
    /// ([`milestone_transitions::validate_milestone_transition`]) so state
    /// machine enforcement stays identical across all mutation paths
    /// (issue #1340).
    pub(crate) fn release_milestone_impl(
        env: &Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        Self::require_not_paused(&env);
        caller.require_auth();

        Self::require_not_finalized(&env, contract_id);

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);

        // V3 — lifecycle. A disputed or terminal contract is never releasable.
        validate_status(contract.status).unwrap_or_else(|e| env.panic_with_error(e));

        // V4 — the caller's role must be admitted by the contract's release mode.
        let (is_client, is_freelancer, is_arbiter) = Self::release_roles(&contract, &caller);
        validate_release_authority(
            contract.release_authorization,
            is_client,
            is_freelancer,
            is_arbiter,
        )
        .unwrap_or_else(|e| env.panic_with_error(e));

        let milestone_key = keys::milestone_key(&env, contract_id);
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&milestone_key)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_milestone_ttl(&env, contract_id);

        // V5 — the index must address a milestone of this contract.
        validate_index(milestone_index, milestones.len())
            .unwrap_or_else(|e| env.panic_with_error(e));
        let mut milestone = milestones.get(milestone_index).unwrap();

        // V6 — the milestone must be unsettled, with a positive stored amount.
        validate_milestone_releasable(
            &milestone,
            Self::is_settlement_marked(&env, contract_id, milestone_index),
        )
        .unwrap_or_else(|e| env.panic_with_error(e));

        let current_state = milestone_transitions::MilestoneState::from_milestone(&milestone)
            .unwrap_or_else(|e| env.panic_with_error(e));
        let requested_state = milestone_transitions::MilestoneState::Released;

        milestone_transitions::validate_milestone_transition(current_state, requested_state)
            .unwrap_or_else(|e| env.panic_with_error(e));

        // V7 — the release mode's approval quorum must be present.
        validate_approvals(&env, &contract, contract_id, milestone_index)
            .unwrap_or_else(|e| env.panic_with_error(e));

        // V8 — the single-milestone gross total.
        let mut gross_amounts: Vec<i128> = Vec::new(env);
        gross_amounts.push_back(milestone.amount);
        let gross_amount =
            checked_total(&gross_amounts).unwrap_or_else(|e| env.panic_with_error(e));

        // V9 — fee configuration and net payout, derived from one fee rate.
        let fee_bps =
            Self::effective_protocol_fee_bps(&env).unwrap_or_else(|e| env.panic_with_error(e));
        let protocol_fee = Self::protocol_fee_for(&env, gross_amount, fee_bps);
        let net_amount =
            net_payout(gross_amount, protocol_fee).unwrap_or_else(|e| env.panic_with_error(e));

        let accumulated_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);

        // V10 — the gross amount must be payable out of custody.
        ensure_available_balance(&contract, accumulated_fees, gross_amount)
            .unwrap_or_else(|e| env.panic_with_error(e));

        // Checks-Effects-Interactions: commit the settled flag atomically before
        // outward accounting.
        env.storage().persistent().set(
            &DataKey::MilestoneReleased(contract_id, milestone_index),
            &true,
        );

        milestone.released = true;
        milestones.set(milestone_index, milestone.clone());
        contract.released_amount = contract
            .released_amount
            .checked_add(net_amount)
            .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

        // ── Atomic Version/Actor Persistence ──────────────────────────────────
        // Record who performed this transition and increment the version
        milestone_transitions::store_milestone_transition(
            env,
            contract_id,
            milestone_index,
            caller.clone(),
        );

        // The protocol fee is retained in custody and accumulated exactly once,
        // using the same rate that produced `net_amount` above.
        let new_accumulated = accumulated_fees
            .checked_add(protocol_fee)
            .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
        if protocol_fee > 0 {
            env.storage()
                .persistent()
                .set(&DataKey::AccumulatedProtocolFees, &new_accumulated);
        }

        approvals::clear_approvals(&env, contract_id, milestone_index);

        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
            // Route through the single accrual policy owned by `lib.rs` so the
            // credit ledger cannot diverge between release paths.
            Self::grant_pending_reputation_credit(env, &contract.freelancer);
        }

        // V11 — the settled state must still be conservative.
        ensure_settlement_invariant(
            contract.funded_amount,
            contract.released_amount,
            contract.refunded_amount,
            new_accumulated,
        )
        .unwrap_or_else(|e| env.panic_with_error(e));

        env.storage().persistent().set(&milestone_key, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_and_milestones_ttl(env, contract_id);

        env.events().publish(
            (Symbol::new(&env, "milestone_released"), contract_id),
            (caller, milestone_index, milestone.amount),
        );

        true
    }

    /// Core logic for releasing several milestones in one atomic batch.
    ///
    /// The batch is validated as a whole (V1–V11) before any milestone is
    /// applied, so a batch is either fully released or fully untouched: there is
    /// no partial settlement to recover from. The per-index rules are exactly
    /// the single-release rules, so a caller cannot reach a state through the
    /// batch entrypoint that the single entrypoint would have refused.
    pub(crate) fn release_milestone_batch_impl(
        env: &Env,
        contract_id: u32,
        caller: Address,
        milestone_indices: Vec<u32>,
    ) -> bool {
        Self::require_not_paused(&env);
        caller.require_auth();

        // V1 — request shape, and V2 — duplicate-free, before any state read.
        validate_batch_shape(&milestone_indices).unwrap_or_else(|e| env.panic_with_error(e));
        validate_batch_indices_distinct(&milestone_indices)
            .unwrap_or_else(|e| env.panic_with_error(e));

        Self::require_not_finalized(&env, contract_id);

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);

        // V3 — lifecycle.
        validate_status(contract.status).unwrap_or_else(|e| env.panic_with_error(e));

        // V4 — the caller's role must be admitted by the contract's release mode.
        let (is_client, is_freelancer, is_arbiter) = Self::release_roles(&contract, &caller);
        validate_release_authority(
            contract.release_authorization,
            is_client,
            is_freelancer,
            is_arbiter,
        )
        .unwrap_or_else(|e| env.panic_with_error(e));

        let milestone_key = keys::milestone_key(&env, contract_id);
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&milestone_key)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_milestone_ttl(&env, contract_id);

        let batch_len = milestone_indices.len();
        let mut gross_amounts: Vec<i128> = Vec::new(env);
        for i in 0..batch_len {
            let milestone_index = milestone_indices.get(i).unwrap();

            // V5 — index within the schedule.
            validate_index(milestone_index, milestones.len())
                .unwrap_or_else(|e| env.panic_with_error(e));

            let milestone = milestones.get(milestone_index).unwrap();

            // V6 — unsettled, positive amount. A refunded milestone is rejected
            // here exactly as in the single-release path.
            validate_milestone_releasable(
                &milestone,
                Self::is_settlement_marked(&env, contract_id, milestone_index),
            )
            .unwrap_or_else(|e| env.panic_with_error(e));

            // V7 — approval quorum per target.
            validate_approvals(&env, &contract, contract_id, milestone_index)
                .unwrap_or_else(|e| env.panic_with_error(e));

            gross_amounts.push_back(milestone.amount);
        }

        // V8 — the batch's gross total, checked for overflow.
        let total_gross_amount =
            checked_total(&gross_amounts).unwrap_or_else(|e| env.panic_with_error(e));

        let accumulated_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);

        // V10 — the whole batch must be payable out of custody at once.
        ensure_available_balance(&contract, accumulated_fees, total_gross_amount)
            .unwrap_or_else(|e| env.panic_with_error(e));

        // V9 — one fee rate for the entire batch, validated once.
        let fee_bps =
            Self::effective_protocol_fee_bps(&env).unwrap_or_else(|e| env.panic_with_error(e));
        let mut protocol_fees: Vec<i128> = Vec::new(env);
        for i in 0..batch_len {
            protocol_fees.push_back(Self::protocol_fee_for(
                &env,
                gross_amounts.get(i).unwrap(),
                fee_bps,
            ));
        }
        let total_protocol_fees =
            checked_total(&protocol_fees).unwrap_or_else(|e| env.panic_with_error(e));

        // Pass 2: Atomic State Updates (Checks-Effects-Interactions)
        // All state changes happen before any token transfer
        for i in 0..batch_len {
            let milestone_index = milestone_indices.get(i).unwrap();
            let mut milestone = milestones.get(milestone_index).unwrap();

            env.storage().persistent().set(
                &DataKey::MilestoneReleased(contract_id, milestone_index),
                &true,
            );

            milestone.released = true;
            milestones.set(milestone_index, milestone.clone());

            let gross_amount = gross_amounts.get(i).unwrap();
            let net_amount = net_payout(gross_amount, protocol_fees.get(i).unwrap())
                .unwrap_or_else(|e| env.panic_with_error(e));

            contract.released_amount = contract
                .released_amount
                .checked_add(net_amount)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

            approvals::clear_approvals(&env, contract_id, milestone_index);

            env.events().publish(
                (Symbol::new(&env, "milestone_released"), contract_id),
                (caller.clone(), milestone_index, milestone.amount),
            );
        }

        // Atomically accumulate total protocol fees after all milestone updates
        if total_protocol_fees > 0 {
            let new_accumulated = accumulated_fees
                .checked_add(total_protocol_fees)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
            env.storage()
                .persistent()
                .set(&DataKey::AccumulatedProtocolFees, &new_accumulated);
        }

        let final_accumulated_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);

        // V11 — the batched post-state must still be conservative. This is the
        // same check the single-release path applies, so neither entrypoint can
        // leave the books over-committed. The rejection is raised after the
        // milestone writes but before the contract record is persisted, and a
        // panic reverts the whole invocation either way.
        ensure_settlement_invariant(
            contract.funded_amount,
            contract.released_amount,
            contract.refunded_amount,
            final_accumulated_fees,
        )
        .unwrap_or_else(|e| env.panic_with_error(e));

        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
            // Route through the single accrual policy owned by `lib.rs` so the
            // credit ledger cannot diverge between release paths.
            Self::grant_pending_reputation_credit(env, &contract.freelancer);
        }

        env.storage().persistent().set(&milestone_key, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_and_milestones_ttl(env, contract_id);

        true
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the pure release validation boundary.
    //!
    //! These pin every accepted and rejected branch and the boundary values
    //! (schedule length, batch length, index equality, exact balance, 100 % fee,
    //! `i128` overflow) without touching storage. End-to-end coverage of the
    //! same boundaries through the entrypoints lives in
    //! `crate::test::release_validation_boundaries`.

    use super::*;
    use soroban_sdk::{testutils::Address as _, vec, Address, Env};

    fn milestone(amount: i128) -> Milestone {
        Milestone {
            amount,
            funded_amount: amount,
            released: false,
            refunded: false,
            work_evidence: None,
            refunded_amount: 0,
            deadline: None,
        }
    }

    fn milestone_with(amount: i128, released: bool, refunded: bool) -> Milestone {
        let mut value = milestone(amount);
        value.released = released;
        value.refunded = refunded;
        value
    }

    fn contract(status: ContractStatus, funded: i128, released: i128, refunded: i128) -> Contract {
        let env = Env::default();
        Contract {
            client: Address::generate(&env),
            freelancer: Address::generate(&env),
            arbiter: None,
            status,
            total_deposited: funded,
            funded_amount: funded,
            released_amount: released,
            refunded_amount: refunded,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        }
    }

    // ── V1: batch shape ─────────────────────────────────────────────────────

    #[test]
    fn accepts_a_single_index_batch() {
        let env = Env::default();
        assert_eq!(validate_batch_shape(&vec![&env, 0_u32]), Ok(()));
    }

    #[test]
    fn accepts_a_batch_at_the_protocol_maximum() {
        let env = Env::default();
        let mut indices = Vec::new(&env);
        for index in 0..MAX_BATCH_MILESTONES {
            indices.push_back(index);
        }

        assert_eq!(validate_batch_shape(&indices), Ok(()));
    }

    #[test]
    fn rejects_an_empty_batch() {
        let env = Env::default();
        let indices = Vec::new(&env);
        assert_eq!(validate_batch_shape(&indices), Err(Error::EmptyBatch));
    }

    /// Boundary: one index past the protocol maximum is the first rejection.
    #[test]
    fn rejects_a_batch_one_past_the_protocol_maximum() {
        let env = Env::default();
        let mut indices = Vec::new(&env);
        for index in 0..=MAX_BATCH_MILESTONES {
            indices.push_back(index);
        }

        assert_eq!(
            validate_batch_shape(&indices),
            Err(Error::BatchLimitExceeded)
        );
    }

    /// The shape cap is checked before duplicates so a hostile oversized request
    /// is rejected by the cheaper bound.
    #[test]
    fn rejects_an_oversized_duplicate_batch_by_size() {
        let env = Env::default();
        let mut indices = Vec::new(&env);
        for _ in 0..=MAX_BATCH_MILESTONES {
            indices.push_back(3_u32);
        }

        assert_eq!(
            validate_batch_shape(&indices),
            Err(Error::BatchLimitExceeded)
        );
    }

    // ── V2: duplicate-free batch ────────────────────────────────────────────

    #[test]
    fn accepts_distinct_indices_in_any_order() {
        let env = Env::default();
        for indices in [
            vec![&env, 0_u32, 1_u32, 2_u32],
            vec![&env, 2_u32, 0_u32, 1_u32],
        ] {
            assert_eq!(validate_batch_indices_distinct(&indices), Ok(()));
        }
    }

    #[test]
    fn rejects_adjacent_duplicates() {
        let env = Env::default();
        let indices = vec![&env, 1_u32, 1_u32];
        assert_eq!(
            validate_batch_indices_distinct(&indices),
            Err(Error::DuplicateMilestoneInBatch)
        );
    }

    #[test]
    fn rejects_a_duplicate_spanning_the_batch() {
        let env = Env::default();
        let indices = vec![&env, 0_u32, 1_u32, 2_u32, 0_u32];
        assert_eq!(
            validate_batch_indices_distinct(&indices),
            Err(Error::DuplicateMilestoneInBatch)
        );
    }

    /// A single-element batch is trivially duplicate-free.
    #[test]
    fn accepts_a_single_index_as_duplicate_free() {
        let env = Env::default();
        assert_eq!(validate_batch_indices_distinct(&vec![&env, 7_u32]), Ok(()));
    }

    // ── V3: lifecycle ───────────────────────────────────────────────────────

    #[test]
    fn accepts_a_funded_contract() {
        assert_eq!(validate_status(ContractStatus::Funded), Ok(()));
    }

    #[test]
    fn rejects_every_non_funded_status() {
        for status in [
            ContractStatus::Created,
            ContractStatus::Accepted,
            ContractStatus::PartiallyFunded,
            ContractStatus::Disputed,
            ContractStatus::Completed,
            ContractStatus::Cancelled,
            ContractStatus::Refunded,
        ] {
            assert_eq!(
                validate_status(status),
                Err(Error::InvalidState),
                "{status:?} must not be releasable"
            );
        }
    }

    // ── V4: caller authority ────────────────────────────────────────────────

    #[test]
    fn authority_matrix_matches_every_release_mode() {
        use ReleaseAuthorization::*;

        // (mode, is_client, is_freelancer, is_arbiter, expected)
        let cases = [
            (ClientOnly, true, false, false, Ok(())),
            (ClientOnly, false, true, false, Err(Error::UnauthorizedRole)),
            (ClientOnly, false, false, true, Err(Error::UnauthorizedRole)),
            (ArbiterOnly, false, false, true, Ok(())),
            (
                ArbiterOnly,
                true,
                false,
                false,
                Err(Error::UnauthorizedRole),
            ),
            (ArbiterOnly, false, true, true, Ok(())),
            (ClientAndArbiter, true, false, false, Ok(())),
            (ClientAndArbiter, false, false, true, Ok(())),
            (
                ClientAndArbiter,
                false,
                true,
                false,
                Err(Error::UnauthorizedRole),
            ),
            (MultiSig, true, true, false, Ok(())),
            (MultiSig, false, true, false, Ok(())),
            (MultiSig, true, false, false, Ok(())),
            (MultiSig, false, false, true, Err(Error::UnauthorizedRole)),
        ];

        for (mode, is_client, is_freelancer, is_arbiter, expected) in cases {
            assert_eq!(
                validate_release_authority(mode, is_client, is_freelancer, is_arbiter),
                expected,
                "{mode:?} with client={is_client} freelancer={is_freelancer} \
                 arbiter={is_arbiter}"
            );
        }
    }

    /// A caller that matches no role is rejected in every mode.
    #[test]
    fn authority_rejects_a_stranger_in_every_mode() {
        for mode in [
            ReleaseAuthorization::ClientOnly,
            ReleaseAuthorization::ArbiterOnly,
            ReleaseAuthorization::ClientAndArbiter,
            ReleaseAuthorization::MultiSig,
        ] {
            assert_eq!(
                validate_release_authority(mode, false, false, false),
                Err(Error::UnauthorizedRole),
                "{mode:?} must reject a stranger"
            );
        }
    }

    // ── V5: index bounds ────────────────────────────────────────────────────

    #[test]
    fn accepts_the_first_and_last_index() {
        assert_eq!(validate_index(0, 3), Ok(()));
        assert_eq!(validate_index(2, 3), Ok(()));
    }

    /// Boundary: a single-milestone schedule accepts index 0 only.
    #[test]
    fn accepts_index_zero_of_a_single_milestone_schedule() {
        assert_eq!(validate_index(0, 1), Ok(()));
    }

    /// Boundary: `index == schedule_len` is the first rejected index.
    #[test]
    fn rejects_an_index_equal_to_the_schedule_length() {
        assert_eq!(validate_index(3, 3), Err(Error::IndexOutOfBounds));
    }

    #[test]
    fn rejects_an_index_above_the_schedule_length() {
        assert_eq!(validate_index(4, 3), Err(Error::IndexOutOfBounds));
        assert_eq!(validate_index(u32::MAX, 3), Err(Error::IndexOutOfBounds));
    }

    /// An empty schedule has no valid index at all, including index 0.
    #[test]
    fn rejects_every_index_of_an_empty_schedule() {
        assert_eq!(validate_index(0, 0), Err(Error::IndexOutOfBounds));
    }

    // ── V6: per-milestone releasability ─────────────────────────────────────

    #[test]
    fn accepts_a_pending_milestone() {
        assert_eq!(validate_milestone_releasable(&milestone(10), false), Ok(()));
    }

    #[test]
    fn accepts_the_minimum_positive_amount() {
        assert_eq!(validate_milestone_releasable(&milestone(1), false), Ok(()));
    }

    #[test]
    fn rejects_a_released_milestone() {
        assert_eq!(
            validate_milestone_releasable(&milestone_with(10, true, false), false),
            Err(Error::MilestoneAlreadyReleased)
        );
    }

    /// The settlement marker is an independent witness: it rejects a release
    /// even when the milestone flags still say "pending".
    #[test]
    fn rejects_a_marked_milestone_whose_flag_was_cleared() {
        assert_eq!(
            validate_milestone_releasable(&milestone(10), true),
            Err(Error::MilestoneAlreadyReleased)
        );
    }

    /// Regression: the single-release path used to fall through to the state
    /// matrix for a refunded milestone and report `InvalidStatusTransition`,
    /// while the batch path reported `AlreadyRefunded`.
    #[test]
    fn rejects_a_refunded_milestone_with_a_settled_specific_error() {
        assert_eq!(
            validate_milestone_releasable(&milestone_with(10, false, true), false),
            Err(Error::AlreadyRefunded)
        );
    }

    /// A milestone that is both released and refunded is reported as already
    /// released: settlement is checked before corruption is interpreted.
    #[test]
    fn rejects_a_both_flags_milestone_as_already_released() {
        assert_eq!(
            validate_milestone_releasable(&milestone_with(10, true, true), false),
            Err(Error::MilestoneAlreadyReleased)
        );
    }

    /// A non-positive stored amount can only be corrupt state; releasing it
    /// would move `released_amount` backwards.
    #[test]
    fn rejects_a_non_positive_stored_amount() {
        for amount in [0, -1, i128::MIN] {
            assert_eq!(
                validate_milestone_releasable(&milestone(amount), false),
                Err(Error::InvalidMilestoneAmount),
                "amount {amount} must be refused"
            );
        }
    }

    // ── V8: gross total ─────────────────────────────────────────────────────

    #[test]
    fn sums_every_amount() {
        let env = Env::default();
        let amounts = vec![&env, 10_i128, 20_i128, 30_i128];
        assert_eq!(checked_total(&amounts), Ok(60));
    }

    #[test]
    fn an_empty_amount_list_sums_to_zero() {
        let env = Env::default();
        assert_eq!(checked_total(&Vec::new(&env)), Ok(0));
    }

    /// Boundary: a single stroop is a valid gross amount.
    #[test]
    fn accepts_a_single_stroop_amount() {
        let env = Env::default();
        assert_eq!(checked_total(&vec![&env, 1_i128]), Ok(1));
    }

    #[test]
    fn reports_overflow_instead_of_wrapping() {
        let env = Env::default();
        let amounts = vec![&env, i128::MAX, 1_i128];
        assert_eq!(checked_total(&amounts), Err(Error::PotentialOverflow));
    }

    /// Boundary: the largest representable single amount is accepted.
    #[test]
    fn accepts_the_i128_ceiling_as_a_single_amount() {
        let env = Env::default();
        assert_eq!(checked_total(&vec![&env, i128::MAX]), Ok(i128::MAX));
    }

    // ── V9: protocol fee ────────────────────────────────────────────────────

    #[test]
    fn accepts_fee_rates_within_the_ceiling() {
        for fee_bps in [0, 1, 5_000, MAX_FEE_BPS] {
            assert_eq!(validate_fee_bps(fee_bps), Ok(()), "rate {fee_bps}");
        }
    }

    /// Boundary: exactly 100 % is the highest admissible rate.
    #[test]
    fn rejects_a_fee_rate_above_the_ceiling() {
        assert_eq!(
            validate_fee_bps(MAX_FEE_BPS + 1),
            Err(Error::InvalidProtocolParameters)
        );
        assert_eq!(
            validate_fee_bps(u32::MAX),
            Err(Error::InvalidProtocolParameters)
        );
    }

    #[test]
    fn net_payout_subtracts_the_fee() {
        assert_eq!(net_payout(1_000, 100), Ok(900));
    }

    /// Boundary: a zero fee pays the gross amount in full, and a 100 % fee pays
    /// nothing — both are legal.
    #[test]
    fn net_payout_accepts_both_fee_ceilings() {
        assert_eq!(net_payout(1_000, 0), Ok(1_000));
        assert_eq!(net_payout(1_000, 1_000), Ok(0));
    }

    /// Regression: a corrupt fee configuration above 100 % used to produce a
    /// negative net amount that silently shrank `released_amount`.
    #[test]
    fn net_payout_refuses_a_fee_above_the_gross_amount() {
        assert_eq!(
            net_payout(1_000, 1_001),
            Err(Error::AccountingInvariantViolated)
        );
    }

    #[test]
    fn net_payout_reports_a_refundable_difference_as_a_violation() {
        assert_eq!(
            net_payout(i128::MIN, 1),
            Err(Error::AccountingInvariantViolated)
        );
    }

    // ── V10: custody ────────────────────────────────────────────────────────

    #[test]
    fn available_balance_subtracts_released_refunded_and_fees() {
        let subject = contract(ContractStatus::Funded, 1_000, 300, 200);
        assert_eq!(available_balance(&subject, 50), Ok(450));
    }

    #[test]
    fn available_balance_is_zero_when_everything_is_committed() {
        let subject = contract(ContractStatus::Funded, 1_000, 500, 400);
        assert_eq!(available_balance(&subject, 100), Ok(0));
    }

    /// An over-committed contract yields a representable negative balance, which
    /// the balance gate then reports as `InsufficientFunds`.
    #[test]
    fn available_balance_is_negative_for_broken_accounting() {
        let subject = contract(ContractStatus::Funded, 1_000, 1_001, 0);
        assert_eq!(available_balance(&subject, 0), Ok(-1));
        assert_eq!(
            ensure_available_balance(&subject, 0, 1),
            Err(Error::InsufficientFunds)
        );
    }

    #[test]
    fn available_balance_reports_overflow_for_unrepresentable_accounting() {
        let subject = contract(ContractStatus::Funded, i128::MIN, 1, 0);
        assert_eq!(
            available_balance(&subject, 0),
            Err(Error::PotentialOverflow)
        );
    }

    /// Boundary: releasing exactly the remaining balance is accepted.
    #[test]
    fn balance_gate_accepts_the_exact_remaining_amount() {
        let subject = contract(ContractStatus::Funded, 1_000, 300, 200);
        assert_eq!(ensure_available_balance(&subject, 50, 450), Ok(()));
    }

    /// Boundary: one stroop beyond it is rejected.
    #[test]
    fn balance_gate_rejects_one_stroop_beyond_the_balance() {
        let subject = contract(ContractStatus::Funded, 1_000, 300, 200);
        assert_eq!(
            ensure_available_balance(&subject, 50, 451),
            Err(Error::InsufficientFunds)
        );
    }

    /// Unrepresentable accounting surfaces before the affordability verdict, so
    /// the caller learns about the corruption instead of a misleading shortfall.
    #[test]
    fn balance_gate_surfaces_unrepresentable_accounting_first() {
        let subject = contract(ContractStatus::Funded, i128::MIN, 1, 0);
        assert_eq!(
            ensure_available_balance(&subject, 0, 1),
            Err(Error::PotentialOverflow)
        );
    }

    // ── V11: post-state conservation ────────────────────────────────────────

    #[test]
    fn settlement_invariant_accepts_a_fully_committed_contract() {
        assert_eq!(
            ensure_settlement_invariant(1_000, 900, 100, 0),
            Ok(()),
            "a fully settled contract is conservative"
        );
    }

    /// Boundary: the committed sum may equal but never exceed the funded amount.
    #[test]
    fn settlement_invariant_accepts_equality_but_not_one_stroop_more() {
        assert_eq!(ensure_settlement_invariant(1_000, 1_000, 0, 0), Ok(()));
        assert_eq!(
            ensure_settlement_invariant(1_000, 1_000, 0, 1),
            Err(Error::AccountingInvariantViolated)
        );
    }

    /// The protocol fee counts as committed: it left the freelancer's payout and
    /// is retained in custody.
    #[test]
    fn settlement_invariant_counts_accumulated_fees() {
        assert_eq!(
            ensure_settlement_invariant(1_000, 900, 0, 101),
            Err(Error::AccountingInvariantViolated)
        );
    }

    #[test]
    fn settlement_invariant_reports_unrepresentable_sums_as_a_violation() {
        assert_eq!(
            ensure_settlement_invariant(i128::MAX, i128::MAX, 1, 0),
            Err(Error::AccountingInvariantViolated)
        );
    }

    /// A negative committed sum is representable and stays under the funded
    /// amount, so it is not a conservation violation.
    #[test]
    fn settlement_invariant_accepts_negative_accounting() {
        assert_eq!(ensure_settlement_invariant(0, -10, -20, 0), Ok(()));
    }
}
