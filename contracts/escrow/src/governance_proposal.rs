//! Two-step approval workflow for high-impact governance overrides (#1221).
//!
//! High-impact protocol-configuration changes (`set_protocol_fee_bps`,
//! `set_governed_params`, `set_fee_withdrawal_cap`, `set_fee_withdrawal_cooldown`,
//! `set_max_milestones`) must pass through a two-step request → approve/reject →
//! apply state machine before they take effect.  This prevents a single
//! unreviewed request from unilaterally changing sensitive parameters.
//!
//! ## State machine
//!
//! ```text
//! [admin]  request_governance_proposal(kind)  →  Pending
//! [approver ≠ requester]  approve_governance_proposal(id)  →  Approved
//! [approver ≠ requester]  reject_governance_proposal(id)  →  Rejected  (terminal)
//! [admin]  apply_governance_proposal(id)  →  Applied  (terminal; side-effects executed)
//!
//! Any step fails with GovernanceProposalExpired if ledger.sequence() > expires_at_ledger.
//! ```
//!
//! ## Security properties
//!
//! * **Separate approver identity** — `approve_governance_proposal` rejects the
//!   requester's own address with `GovernanceSelfApproval`.
//! * **Short expiry window** — proposals expire after
//!   [`GOVERNANCE_PROPOSAL_TTL_LEDGERS`] (~3 days). Stale proposals cannot be
//!   applied after circumstances change.
//! * **Idempotency guard** — `apply_governance_proposal` can only be called once
//!   per proposal; subsequent calls fail with `GovernanceProposalInvalidState`.
//! * **Audit trail** — every state transition emits a structured Soroban event
//!   with proposal ID, kind, parties, and timestamp.
//! * **Rejection is terminal** — a rejected proposal cannot be re-approved or
//!   applied; the admin must open a fresh proposal.

use crate::storage_validation;
use crate::ttl::{set_governance_proposal_ttl, GOVERNANCE_PROPOSAL_TTL_LEDGERS};
use crate::{
    DataKey, Error, Escrow, EscrowArgs, EscrowClient, GovernanceProposal, GovernanceProposalKind,
    GovernanceProposalState, GovernedParameters, MAX_FEE_BPS,
};
use soroban_sdk::{contractimpl, symbol_short, Address, Env, Symbol};

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Allocate a new monotonically-increasing proposal ID.
fn next_proposal_id(env: &Env) -> u64 {
    let current: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::NextGovernanceProposalId)
        .unwrap_or(0u64);
    let next = current.saturating_add(1);
    env.storage()
        .persistent()
        .set(&DataKey::NextGovernanceProposalId, &next);
    next
}

/// Load a governance proposal from persistent storage, returning an error if not found.
///
/// Does **not** check expiry — callers must do that themselves so they can
/// distinguish "not found" from "expired-but-still-in-storage".
fn load_proposal(env: &Env, proposal_id: u64) -> GovernanceProposal {
    env.storage()
        .persistent()
        .get(&DataKey::GovernanceProposal(proposal_id))
        .unwrap_or_else(|| env.panic_with_error(Error::GovernanceProposalNotFound))
}

/// Persist a governance proposal and renew its TTL.
fn save_proposal(env: &Env, proposal: &GovernanceProposal) {
    env.storage()
        .persistent()
        .set(&DataKey::GovernanceProposal(proposal.proposal_id), proposal);
    set_governance_proposal_ttl(env, proposal.proposal_id);
}

/// Assert the proposal has not yet passed its expiry ledger.
fn require_not_expired(env: &Env, proposal: &GovernanceProposal) {
    if env.ledger().sequence() > proposal.expires_at_ledger {
        env.panic_with_error(Error::GovernanceProposalExpired);
    }
}

