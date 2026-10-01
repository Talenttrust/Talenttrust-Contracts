//! Milestone approval storage and release authorization checks.
//!
//! This module owns the
//! `DataKey::MilestoneApprovals(contract_id, milestone_index)` records used by
//! `approve_milestone_release`, `revoke_milestone_approval`, and
//! `release_milestone`. It reads the escrow contract and milestone vector to
//! validate state and role authorization, but it does not move funds or mutate
//! milestone accounting.
//!
//! Approval records live in Soroban temporary storage and expire according to
//! `PENDING_APPROVAL_TTL_LEDGERS`. Missing or expired approvals fail closed.
//!
//! # Approval state machine
//!
//! An approval record is a set of at most three independent, per-party boolean
//! flags (`client_approved`, `freelancer_approved`, `arbiter_approved`) plus
//! the Soroban temporary-entry TTL. Which flags are *required* is derived
//! purely from the contract's [`ReleaseAuthorization`] mode:
//!
//! | Mode | Required flags |
//! | --- | --- |
//! | `ClientOnly` | client |
//! | `ArbiterOnly` | arbiter |
//! | `ClientAndArbiter` | client **or** arbiter |
//! | `MultiSig` | client **and** freelancer |
//!
//! # Invariants
//!
//! The following invariants hold for every code path in this module. They are
//! the contract that makes failure recovery deterministic.
//!
//! **I1 — Fail closed.** A missing or TTL-evicted record is indistinguishable
//! from "never approved" and always denies release. Eviction can therefore
//! never *widen* authority; it can only force a fresh approval.
//!
//! **I2 — Revocation is authority-reducing and monotonic.** [`revoke_approval`]
//! can only clear a flag that is already set, and only the caller's own flag.
//! It has no path that sets a flag. Consequently revocation can never move a
//! milestone from *insufficient* to *sufficient* approvals, and it is safe to
//! allow from any contract state — there is no interleaving of concurrent
//! revoke/approve calls that can produce a release that neither party agreed to.
//!
//! **I3 — Revocation never extends a deadline.** A revoke clears a flag but
//! does not bump the temporary entry's TTL, so the remaining parties' approvals
//! keep the expiry they were originally granted. Recovery therefore cannot be
//! used to silently prolong an approval window. To obtain a fresh window a
//! party revokes and re-approves, which resets the TTL as a deliberate act.
//!
//! **I4 — Empty records are removed.** When the last set flag is cleared the
//! whole record is removed rather than persisted as an all-false entry, so the
//! observable state after full revocation is byte-identical to the state before
//! any approval was given.
//!
//! **I5 — Terminal milestones are immutable.** Neither approving nor revoking
//! is possible once a milestone is released
//! ([`Error::MilestoneAlreadyReleased`]). A revoked record can never be
//! resurrected for a settled milestone.
//!
//! **I6 — Disputes void approvals.** A dispute is an assertion that the prior
//! release authorization was invalid. [`raise_dispute`](crate::Escrow::raise_dispute),
//! [`rollback_dispute`](crate::Escrow::rollback_dispute), and
//! [`resolve_dispute`](crate::Escrow::resolve_dispute) therefore all call
//! [`clear_all_approvals`]. Without this, an approval recorded *before* a
//! dispute would be restored to a releasable state by a rollback and could be
//! spent without any party re-consenting.
//!
//! **I7 — Fail-closed and idempotent under replay.** Soroban serialises
//! execution inside one ledger, so the "concurrency" this module actually faces
//! is *replayed, duplicated, or interleaved* calls across transactions (a
//! client retrying after a timeout, an indexer resubmitting, two parties racing
//! to approve). Every mutating path is idempotent with respect to the
//! observable state:
//!
//! * Re-approving an already-set flag is rejected with
//!   [`Error::AlreadyApproved`] and writes nothing, so a retried approval can
//!   never inflate the approval count or flip another party's flag.
//! * Revocation is authority-reducing only (I2) and clears at most the caller's
//!   own flag, so any approve/revoke interleaving converges on a state that is
//!   no more releasable than the sequential result.
//! * Revoking with no live record is a typed, inert rejection
//!   ([`Error::InsufficientApprovals`]) that leaves storage untouched, making a
//!   retry after an unknown outcome a safe no-op.
//! * [`release_readiness`] and [`check_approvals`] mutate nothing, so polling
//!   them cannot change (or prolong the TTL of) the state they report (I3).
//!
//! Together with **I1**, this is the *fail-closed + idempotent under replay*
//! invariant: any prefix of a repeated or reordered call sequence converges on
//! a state identical to executing it once, or strictly *less* authorised —
//! never more. Replaying or reordering calls can therefore never accumulate a
//! release permission that a single honest execution would not have produced.
//!
//! # Failure recovery
//!
//! Every recoverable approval failure has exactly one deterministic repair, and
//! no repair requires waiting out the TTL:
//!
//! | Situation | Repair |
//! | --- | --- |
//! | Approval recorded but should not have been | `revoke_milestone_approval` |
//! | Partial `MultiSig` set, one party unavailable | Wait for the party, or the arbiter resolves the dispute |
//! | Record evicted by TTL, release denied with `InsufficientApprovals` | Re-approve; [`MilestoneReleaseReadiness`] shows the record is gone |
//! | Release denied and the cause is unclear | [`get_milestone_release_readiness`](crate::Escrow::get_milestone_release_readiness) |
//! | Dispute opened, then rolled back | Approvals are already void (I6); re-approve |
//!
//! Duplicate approval remains a typed rejection ([`Error::AlreadyApproved`])
//! rather than a silent success, so "I did not mean to do this" is always
//! distinguishable from "this was already done". Because approval and
//! revocation are both flag operations against on-ledger state, a retry after
//! an unknown outcome (dropped receipt, RPC timeout) is always safe: read the
//! state with `get_milestone_release_readiness`, then either re-approve,
//! revoke, or take no action.

use crate::keys;
use crate::ttl::{PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS};
use crate::types::{
    AuthorizationRecord, Contract, ContractStatus, DataKey, Error, Milestone, MilestoneApprovals,
    MilestoneReleaseReadiness, ReleaseAuthorization, MAX_PAGINATION_LIMIT,
};
use soroban_sdk::{Address, Env, Vec};

/// Which of a contract's three approval flags a caller controls.
///
/// Role resolution is performed in exactly one place ([`resolve_role`]) and is
/// shared by the approval and revocation paths. Having a single resolver is
/// what guarantees that a caller can always revoke precisely the flag they
/// were able to set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalRole {
    Client,
    Freelancer,
    Arbiter,
}

impl ApprovalRole {
    /// Returns `true` when this role's flag is currently set.
    fn is_set(self, approvals: &MilestoneApprovals) -> bool {
        match self {
            Self::Client => approvals.client_approved,
            Self::Freelancer => approvals.freelancer_approved,
            Self::Arbiter => approvals.arbiter_approved,
        }
    }