/// Validate that the payload carried in `kind` satisfies the same bounds
/// enforced by the corresponding live setter.
fn validate_kind(env: &Env, kind: &GovernanceProposalKind) {
    match kind {
        GovernanceProposalKind::SetProtocolFeeBps(bps) => {
            if *bps > MAX_FEE_BPS {
                env.panic_with_error(Error::InvalidProtocolParameters);
            }
        }
        GovernanceProposalKind::SetGovernedParams(params) => {
            if params.protocol_fee_bps > MAX_FEE_BPS {
                env.panic_with_error(Error::InvalidProtocolParameters);
            }
            storage_validation::validate_escrow_total_cap(env, params.max_escrow_total_stroops);
            if params.max_escrow_total_stroops <= 0 {
                env.panic_with_error(Error::InvalidProtocolParameters);
            }
        }
        GovernanceProposalKind::SetFeeWithdrawalCap(cap_bps) => {
            if *cap_bps > 10_000 {
                env.panic_with_error(Error::InvalidProtocolParameters);
            }
        }
        GovernanceProposalKind::SetFeeWithdrawalCooldown(cooldown) => {
            if *cooldown > 2_592_000 {
                env.panic_with_error(Error::InvalidProtocolParameters);
            }
        }
        GovernanceProposalKind::SetMaxMilestones(max) => {
            if *max < crate::MIN_MAX_MILESTONES || *max > crate::MAX_MAX_MILESTONES {
                env.panic_with_error(Error::LimitOutOfRange);
            }
        }
    }
}

/// Apply the side-effects of an approved proposal.  All mutations follow
/// the same patterns as the existing single-step setters in `governance.rs`.
fn apply_kind(env: &Env, kind: &GovernanceProposalKind) {
    match kind {
        GovernanceProposalKind::SetProtocolFeeBps(new_bps) => {
            let old_bps: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::ProtocolFeeBps)
                .unwrap_or(0u32);
            env.storage()
                .persistent()
                .set(&DataKey::ProtocolFeeBps, new_bps);
            env.events().publish(
                (Symbol::new(env, "protocol_fee_bps"),),
                (old_bps, *new_bps, env.ledger().timestamp()),
            );
        }
        GovernanceProposalKind::SetGovernedParams(new_params) => {
            let old_params: Option<GovernedParameters> =
                env.storage().persistent().get(&DataKey::GovernedParameters);
            env.storage()
                .persistent()
                .set(&DataKey::GovernedParameters, new_params);
            crate::ttl::extend_governed_parameters_ttl(env);
            // Update readiness checklist
            let mut checklist: crate::ReadinessChecklist = env
                .storage()
                .persistent()
                .get(&DataKey::ReadinessChecklist)
                .unwrap_or_default();
            checklist.governed_params_set = true;
            env.storage()
                .persistent()
                .set(&DataKey::ReadinessChecklist, &checklist);
            env.events().publish(
                (Symbol::new(env, "governed_parameters"),),
                (old_params, new_params.clone(), env.ledger().timestamp()),
            );
        }
        GovernanceProposalKind::SetFeeWithdrawalCap(new_cap) => {
            let old_cap: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::FeeWithdrawalCap)
                .unwrap_or(5_000u32);
            env.storage()
                .persistent()
                .set(&DataKey::FeeWithdrawalCap, new_cap);
            env.events().publish(
                (Symbol::new(env, "fee_cap"),),
                (old_cap, *new_cap, env.ledger().timestamp()),
            );
        }
        GovernanceProposalKind::SetFeeWithdrawalCooldown(new_cooldown) => {
            let old_cooldown: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::FeeWithdrawalCooldownLedgers)
                .unwrap_or(17_280u32);
            env.storage()
                .persistent()
                .set(&DataKey::FeeWithdrawalCooldownLedgers, new_cooldown);
            env.events().publish(
                (Symbol::new(env, "fee_cooldown"),),
                (old_cooldown, *new_cooldown, env.ledger().timestamp()),
            );
        }
        GovernanceProposalKind::SetMaxMilestones(new_max) => {
            env.storage()
                .persistent()
                .set(&DataKey::MaxMilestones, new_max);
            env.events().publish(
                (Symbol::new(env, "max_milestones"),),
                (*new_max, env.ledger().timestamp()),
            );
        }
    }
}