    /// Clears this role's flag in `approvals`, returning `true` if it changed.
    ///
    /// The caller is responsible for rejecting an already-clear flag; this
    /// helper only performs the mutation.
    fn clear(self, approvals: &mut MilestoneApprovals) -> bool {
        let was_set = self.is_set(approvals);
        match self {
            Self::Client => approvals.client_approved = false,
            Self::Freelancer => approvals.freelancer_approved = false,
            Self::Arbiter => approvals.arbiter_approved = false,
        }
        was_set
    }

    /// Returns `true` when no role has a set flag.
    fn record_is_empty(approvals: &MilestoneApprovals) -> bool {
        !approvals.client_approved && !approvals.freelancer_approved && !approvals.arbiter_approved
    }
}

/// Resolves `caller` to exactly one approval role for `contract`.
///
/// Returns [`Error::UnauthorizedRole`] when the caller is not one of the
/// contract's three parties, or when the contract's
/// [`ReleaseAuthorization`] mode does not permit that role to approve.
///
/// # Role-overlap precedence
///
/// When a contract records the same address in more than one role, precedence
/// is client > freelancer > arbiter. This matches the historical approval
/// path exactly, so resolving a role is stable across the approve/revoke pair:
/// a caller never has one flag set under one resolution and a different flag
/// cleared under another.
pub fn resolve_role(contract: &Contract, caller: &Address) -> Result<ApprovalRole, Error> {
    if caller == &contract.client {
        return Ok(ApprovalRole::Client);
    }
    if caller == &contract.freelancer {
        return Ok(ApprovalRole::Freelancer);
    }
    if contract.arbiter.as_ref() == Some(caller) {
        return Ok(ApprovalRole::Arbiter);
    }
    Err(Error::UnauthorizedRole)
}

/// Returns the roles permitted to approve under `mode`.
///
/// Used to enforce the release mode without duplicating the match in the
/// approval and revocation paths.
fn mode_permits(mode: ReleaseAuthorization, role: ApprovalRole) -> bool {
    match mode {
        ReleaseAuthorization::ClientOnly => role == ApprovalRole::Client,
        ReleaseAuthorization::ArbiterOnly => role == ApprovalRole::Arbiter,
        ReleaseAuthorization::ClientAndArbiter => {
            matches!(role, ApprovalRole::Client | ApprovalRole::Arbiter)
        }
        ReleaseAuthorization::MultiSig => {
            matches!(role, ApprovalRole::Client | ApprovalRole::Freelancer)
        }
    }
}

/// Computes `(present, required)` approval counts for `mode` against a record.
///
/// A missing record counts as zero present approvals, which keeps this
/// consistent with the fail-closed rule (I1). `MultiSig` is the only mode that
/// requires two approvals; every other mode requires exactly one, counted as
/// satisfied when *any* single permitted flag is set.
fn approval_counts(
    mode: ReleaseAuthorization,
    approvals: Option<&MilestoneApprovals>,
) -> (u32, u32) {
    let (client, freelancer, arbiter) = match approvals {
        Some(v) => (v.client_approved, v.freelancer_approved, v.arbiter_approved),
        None => (false, false, false),
    };

    match mode {
        ReleaseAuthorization::ClientOnly => (u32::from(client), 1),
        ReleaseAuthorization::ArbiterOnly => (u32::from(arbiter), 1),
        ReleaseAuthorization::ClientAndArbiter => (u32::from(client || arbiter), 1),
        ReleaseAuthorization::MultiSig => (u32::from(client) + u32::from(freelancer), 2),
    }
}

/// Loads the milestone vector for `contract_id`, or `None` if absent.
fn load_milestones(env: &Env, contract_id: u32) -> Option<Vec<Milestone>> {
    env.storage()
        .persistent()
        .get(&crate::ttl::milestone_storage_key(env, contract_id))
}

/// Approves a milestone for release by the caller.
///
/// Records the approval in temporary storage, so it expires with the rest of
/// the transient approval record after `PENDING_APPROVAL_TTL_LEDGERS`. A
/// partially-collected multi-sig quorum does not silently reset before then:
/// the entry's TTL is renewed on every approval, so consent stays live until
/// the whole record is left untouched for the full window.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `milestone_index` - The index of the milestone to approve
/// * `caller` - The address of the caller. In MultiSig mode, exactly the
///   client and freelancer can approve, and both approvals are required.
///
/// # Returns
/// `true` if approval was recorded successfully
///
/// # Errors
/// * `ContractNotFound` - If contract doesn't exist
/// * `InvalidState` - If contract is not in `Funded` or `PartiallyFunded` state
///   (including `Disputed`, which is release-locked)
/// * `IndexOutOfBounds` - If milestone index is invalid
/// * `MilestoneAlreadyReleased` - If milestone was already released
/// * `UnauthorizedRole` - If caller is not authorized to approve
/// * `AlreadyApproved` - If caller has already approved this milestone
///
/// # Determinism and retry
///
/// This is a **flag-setting** operation, so it is not idempotent by design: a
/// second approval by the same party is rejected with `AlreadyApproved` rather
/// than silently succeeding. That keeps "I did not intend this" distinguishable
/// from "this was already recorded", which matters when a client retries after
/// an unknown outcome.
///
/// Because approval only ever sets a previously-clear flag, and the flag is
/// bounded by the 7-day TTL, a retry can never widen authority. After any
/// unknown outcome the caller can deterministically inspect the result with
/// [`Escrow::get_milestone_release_readiness`](crate::Escrow::get_milestone_release_readiness)
/// and then choose to re-approve, to
/// [`revoke_approval`](crate::Escrow::revoke_milestone_approval), or to do
/// nothing. A mistaken approval is never a dead end.
///
/// # Security
/// - Caller must be authenticated via require_auth()
/// - Only parties authorized by the contract's release mode can approve
/// - Approvals live in temporary storage and fail closed once evicted (I1)
/// - Duplicate approvals from the same party are rejected
pub fn approve_milestone(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    caller: &Address,
) -> Result<bool, Error> {
    // Load contract
    let contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .ok_or(Error::ContractNotFound)?;

    // A contract under dispute is locked: releases and approval writes must
    // fail closed until the arbiter resolves the dispute via the authorized
    // flow. This preserves the ordering guarantee that funds are never released
    // while a dispute is active.
    validate_approvable_state(&contract)?;

    // Load milestones
    let milestones: Vec<Milestone> =
        load_milestones(env, contract_id).ok_or(Error::ContractNotFound)?;

    // Validate milestone index
    validate_milestone_index(&milestones, milestone_index)?;

    let milestone = milestones.get(milestone_index).unwrap();

    // Check if milestone is already released
    validate_milestone_not_released(&milestone)?;

    // Resolve the caller's role and enforce the contract's release mode.
    //
    // `resolve_role` rejects any non-participant; `mode_permits` then rejects
    // participants the current mode does not allow. Both revocation and
    // approval share this resolution so a caller can always withdraw exactly
    // the flag they were able to set.
    let role = resolve_role(&contract, caller)?;
    if !mode_permits(contract.release_authorization, role) {
        return Err(Error::UnauthorizedRole);
    }

    // Load or create approval record from temporary storage, matching every
    // reader (`check_approvals`, `revoke_approval`, `release_readiness`,
    // `get_milestone_approvals`). Writing to persistent here would make the
    // record invisible to the revoke/readiness paths and would allow approvals
    // to outlive the documented TTL window, widening authority.
    let approval_key = keys::milestone_approval_key(env, contract_id, milestone_index);
    let mut approvals: MilestoneApprovals =
        env.storage()
            .temporary()
            .get(&approval_key)
            .unwrap_or(MilestoneApprovals {
                client_approved: false,
                freelancer_approved: false,
                arbiter_approved: false,
            });

    // Reject a duplicate approval from the same party, then set that party's
    // single flag. Only the resolved role's flag is ever touched, so one call
    // can never set two flags.
    if role.is_set(&approvals) {
        return Err(Error::AlreadyApproved);
    }
    match role {
        ApprovalRole::Client => approvals.client_approved = true,
        ApprovalRole::Freelancer => approvals.freelancer_approved = true,
        ApprovalRole::Arbiter => approvals.arbiter_approved = true,
    }

    // Store approval in temporary storage and renew its TTL. A live quorum is
    // kept alive by each approval; an abandoned one expires and fails closed.
    env.storage().temporary().set(&approval_key, &approvals);

    env.storage().temporary().extend_ttl(
        &approval_key,
        PENDING_APPROVAL_BUMP_THRESHOLD,
        PENDING_APPROVAL_TTL_LEDGERS,
    );

    Ok(true)
}

/// What a successful [`revoke_approval`] actually did.
///
/// Returned so the caller and the emitted event can distinguish the two
/// outcomes without a second read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevocationOutcome {
    /// The caller's flag was cleared and at least one other party's approval
    /// remains live, so the temporary record was kept and its TTL left alone.
    FlagCleared,
    /// The caller's flag was the last one set, so the whole temporary record
    /// was removed. The observable state is now identical to "never approved".
    RecordRemoved,
}

/// Outcome of a revocation, including the role whose flag was cleared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RevocationRecord {
    pub role: ApprovalRole,
    pub outcome: RevocationOutcome,
    /// `true` when other parties' approvals remain live after the revoke.
    pub other_approvals_remain: bool,
}

/// Withdraws the caller's own approval for a milestone.
///
/// This is the recovery path for an approval that was recorded but should not
/// stand — for example a client who approved a milestone before discovering
/// that the deliverable was incomplete. Without it the only exits are a
/// successful release (which consumes the approval) or waiting out the
/// 7-day TTL, which reads to the user as being stuck.
///
/// # Guarantees
///
/// * **Own flag only.** A caller can clear only the flag that
///   [`resolve_role`] maps them to; other parties' flags are never touched, so
///   one party cannot sabotage a `MultiSig` set it is not part of.
/// * **Authority-reducing only (I2).** There is no branch that sets a flag, so
///   revocation can never make a milestone *more* releasable. It is therefore
///   safe to permit from any contract state and safe under concurrent
///   approve/revoke interleavings.
/// * **No deadline extension (I3).** A revoke never bumps the temporary TTL, so
///   the remaining parties keep the expiry they were granted. Recovery cannot
///   be used to prolong an approval window; a fresh window requires an
///   explicit revoke-then-approve cycle.
/// * **Observable (I4).** When the last flag is cleared the record is removed
///   entirely rather than left as an all-false entry.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `milestone_index` - The index of the milestone to withdraw approval for
/// * `caller` - The party withdrawing. Must be a participant of the contract.
///
/// # Returns
/// `Ok(RevocationRecord)` describing which flag was cleared and whether the
/// record was removed.
///
/// # Errors
/// * `ContractNotFound` - If the contract or its milestone vector is missing
/// * `UnauthorizedRole` - If `caller` is not the client, freelancer, or arbiter
/// * `IndexOutOfBounds` - If `milestone_index` is not a valid milestone
/// * `MilestoneAlreadyReleased` - If the milestone is already released (I5)
/// * `InsufficientApprovals` - If no live approval record exists (never
///   approved, already fully revoked, or evicted by TTL), or if the caller's own
///   flag is not currently set. Both cases leave storage untouched, so a retry
///   is a safe no-op.
///
/// # Security
/// - The caller must additionally prove control of `caller` via
///   `require_auth()` in the calling entrypoint; this helper does not.
///
/// Note that no contract-status gate is applied. Revocation strictly reduces
/// authority, so keeping it available in every state — rather than mirroring
/// the approval gate — is what makes it a reliable recovery path for records
/// left behind by an earlier state transition.
pub fn revoke_approval(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    caller: &Address,
) -> Result<RevocationRecord, Error> {
    let contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .ok_or(Error::ContractNotFound)?;

    let role = resolve_role(&contract, caller)?;

    let milestones: Vec<Milestone> =
        load_milestones(env, contract_id).ok_or(Error::ContractNotFound)?;

    if milestone_index >= milestones.len() {
        return Err(Error::IndexOutOfBounds);
    }

    let milestone = milestones.get(milestone_index).unwrap();

    // A released milestone is settled; its approval was already consumed by
    // the release, so there is nothing to withdraw and nothing may be
    // reintroduced (I5).
    if milestone.released {
        return Err(Error::MilestoneAlreadyReleased);
    }

    let approval_key = keys::milestone_approval_key(contract_id, milestone_index);

    // Fail closed and stay inert when there is no live record. This collapses
    // "never approved", "already fully revoked", and "evicted by TTL" into one
    // typed, retryable error rather than writing an empty record (I1, I4).
    let mut approvals: MilestoneApprovals = env
        .storage()
        .temporary()
        .get(&approval_key)
        .ok_or(Error::InsufficientApprovals)?;

    // Only the caller's own flag may be cleared, and only if it is set.
    if !role.is_set(&approvals) {
        return Err(Error::InsufficientApprovals);
    }

    role.clear(&mut approvals);

    let outcome = if ApprovalRole::record_is_empty(&approvals) {
        // I4: drop the entry entirely so the post-state matches a contract on
        // which no approval was ever given.
        env.storage().temporary().remove(&approval_key);
        RevocationOutcome::RecordRemoved
    } else {
        // I3: persist the reduced record but deliberately do NOT extend the TTL,
        // so surviving approvals keep their original expiry.
        env.storage().temporary().set(&approval_key, &approvals);
        RevocationOutcome::FlagCleared
    };

    let other_approvals_remain = matches!(outcome, RevocationOutcome::FlagCleared);

    Ok(RevocationRecord {
        role,
        outcome,
        other_approvals_remain,
    })
}