// ── Public contract entrypoints ───────────────────────────────────────────────

#[contractimpl]
impl Escrow {
    // ── Request ────────────────────────────────────────────────────────────────

    /// Submit a two-step governance override proposal.
    ///
    /// The stored admin must authorise the call. The payload in `kind` is
    /// validated against the same bounds as the corresponding live setter so
    /// invalid values are rejected immediately rather than at apply time.
    ///
    /// Returns the newly allocated proposal ID.
    ///
    /// # Errors
    /// * [`Error::NotInitialized`] — contract not initialised.
    /// * [`Error::UnauthorizedRole`] — caller is not the stored admin.
    /// * [`Error::InvalidProtocolParameters`] / [`Error::LimitOutOfRange`] —
    ///   the proposed value is out of range.
    ///
    /// # Events
    /// `(symbol_short!("gov"), Symbol("requested"))` →
    /// `(proposal_id, requester, kind, expires_at_ledger, timestamp)`
    pub fn request_governance_proposal(env: Env, kind: GovernanceProposalKind) -> u64 {
        Self::require_initialized(&env);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();

        // Validate the proposed value before creating the record.
        validate_kind(&env, &kind);

        let proposal_id = next_proposal_id(&env);
        let proposed_at = env.ledger().sequence();
        let expires_at = proposed_at.saturating_add(GOVERNANCE_PROPOSAL_TTL_LEDGERS);

        let proposal = GovernanceProposal {
            proposal_id,
            requester: admin.clone(),
            state: GovernanceProposalState::Pending,
            kind: kind.clone(),
            proposed_at_ledger: proposed_at,
            expires_at_ledger: expires_at,
            approver: None,
        };

        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), Symbol::new(&env, "requested")),
            (
                proposal_id,
                admin,
                kind,
                expires_at,
                env.ledger().timestamp(),
            ),
        );

        proposal_id
    }

    // ── Approve ────────────────────────────────────────────────────────────────

    /// Approve a pending governance proposal.
    ///
    /// The approver must authorise the call and **must not** be the same
    /// address as the requester (self-approval is prohibited).  Once approved
    /// the proposal transitions to `Approved` and the admin may call
    /// `apply_governance_proposal` to materialise the change.
    ///
    /// # Errors
    /// * [`Error::GovernanceProposalNotFound`] — no proposal with `proposal_id`.
    /// * [`Error::GovernanceProposalExpired`] — proposal TTL has elapsed.
    /// * [`Error::GovernanceProposalInvalidState`] — proposal is not `Pending`.
    /// * [`Error::GovernanceSelfApproval`] — approver == requester.
    ///
    /// # Events
    /// `(symbol_short!("gov"), Symbol("approved"))` →
    /// `(proposal_id, approver, timestamp)`
    pub fn approve_governance_proposal(env: Env, proposal_id: u64, approver: Address) -> bool {
        approver.require_auth();

        let mut proposal = load_proposal(&env, proposal_id);
        require_not_expired(&env, &proposal);

        if proposal.state != GovernanceProposalState::Pending {
            env.panic_with_error(Error::GovernanceProposalInvalidState);
        }

        // Prohibit self-approval: the approver must differ from the requester.
        if approver == proposal.requester {
            env.panic_with_error(Error::GovernanceSelfApproval);
        }

        proposal.state = GovernanceProposalState::Approved;
        proposal.approver = Some(approver.clone());
        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), Symbol::new(&env, "approved")),
            (proposal_id, approver, env.ledger().timestamp()),
        );

        true
    }

    // ── Reject ─────────────────────────────────────────────────────────────────

    /// Explicitly reject a pending governance proposal.
    ///
    /// Moves the proposal to the `Rejected` terminal state.  Subsequent calls to
    /// `approve_governance_proposal` or `apply_governance_proposal` for this
    /// proposal ID will fail with `GovernanceProposalInvalidState`.
    ///
    /// The approver must authorise and must not be the requester.
    ///
    /// # Errors
    /// * [`Error::GovernanceProposalNotFound`] — no proposal with `proposal_id`.
    /// * [`Error::GovernanceProposalExpired`] — proposal TTL has elapsed.
    /// * [`Error::GovernanceProposalInvalidState`] — proposal is not `Pending`.
    /// * [`Error::GovernanceSelfApproval`] — approver == requester.
    ///
    /// # Events
    /// `(symbol_short!("gov"), Symbol("rejected"))` →
    /// `(proposal_id, approver, timestamp)`
    pub fn reject_governance_proposal(env: Env, proposal_id: u64, approver: Address) -> bool {
        approver.require_auth();

        let mut proposal = load_proposal(&env, proposal_id);
        require_not_expired(&env, &proposal);

        if proposal.state != GovernanceProposalState::Pending {
            env.panic_with_error(Error::GovernanceProposalInvalidState);
        }

        if approver == proposal.requester {
            env.panic_with_error(Error::GovernanceSelfApproval);
        }

        proposal.state = GovernanceProposalState::Rejected;
        proposal.approver = Some(approver.clone());
        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), Symbol::new(&env, "rejected")),
            (proposal_id, approver, env.ledger().timestamp()),
        );

        true
    }

    // ── Apply ──────────────────────────────────────────────────────────────────

    /// Apply an approved governance proposal, materialising the parameter change.
    ///
    /// Only the stored admin may call this, and only for proposals in the
    /// `Approved` state.  On success the proposal transitions to `Applied`
    /// (idempotency guard: a second call fails with
    /// `GovernanceProposalInvalidState`) and the parameter change is written to
    /// persistent storage exactly as the corresponding live setter would do.
    ///
    /// # Errors
    /// * [`Error::NotInitialized`] — contract not initialised.
    /// * [`Error::GovernanceProposalNotFound`] — no proposal with `proposal_id`.
    /// * [`Error::GovernanceProposalExpired`] — proposal TTL has elapsed.
    /// * [`Error::GovernanceProposalInvalidState`] — proposal is not `Approved`.
    ///
    /// # Events
    /// `(symbol_short!("gov"), Symbol("applied"))` →
    /// `(proposal_id, admin, kind, timestamp)`
    ///
    /// Plus the parameter-specific event emitted by `apply_kind` (e.g.
    /// `"protocol_fee_bps"`, `"governed_parameters"`, etc.).
    pub fn apply_governance_proposal(env: Env, proposal_id: u64) -> bool {
        Self::require_initialized(&env);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();

        let mut proposal = load_proposal(&env, proposal_id);
        require_not_expired(&env, &proposal);

        if proposal.state != GovernanceProposalState::Approved {
            env.panic_with_error(Error::GovernanceProposalInvalidState);
        }

        // Materialise the parameter change.
        apply_kind(&env, &proposal.kind);

        // Mark as applied so a second call fails.
        proposal.state = GovernanceProposalState::Applied;
        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), Symbol::new(&env, "applied")),
            (
                proposal_id,
                admin,
                proposal.kind.clone(),
                env.ledger().timestamp(),
            ),
        );

        true
    }

    // ── Read ───────────────────────────────────────────────────────────────────

    /// Return the governance proposal record for `proposal_id`, or `None` if it
    /// does not exist (was never created or has been evicted after expiry).
    pub fn get_governance_proposal(env: Env, proposal_id: u64) -> Option<GovernanceProposal> {
        env.storage()
            .persistent()
            .get(&DataKey::GovernanceProposal(proposal_id))
    }

    /// Return the next proposal ID that would be assigned by the next
    /// `request_governance_proposal` call.  Useful for off-chain indexers.
    pub fn get_next_governance_proposal_id(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::NextGovernanceProposalId)
            .unwrap_or(0u64)
            .saturating_add(1)
    }
}