/// Removes every live approval record belonging to `contract_id`.
///
/// Used on transitions that void outstanding release authorization
/// (dispute open, dispute rollback, dispute resolution, cancellation). See
/// invariant **I6**: an approval recorded before a dispute must not become
/// releasable again once the dispute is rolled back, because no party
/// re-consented in the meantime.
///
/// Bounded by the contract's own milestone count, so the work is O(milestones)
/// and cannot be driven unbounded by caller input.
///
/// Each key is probed with `has` before removal. Most contracts have no
/// outstanding approvals when they reach a terminal transition, and skipping
/// the removal keeps this a read-only no-op in that case — the call sites
/// include gas-budget assertions, and issuing a storage write for an entry
/// that does not exist would be pure waste. Removing an absent key is already
/// a no-op, so the guard changes behaviour only by avoiding that cost.
///
/// The terminal-state transitions that call this have already proven they may
/// mutate, so this deliberately does not re-check pause, initialization, or
/// authorization.
pub fn clear_all_approvals(env: &Env, contract_id: u32) {
    let milestones = match load_milestones(env, contract_id) {
        Some(m) => m,
        None => return,
    };

    for index in 0..milestones.len() {
        let key = keys::milestone_approval_key(contract_id, index);
        if env.storage().temporary().has(&key) {
            env.storage().temporary().remove(&key);
        }
    }
}

/// Checks if a milestone has sufficient approvals for release.
///
/// Missing approvals are treated as absent and return InsufficientApprovals.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract` - The contract data
/// * `contract_id` - The contract ID
/// * `milestone_index` - The milestone index
///
/// # Returns
/// * `Ok(true)` - If sufficient approvals exist and are valid
/// * `Err(InsufficientApprovals)` - If approvals are missing or insufficient
///
/// # Security
/// - Fail-closed: missing approvals prevent release
/// - MultiSig requires both client and freelancer approvals
/// - TTL expiry is enforced by Soroban's temporary storage
///
/// # Note on expiry
///
/// An expired record and a never-created record are **indistinguishable**:
/// Soroban evicts temporary entries silently, so a `None` read carries no
/// history. Both are reported as [`Error::InsufficientApprovals`] — there is no
/// `ApprovalExpired` error, and none should be inferred from one. This is
/// deliberate (I1): eviction must never widen authority. Because both cases
/// require the same repair, a fresh approval, the ambiguity is not actionable
/// and exposing it would only invite callers to branch on a state the host
/// does not preserve. Use
/// [`release_readiness`](crate::Escrow::get_milestone_release_readiness) to
/// learn whether a record is currently live before attempting a release.
pub fn check_approvals(
    env: &Env,
    contract: &Contract,
    contract_id: u32,
    milestone_index: u32,
) -> Result<bool, Error> {
    let approval_key = keys::milestone_approval_key(env, contract_id, milestone_index);

    // Load approvals from temporary storage — the same space
    // `approve_milestone` writes to. A record that was never created and one
    // evicted by TTL are both `None`, and both fail closed (I1).
    let approvals: Option<MilestoneApprovals> = env.storage().temporary().get(&approval_key);

    // If no approvals exist, fail closed.
    let approvals = approvals.ok_or(Error::InsufficientApprovals)?;

    // Check if required approvals are present based on authorization mode
    let sufficient = match contract.release_authorization {
        ReleaseAuthorization::ClientOnly => approvals.client_approved,
        ReleaseAuthorization::ArbiterOnly => approvals.arbiter_approved,
        ReleaseAuthorization::ClientAndArbiter => {
            approvals.client_approved || approvals.arbiter_approved
        }
        ReleaseAuthorization::MultiSig => {
            approvals.client_approved && approvals.freelancer_approved
        }
    };

    if sufficient {
        Ok(true)
    } else {
        Err(Error::InsufficientApprovals)
    }
}

/// Clears approval records for a milestone after successful release.
///
/// This prevents approval reuse and cleans up temporary storage.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `milestone_index` - The milestone index
pub fn clear_approvals(env: &Env, contract_id: u32, milestone_index: u32) {
    let approval_key = keys::milestone_approval_key(env, contract_id, milestone_index);
    env.storage().temporary().remove(&approval_key);
}

/// Returns a bounded, paginated read view of authorization records for a contract's milestones.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `contract_id` - Contract ID
/// * `start` - 0-based milestone index to start from
/// * `limit` - Maximum records to return (capped by MAX_PAGINATION_LIMIT)
///
/// # Returns
/// A `Vec<AuthorizationRecord>` slice of authorization records for the specified range.
/// Empty-safe: returns empty vector for unknown contracts, out-of-range bounds, or limit == 0.
pub fn get_authorization_records(
    env: &Env,
    contract_id: u32,
    start: u32,
    limit: u32,
) -> Vec<AuthorizationRecord> {
    if limit == 0 {
        return Vec::new(env);
    }

    let milestones: Option<Vec<Milestone>> = env
        .storage()
        .persistent()
        .get(&crate::ttl::milestone_storage_key(env, contract_id));

    let milestones = match milestones {
        Some(m) => m,
        None => return Vec::new(env),
    };

    let total = milestones.len();
    if start >= total {
        return Vec::new(env);
    }

    let effective_limit = if limit > MAX_PAGINATION_LIMIT {
        MAX_PAGINATION_LIMIT
    } else {
        limit
    };

    let end = core::cmp::min(start.saturating_add(effective_limit), total);
    let mut records = Vec::new(env);

    for index in start..end {
        let approval_key = keys::milestone_approval_key(contract_id, index);
        let approvals: Option<MilestoneApprovals> = env.storage().temporary().get(&approval_key);

        let has_approvals = approvals.is_some();
        let (client_approved, freelancer_approved, arbiter_approved) = match &approvals {
            Some(app) => (
                app.client_approved,
                app.freelancer_approved,
                app.arbiter_approved,
            ),
            None => (false, false, false),
        };

        records.push_back(AuthorizationRecord {
            milestone_index: index,
            has_approvals,
            client_approved,
            freelancer_approved,
            arbiter_approved,
        });
    }

    records
}

/// Builds the read-only diagnostic view for one milestone's release readiness.
///
/// Answers "can this milestone be released, and if not, what is blocking it?"
/// in a single call, which is what makes an `InsufficientApprovals` rejection
/// actionable instead of opaque.
///
/// # Empty-safe behavior
///
/// Rather than panicking, an unknown contract or an out-of-range index yields a
/// fully-defaulted view with `has_record == false` and
/// `release_authorized == false`. This keeps the view usable as a cheap probe
/// in a polling UI: an unknown id is reported as "nothing approved, nothing
/// outstanding", never as an error. The counter is still `0` outstanding for
/// unknown ids because no approval is expected for a milestone that does not
/// exist.
///
/// Derives everything from stored state and mutates nothing — in particular it
/// does **not** bump the approval TTL, so a poller cannot accidentally extend
/// an approval window it is only observing (I3).
pub fn release_readiness(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
) -> MilestoneReleaseReadiness {
    let contract: Option<Contract> = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id));
    let milestones = load_milestones(env, contract_id);

    // Resolve the milestone, defaulting to a non-existent milestone.
    let milestone: Option<Milestone> = milestones.as_ref().and_then(|m| {
        if milestone_index < m.len() {
            Some(m.get(milestone_index).unwrap())
        } else {
            None
        }
    });

    let approvals: Option<MilestoneApprovals> = env
        .storage()
        .temporary()
        .get(&keys::milestone_approval_key(contract_id, milestone_index));

    let mode = contract
        .as_ref()
        .map(|c| c.release_authorization)
        .unwrap_or(ReleaseAuthorization::ClientOnly);

    let (present, required) = approval_counts(mode, approvals.as_ref());
    let released = milestone.as_ref().map(|m| m.released).unwrap_or(false);
    let refunded = milestone.as_ref().map(|m| m.refunded).unwrap_or(false);

    MilestoneReleaseReadiness {
        milestone_index,
        // A settled milestone is never releasable again regardless of any
        // lingering approval record.
        release_authorized: present >= required && required > 0 && !released && !refunded,
        has_record: approvals.is_some(),
        approvals_required: required,
        approvals_present: present,
        approvals_missing: required.saturating_sub(present),
        released,
        refunded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Escrow;
    use soroban_sdk::{
        testutils::{storage::Temporary as _, Address as _, Ledger as _},
        Env, Vec,
    };

    fn setup_contract_in_storage(
        env: &Env,
        escrow_id: &crate::Address,
        contract_id: u32,
        contract: &Contract,
        release_auth: ReleaseAuthorization,
    ) {
        env.as_contract(escrow_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), contract);
            let milestones = Vec::from_array(
                env,
                [Milestone {
                    amount: 1000,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let _ = release_auth;
            let milestone_key = keys::milestone_key(env, contract_id);
            env.storage().persistent().set(&milestone_key, &milestones);
        });
    }

    #[test]
    fn test_approve_milestone_client_only() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_address = env.register(crate::Escrow, ());

        let escrow_id = env.register(Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };

        let contract_id = 1u32;
        env.as_contract(&contract_address, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), &contract);

            let milestones = Vec::from_array(
                &env,
                [Milestone {
                    amount: 1000,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let milestone_key = keys::milestone_key(&env, contract_id);
            env.storage().persistent().set(&milestone_key, &milestones);

            // Client approves
            let result = approve_milestone(&env, contract_id, 0, &client);
            assert!(result.is_ok());

            // Check approvals
            let check = check_approvals(&env, &contract, contract_id, 0);
            assert!(check.is_ok());
        });
    }

    #[test]
    fn test_approve_milestone_multisig() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_address = env.register(crate::Escrow, ());

        let escrow_id = env.register(Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::MultiSig,
            reputation_issued: false,
        };

        let contract_id = 1u32;
        env.as_contract(&contract_address, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), &contract);

            let milestones = Vec::from_array(
                &env,
                [Milestone {
                    amount: 1000,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let milestone_key = keys::milestone_key(&env, contract_id);
            env.storage().persistent().set(&milestone_key, &milestones);

            // Only client approves - insufficient
            let result = approve_milestone(&env, contract_id, 0, &client);
            assert!(result.is_ok());

            let check = check_approvals(&env, &contract, contract_id, 0);
            assert_eq!(check, Err(Error::InsufficientApprovals));

            // Freelancer also approves - now sufficient
            let result = approve_milestone(&env, contract_id, 0, &freelancer);
            assert!(result.is_ok());

            let check = check_approvals(&env, &contract, contract_id, 0);
            assert!(check.is_ok());
        });
    }

    #[test]
    fn test_duplicate_approval_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_address = env.register(crate::Escrow, ());

        let escrow_id = env.register(Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };

        let contract_id = 1u32;
        env.as_contract(&contract_address, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), &contract);

            let milestones = Vec::from_array(
                &env,
                [Milestone {
                    amount: 1000,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let milestone_key = keys::milestone_key(&env, contract_id);
            env.storage().persistent().set(&milestone_key, &milestones);

            // First approval succeeds
            let result = approve_milestone(&env, contract_id, 0, &client);
            assert!(result.is_ok());

            // Second approval fails
            let result = approve_milestone(&env, contract_id, 0, &client);
            assert_eq!(result, Err(Error::AlreadyApproved));
        });
    }

    // ── Role resolution ────────────────────────────────────────────────────

    fn role_fixture() -> (
        Env,
        Contract,
        crate::Address,
        crate::Address,
        crate::Address,
    ) {
        let env = Env::default();
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let arbiter = crate::Address::generate(&env);
        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: Some(arbiter.clone()),
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };
        (env, contract, client, freelancer, arbiter)
    }

    #[test]
    fn test_resolve_role_maps_each_participant() {
        let (_env, contract, client, freelancer, arbiter) = role_fixture();
        assert_eq!(resolve_role(&contract, &client), Ok(ApprovalRole::Client));
        assert_eq!(
            resolve_role(&contract, &freelancer),
            Ok(ApprovalRole::Freelancer)
        );
        assert_eq!(resolve_role(&contract, &arbiter), Ok(ApprovalRole::Arbiter));
    }

    #[test]
    fn test_resolve_role_rejects_non_participant() {
        let (env, contract, _client, _freelancer, _arbiter) = role_fixture();
        let stranger = crate::Address::generate(&env);
        assert_eq!(
            resolve_role(&contract, &stranger),
            Err(Error::UnauthorizedRole)
        );
    }

    /// Role-overlap precedence must be identical for the approve and revoke
    /// paths, otherwise a caller could set one flag and clear another.
    #[test]
    fn test_resolve_role_precedence_is_client_then_freelancer_then_arbiter() {
        let env = Env::default();
        let shared = crate::Address::generate(&env);
        let contract = Contract {
            client: shared.clone(),
            freelancer: shared.clone(),
            arbiter: Some(shared.clone()),
            status: ContractStatus::Funded,
            total_deposited: 0,
            funded_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };
        assert_eq!(resolve_role(&contract, &shared), Ok(ApprovalRole::Client));
    }

    #[test]
    fn test_mode_permits_matrix() {
        use ApprovalRole::*;
        use ReleaseAuthorization::*;

        assert!(mode_permits(ClientOnly, Client));
        assert!(!mode_permits(ClientOnly, Freelancer));
        assert!(!mode_permits(ClientOnly, Arbiter));

        assert!(mode_permits(ArbiterOnly, Arbiter));
        assert!(!mode_permits(ArbiterOnly, Client));

        assert!(mode_permits(ClientAndArbiter, Client));
        assert!(mode_permits(ClientAndArbiter, Arbiter));
        assert!(!mode_permits(ClientAndArbiter, Freelancer));

        assert!(mode_permits(MultiSig, Client));
        assert!(mode_permits(MultiSig, Freelancer));
        assert!(!mode_permits(MultiSig, Arbiter));
    }

    // ── Approval counting ──────────────────────────────────────────────────

    fn approvals_from(client: bool, freelancer: bool, arbiter: bool) -> MilestoneApprovals {
        MilestoneApprovals {
            client_approved: client,
            freelancer_approved: freelancer,
            arbiter_approved: arbiter,
        }
    }

    use ReleaseAuthorization as RA;

    #[test]
    fn test_approval_counts_client_only() {
        let (present, required) =
            approval_counts(RA::ClientOnly, Some(&approvals_from(true, false, false)));
        assert_eq!((present, required), (1, 1));
    }

    #[test]
    fn test_approval_counts_arbiter_only() {
        let (present, required) =
            approval_counts(RA::ArbiterOnly, Some(&approvals_from(false, false, true)));
        assert_eq!((present, required), (1, 1));

        // A client approval does not satisfy ArbiterOnly.
        let (present, required) =
            approval_counts(RA::ArbiterOnly, Some(&approvals_from(true, false, false)));
        assert_eq!((present, required), (0, 1));
    }

    /// `ClientAndArbiter` is an OR: either flag alone satisfies the requirement.
    #[test]
    fn test_approval_counts_client_and_arbiter_is_or() {
        let (present, required) = approval_counts(
            RA::ClientAndArbiter,
            Some(&approvals_from(false, false, true)),
        );
        assert_eq!((present, required), (1, 1));

        let (present, required) = approval_counts(
            RA::ClientAndArbiter,
            Some(&approvals_from(true, false, false)),
        );
        assert_eq!((present, required), (1, 1));

        let (present, required) = approval_counts(
            RA::ClientAndArbiter,
            Some(&approvals_from(false, true, false)),
        );
        assert_eq!((present, required), (0, 1));
    }

    /// `MultiSig` is an AND: the required count is two and never caps above two.
    #[test]
    fn test_approval_counts_multisig_requires_two() {
        let (present, required) =
            approval_counts(RA::MultiSig, Some(&approvals_from(true, true, false)));
        assert_eq!((present, required), (2, 2));

        let (present, required) =
            approval_counts(RA::MultiSig, Some(&approvals_from(true, false, false)));
        assert_eq!((present, required), (1, 2));

        // The arbiter flag never counts toward a MultiSig requirement.
        let (present, required) =
            approval_counts(RA::MultiSig, Some(&approvals_from(false, false, true)));
        assert_eq!((present, required), (0, 2));
    }

    /// A missing record counts as zero present approvals, never as sufficient.
    #[test]
    fn test_approval_counts_absent_record_is_zero_present() {
        for mode in [
            RA::ClientOnly,
            RA::ArbiterOnly,
            RA::ClientAndArbiter,
            RA::MultiSig,
        ] {
            let (present, required) = approval_counts(mode, None);
            assert_eq!(present, 0, "absent record must never count as present");
            assert!(required >= 1);
        }
    }

    #[test]
    fn test_approval_counts_all_false_record_is_zero_present() {
        let empty = approvals_from(false, false, false);
        for mode in [
            RA::ClientOnly,
            RA::ArbiterOnly,
            RA::ClientAndArbiter,
            RA::MultiSig,
        ] {
            assert_eq!(approval_counts(mode, Some(&empty)).0, 0);
        }
    }

    // ── Flag helpers ───────────────────────────────────────────────────────

    #[test]
    fn test_role_is_set_reads_only_own_flag() {
        let approvals = approvals_from(true, false, true);
        assert!(ApprovalRole::Client.is_set(&approvals));
        assert!(!ApprovalRole::Freelancer.is_set(&approvals));
        assert!(ApprovalRole::Arbiter.is_set(&approvals));
    }

    #[test]
    fn test_role_clear_touches_only_own_flag() {
        let mut approvals = approvals_from(true, true, true);
        assert!(ApprovalRole::Client.clear(&mut approvals));
        assert_eq!(
            approvals,
            approvals_from(false, true, true),
            "clearing the client flag must not disturb the other two"
        );
        assert!(!ApprovalRole::record_is_empty(&approvals));
    }

    #[test]
    fn test_role_clear_reports_false_when_already_clear() {
        let mut approvals = approvals_from(false, true, false);
        assert!(!ApprovalRole::Client.clear(&mut approvals));
        assert_eq!(approvals, approvals_from(false, true, false));
    }

    #[test]
    fn test_record_is_empty_only_when_all_flags_false() {
        assert!(ApprovalRole::record_is_empty(&approvals_from(
            false, false, false
        )));
        assert!(!ApprovalRole::record_is_empty(&approvals_from(
            false, false, true
        )));
        assert!(!ApprovalRole::record_is_empty(&approvals_from(
            true, false, false
        )));
        assert!(!ApprovalRole::record_is_empty(&approvals_from(
            false, true, false
        )));
    }

    // ── Concurrency / replay hardening (#1395) ─────────────────────────────
    //
    // Soroban serialises execution inside one ledger, so the failure modes
    // pinned below are duplicated, retried and interleaved calls across
    // transactions (not in-flight parallel writes). Each test asserts the
    // *fail-closed + idempotent under replay* invariant (I7): a repeated or
    // reordered call converges on the same state as a single execution, or on
    // a strictly less-authorized one — never more.

    /// Builds a `Funded` contract with the given parties and release mode.
    fn seeded_contract(
        client: &crate::Address,
        freelancer: &crate::Address,
        arbiter: Option<crate::Address>,
        mode: ReleaseAuthorization,
    ) -> Contract {
        Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: mode,
            reputation_issued: false,
        }
    }

    /// A single unreleased, unrefunded milestone.
    fn milestone_vector(env: &Env) -> Vec<Milestone> {
        Vec::from_array(
            env,
            [Milestone {
                amount: 1000,
                funded_amount: 0,
                released: false,
                refunded: false,
                work_evidence: None,
                refunded_amount: 0,
                deadline: None,
            }],
        )
    }

    /// Seeds `contract` and its single milestone into `escrow_id` storage.
    fn seed_state(env: &Env, escrow_id: &crate::Address, contract_id: u32, contract: &Contract) {
        env.as_contract(escrow_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), contract);
            env.storage().persistent().set(
                &keys::milestone_key(env, contract_id),
                &milestone_vector(env),
            );
        });
    }

    /// Reads the raw temporary approval record (bypassing the readiness view).
    fn stored(env: &Env, contract_id: u32) -> Option<MilestoneApprovals> {
        env.storage()
            .temporary()
            .get(&keys::milestone_approval_key(contract_id, 0))
    }

    /// `true` when a live temporary approval record exists.
    fn has_live_record(env: &Env, contract_id: u32) -> bool {
        env.storage()
            .temporary()
            .has(&keys::milestone_approval_key(contract_id, 0))
    }

    /// A replayed duplicate approval is rejected and cannot widen the flag set.
    #[test]
    fn replay_duplicate_approval_is_rejected_and_leaves_flag_count_unchanged() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract = seeded_contract(&client, &freelancer, None, ReleaseAuthorization::MultiSig);
        let cid = 1u32;

        seed_state(&env, &escrow_id, cid, &contract);

        env.as_contract(&escrow_id, || {
            assert_eq!(approve_milestone(&env, cid, 0, &client), Ok(true));

            // A replay from the same party is a typed rejection, not a silent
            // success, and must not touch the other party's flag.
            assert_eq!(
                approve_milestone(&env, cid, 0, &client),
                Err(Error::AlreadyApproved)
            );

            let record = stored(&env, cid).expect("record still present");
            assert!(record.client_approved);
            assert!(
                !record.freelancer_approved,
                "a duplicate must not set another party's flag"
            );

            // Required-flag counts are derived from stored flags, so a replay
            // cannot move the milestone toward release.
            let readiness = release_readiness(&env, cid, 0);
            assert_eq!(readiness.approvals_required, 2);
            assert_eq!(readiness.approvals_present, 1);
            assert_eq!(readiness.approvals_missing, 1);
            assert!(!readiness.release_authorized);
            assert_eq!(
                check_approvals(&env, &contract, cid, 0),
                Err(Error::InsufficientApprovals)
            );
        });
    }

    /// Approve → revoke → approve recomputes readiness deterministically, and a
    /// revoke never resurrects or clears another party's flag.
    #[test]
    fn replay_approve_revoke_approve_recomputes_readiness_deterministically() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract = seeded_contract(&client, &freelancer, None, ReleaseAuthorization::MultiSig);
        let cid = 1u32;

        seed_state(&env, &escrow_id, cid, &contract);

        env.as_contract(&escrow_id, || {
            assert_eq!(approve_milestone(&env, cid, 0, &client), Ok(true));
            assert_eq!(approve_milestone(&env, cid, 0, &freelancer), Ok(true));
            let both_approved = release_readiness(&env, cid, 0);
            assert!(both_approved.release_authorized);

            // Client withdraws: readiness drops to one present, one missing.
            let revoke = revoke_approval(&env, cid, 0, &client).expect("live record");
            assert_eq!(revoke.role, ApprovalRole::Client);
            assert_eq!(revoke.outcome, RevocationOutcome::FlagCleared);
            assert!(revoke.other_approvals_remain);

            let after_revoke = release_readiness(&env, cid, 0);
            assert!(!after_revoke.release_authorized);
            assert!(after_revoke.has_record);
            assert_eq!(after_revoke.approvals_present, 1);
            assert_eq!(after_revoke.approvals_missing, 1);

            // Only the client's flag was cleared; the freelancer's live consent
            // was neither cleared nor resurrected.
            let record = stored(&env, cid).expect("record kept while one flag lives");
            assert!(!record.client_approved);
            assert!(record.freelancer_approved);

            // Re-approving restores the deterministic "both approved" view.
            assert_eq!(approve_milestone(&env, cid, 0, &client), Ok(true));
            assert_eq!(release_readiness(&env, cid, 0), both_approved);
        });
    }

    /// Interleaving `MultiSig` approvals in either order yields the same
    /// `MilestoneReleaseReadiness`, at every step.
    #[test]
    fn replay_multisig_approval_order_is_independent() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract = seeded_contract(&client, &freelancer, None, ReleaseAuthorization::MultiSig);

        seed_state(&env, &escrow_id, 1, &contract);
        seed_state(&env, &escrow_id, 2, &contract);

        env.as_contract(&escrow_id, || {
            // Contract 1: client, then freelancer.
            assert_eq!(approve_milestone(&env, 1, 0, &client), Ok(true));
            let one_approval_a = release_readiness(&env, 1, 0);
            assert_eq!(approve_milestone(&env, 1, 0, &freelancer), Ok(true));
            let both_a = release_readiness(&env, 1, 0);

            // Contract 2: freelancer, then client — the interleaving reversed.
            assert_eq!(approve_milestone(&env, 2, 0, &freelancer), Ok(true));
            let one_approval_b = release_readiness(&env, 2, 0);
            assert_eq!(approve_milestone(&env, 2, 0, &client), Ok(true));
            let both_b = release_readiness(&env, 2, 0);

            assert!(both_a.release_authorized);
            assert!(both_b.release_authorized);
            assert_eq!(
                one_approval_a, one_approval_b,
                "a single MultiSig approval must look identical regardless of who made it"
            );
            assert_eq!(
                both_a, both_b,
                "the completed MultiSig set must be order-independent"
            );
        });
    }

    /// A revoke that finds no live record is a typed `InsufficientApprovals`
    /// no-op: it writes nothing and is safe to retry.
    #[test]
    fn replay_revoke_without_live_record_is_an_inert_typed_no_op() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract =
            seeded_contract(&client, &freelancer, None, ReleaseAuthorization::ClientOnly);
        let cid = 1u32;

        seed_state(&env, &escrow_id, cid, &contract);

        env.as_contract(&escrow_id, || {
            // Never approved: nothing to revoke, nothing written.
            assert_eq!(
                revoke_approval(&env, cid, 0, &client),
                Err(Error::InsufficientApprovals)
            );
            assert!(!has_live_record(&env, cid));

            // Approve then fully revoke; a repeated revoke is the same no-op.
            assert_eq!(approve_milestone(&env, cid, 0, &client), Ok(true));
            let first = revoke_approval(&env, cid, 0, &client).expect("live record");
            assert_eq!(first.outcome, RevocationOutcome::RecordRemoved);
            assert!(!first.other_approvals_remain);
            assert!(!has_live_record(&env, cid));

            for _ in 0..3 {
                assert_eq!(
                    revoke_approval(&env, cid, 0, &client),
                    Err(Error::InsufficientApprovals)
                );
            }
            assert!(
                !has_live_record(&env, cid),
                "a retried revoke must not synthesize an empty record"
            );
        });
    }

    /// Repeated read-only probes are pure: they neither rewrite the record nor
    /// extend (or shorten) its TTL.
    #[test]
    fn replay_repeated_reads_do_not_mutate_storage() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract = seeded_contract(&client, &freelancer, None, ReleaseAuthorization::MultiSig);
        let cid = 1u32;

        seed_state(&env, &escrow_id, cid, &contract);

        env.as_contract(&escrow_id, || {
            assert_eq!(approve_milestone(&env, cid, 0, &client), Ok(true));

            let key = keys::milestone_approval_key(cid, 0);
            let value_before = stored(&env, cid);
            let ttl_before = env.storage().temporary().get_ttl(&key);

            for _ in 0..8 {
                assert!(!release_readiness(&env, cid, 0).release_authorized);
                assert_eq!(
                    check_approvals(&env, &contract, cid, 0),
                    Err(Error::InsufficientApprovals)
                );
            }

            assert_eq!(
                stored(&env, cid),
                value_before,
                "a read must not rewrite the approval record"
            );
            assert_eq!(
                env.storage().temporary().get_ttl(&key),
                ttl_before,
                "a read must not extend or shorten the approval TTL"
            );
        });
    }

    /// After the temporary record is evicted, release is denied and readiness
    /// reports "no record"; recovery is a fresh approval, never stale reuse.
    #[test]
    fn replay_ttl_eviction_forces_reapproval_and_never_reuses_stale_state() {
        let env = Env::default();
        env.mock_all_auths();
        let instance_bound = PENDING_APPROVAL_TTL_LEDGERS * 2;
        env.ledger().with_mut(|li| {
            li.max_entry_ttl = instance_bound;
            li.min_persistent_entry_ttl = instance_bound;
            li.sequence_number = 1_000;
        });
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract =
            seeded_contract(&client, &freelancer, None, ReleaseAuthorization::ClientOnly);
        let cid = 1u32;

        seed_state(&env, &escrow_id, cid, &contract);

        env.as_contract(&escrow_id, || {
            assert_eq!(approve_milestone(&env, cid, 0, &client), Ok(true));
            assert!(release_readiness(&env, cid, 0).release_authorized);
            assert!(has_live_record(&env, cid));
        });

        // Advance past the approval window, keeping the contract instance alive
        // so the approval is the thing that expires.
        let expiry = env.as_contract(&escrow_id, || {
            env.ledger().sequence() + PENDING_APPROVAL_TTL_LEDGERS
        });
        env.ledger().set_sequence_number(expiry + 1);
        env.as_contract(&escrow_id, || {
            env.storage()
                .instance()
                .extend_ttl(instance_bound, instance_bound);
        });

        env.as_contract(&escrow_id, || {
            // I1: eviction is indistinguishable from "never approved".
            assert!(
                !has_live_record(&env, cid),
                "the temporary approval must be evicted"
            );
            let readiness = release_readiness(&env, cid, 0);
            assert!(!readiness.has_record);
            assert!(!readiness.release_authorized);
            assert_eq!(readiness.approvals_present, 0);
            assert_eq!(readiness.approvals_missing, 1);

            // Release is denied — eviction cannot leave a stale permission.
            assert_eq!(
                check_approvals(&env, &contract, cid, 0),
                Err(Error::InsufficientApprovals)
            );

            // Recovery is a fresh approval; stale reuse is impossible because
            // the evicted record no longer exists to be read.
            assert_eq!(approve_milestone(&env, cid, 0, &client), Ok(true));
            assert!(release_readiness(&env, cid, 0).release_authorized);
        });
    }

    // ── State invariants (issue #1393) ─────────────────────────────────────
    //
    // Each test pins one of the module's documented invariants I1–I6 so a
    // future change cannot silently weaken them.

    /// I1 — fail closed: a revoke with no live record denies with a typed
    /// error and writes nothing.
    #[test]
    fn i1_revoke_without_record_fails_closed_and_writes_nothing() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract = seeded_contract(&client, &freelancer, None, ReleaseAuthorization::MultiSig);
        seed_state(&env, &escrow_id, 1, &contract);

        env.as_contract(&escrow_id, || {
            assert_eq!(
                revoke_approval(&env, 1, 0, &client),
                Err(Error::InsufficientApprovals)
            );
            assert!(!has_live_record(&env, 1));
        });
    }

    /// I2 — revocation is authority-reducing: it only clears the caller's own
    /// set flag and never sets a flag or widens the set.
    #[test]
    fn i2_revocation_only_clears_the_callers_own_flag() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract = seeded_contract(&client, &freelancer, None, ReleaseAuthorization::MultiSig);
        seed_state(&env, &escrow_id, 1, &contract);

        env.as_contract(&escrow_id, || {
            assert_eq!(approve_milestone(&env, 1, 0, &client), Ok(true));
            assert_eq!(approve_milestone(&env, 1, 0, &freelancer), Ok(true));

            let revoke = revoke_approval(&env, 1, 0, &client).expect("live record");
            assert_eq!(revoke.role, ApprovalRole::Client);
            assert_eq!(revoke.outcome, RevocationOutcome::FlagCleared);

            let record = stored(&env, 1).expect("freelancer flag keeps the record");
            assert!(!record.client_approved, "only the caller's flag is cleared");
            assert!(
                record.freelancer_approved,
                "another party's consent is untouched"
            );

            assert_eq!(
                revoke_approval(&env, 1, 0, &client),
                Err(Error::InsufficientApprovals)
            );
        });
    }

    /// I3 — revocation never extends the surviving record's TTL; only a fresh
    /// approval resets the approval window.
    #[test]
    fn i3_revocation_does_not_extend_the_surviving_ttl() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract = seeded_contract(&client, &freelancer, None, ReleaseAuthorization::MultiSig);
        seed_state(&env, &escrow_id, 1, &contract);

        env.as_contract(&escrow_id, || {
            assert_eq!(approve_milestone(&env, 1, 0, &client), Ok(true));
            assert_eq!(approve_milestone(&env, 1, 0, &freelancer), Ok(true));
            let key = keys::milestone_approval_key(1, 0);
            let ttl_at_approval = env.storage().temporary().get_ttl(&key);
            assert!(ttl_at_approval > 0);

            env.ledger()
                .set_sequence_number(env.ledger().sequence() + 100);
            let _ = revoke_approval(&env, 1, 0, &client).expect("live record");

            let ttl_after_revoke = env.storage().temporary().get_ttl(&key);
            assert!(
                ttl_after_revoke <= ttl_at_approval,
                "revocation must not extend the approval window"
            );
        });
    }

    /// I4 — when the last set flag is cleared the whole temporary record is
    /// removed, so the post-state matches "never approved".
    #[test]
    fn i4_last_revocation_removes_the_record() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract =
            seeded_contract(&client, &freelancer, None, ReleaseAuthorization::ClientOnly);
        seed_state(&env, &escrow_id, 1, &contract);

        env.as_contract(&escrow_id, || {
            assert_eq!(approve_milestone(&env, 1, 0, &client), Ok(true));
            assert!(has_live_record(&env, 1));

            let revoke = revoke_approval(&env, 1, 0, &client).expect("live record");
            assert_eq!(revoke.outcome, RevocationOutcome::RecordRemoved);
            assert!(!has_live_record(&env, 1), "empty record must be removed");
            assert!(stored(&env, 1).is_none());
        });
    }

    /// I5 — terminal milestones are immutable: neither approve nor revoke is
    /// possible once the milestone is released.
    #[test]
    fn i5_released_milestone_rejects_approve_and_revoke() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract =
            seeded_contract(&client, &freelancer, None, ReleaseAuthorization::ClientOnly);
        seed_state(&env, &escrow_id, 1, &contract);

        env.as_contract(&escrow_id, || {
            let key = keys::milestone_key(&env, 1);
            let released = Vec::from_array(
                &env,
                [Milestone {
                    amount: 1000,
                    funded_amount: 1000,
                    released: true,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            env.storage().persistent().set(&key, &released);

            assert_eq!(
                approve_milestone(&env, 1, 0, &client),
                Err(Error::MilestoneAlreadyReleased)
            );
            assert_eq!(
                revoke_approval(&env, 1, 0, &client).unwrap_err(),
                Error::MilestoneAlreadyReleased
            );
        });
    }

    /// I6 — a dispute voids approvals: `clear_all_approvals` removes every
    /// stored record for the contract.
    #[test]
    fn i6_clear_all_approvals_voids_records() {
        let env = Env::default();
        env.mock_all_auths();
        let escrow_id = env.register(crate::Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);
        let contract = seeded_contract(&client, &freelancer, None, ReleaseAuthorization::MultiSig);
        seed_state(&env, &escrow_id, 1, &contract);

        env.as_contract(&escrow_id, || {
            assert_eq!(approve_milestone(&env, 1, 0, &client), Ok(true));
            assert_eq!(approve_milestone(&env, 1, 0, &freelancer), Ok(true));
            assert!(release_readiness(&env, 1, 0).release_authorized);

            clear_all_approvals(&env, 1);
            assert!(!has_live_record(&env, 1));
            assert!(!release_readiness(&env, 1, 0).release_authorized);
            assert_eq!(
                check_approvals(&env, &contract, 1, 0),
                Err(Error::InsufficientApprovals)
            );
        });
    }
}
