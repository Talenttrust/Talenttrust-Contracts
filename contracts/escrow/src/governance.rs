//! Governance and protocol-configuration entrypoints.
//!
//! This module owns admin-controlled persistent configuration:
//! `DataKey::Admin` for authorization, `ProtocolFeeBps` for release fees,
//! `GovernedParameters` for escrow caps, `ReadinessChecklist` for deployment
//! readiness state, and `PendingAdmin` for two-step admin rotation proposals.
//! Money movement for protocol-fee withdrawal remains in the crate root because
//! it performs settlement-token transfers.
//!
//! ## Two-step admin transfer
//!
//! `DataKey::Admin` is a single address, so a typo'd or compromised
//! `initialize`/prior transfer hands over the whole contract irrevocably if
//! rotation were a single call. Instead rotation is propose/accept/cancel:
//!
//! 1. `propose_admin(new)` — current admin stores `new` under `PendingAdmin`
//!    with the current ledger sequence. Self-proposals are rejected.
//!    If a previous proposal is still active (within TTL), proposing again
//!    overwrites it and emits a `replaced` event so the superseded proposal
//!    is observable.  If the previous proposal is already expired, it is
//!    silently replaced (it was already unacceptable).
//! 2. `accept_admin()` — the *proposed* address, not the current admin,
//!    authorizes this call. It must arrive no earlier than
//!    `ADMIN_ROTATION_MIN_DELAY_LEDGERS` after the proposal (the reaction
//!    window) and no later than `ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS` after it
//!    (so a stale, unaddressed proposal cannot be accepted long after the
//!    circumstances that produced it have changed).
//! 3. `cancel_admin()` — the current admin can abort a pending proposal at any
//!    time, expired or not.
//! 4. `recover_admin_proposal()` — the current admin can clean up an expired
//!    proposal after `ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS` ledgers have elapsed.
//!    This is the deterministic recovery path after an `accept_admin` call fails
//!    with `AdminProposalExpired`.
//!
//! ## Failure recovery model
//!
//! Soroban panics roll back all storage writes atomically, so no partial state
//! can be persisted.  The following table documents each failure path and its
//! deterministic recovery:
//!
//! | Failure                       | Cause                                      | Recovery                                    |
//! |-------------------------------|--------------------------------------------|---------------------------------------------|
//! | `TimelockNotElapsed`          | `accept_admin` before min-delay ledgers    | Wait; retry `accept_admin` later            |
//! | `AdminProposalExpired`        | `accept_admin` after TTL ledgers           | Admin calls `recover_admin_proposal` then re-proposes |
//! | `InvalidState` (accept)       | `accept_admin` with no pending proposal    | Admin calls `propose_admin` first           |
//! | `InvalidState` (cancel)       | `cancel_admin` with no pending proposal    | No-op; nothing to cancel                    |
//! | `CannotProposeSelf`           | `propose_admin` with current admin address | Use a different address                     |
//! | `NotInitialized`              | Any call before `initialize`               | Call `initialize` first                     |
//!
//! Every transition clears or overwrites `PendingAdmin` so an accept can never
//! be replayed against a cancelled or already-consumed proposal: it simply
//! finds nothing pending and fails with `Error::InvalidState`.
//!
//! Concurrent clients should use the `*_checked` entrypoints with the revision
//! returned by `get_admin_rotation_revision`. Every successful rotation mutation,
//! including a legacy call, advances that revision. A stale request therefore
//! cannot accept/cancel a replacement proposal, even if it has the same address
//! and was created in the same ledger (the ABA case). Soroban serializes conflicting
//! transactions and rolls back failed invocations; no process-local lock is needed.

use crate::storage_validation;
use crate::ttl;
use crate::ttl::{ADMIN_ROTATION_MIN_DELAY_LEDGERS, ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS};
use crate::{
    DataKey, Error, Escrow, EscrowArgs, EscrowClient, GovernedParameters, PendingAdminProposal,
    ReadinessChecklist, MAX_FEE_BPS, MAX_MAX_MILESTONES, MIN_MAX_MILESTONES,
    DEFAULT_PROTOCOL_FEE_BPS, MAX_FEE_WITHDRAWAL_CAP_BPS, DEFAULT_FEE_WITHDRAWAL_CAP_BPS,
    MAX_FEE_WITHDRAWAL_COOLDOWN_LEDGERS, DEFAULT_FEE_WITHDRAWAL_COOLDOWN_LEDGERS,
};
use soroban_sdk::{contractimpl, symbol_short, Address, Env, Symbol};

fn require_rotation_revision(env: &Env, expected_revision: u64) {
    Escrow::require_initialized(env);
    if Escrow::get_admin_rotation_revision(env.clone()) != expected_revision {
        env.panic_with_error(Error::StaleNonce);
    }
}

/// Advance only in the same atomic invocation as the rotation mutation. Instance
/// storage prevents independent expiry of the revision from resetting replay
/// protection while the contract instance remains live. Never wrap at u64::MAX.
fn advance_rotation_revision(env: &Env) {
    let revision = Escrow::get_admin_rotation_revision(env.clone())
        .checked_add(1)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
    env.storage()
        .instance()
        .set(&DataKey::AdminRotationRevision, &revision);
    env.events()
        .publish((Symbol::new(env, "admin_rotation_revision"),), revision);
}

#[contractimpl]
impl Escrow {
    /// Set the protocol fee in basis points.
    ///
    /// Admin-gated: the stored admin (under [`DataKey::Admin`]) must authorize
    /// the call and the contract must be initialized.
    ///
    /// **Two-step alternative**: the same change can be routed through a
    /// governance proposal of kind
    /// `GovernanceProposalKind::SetProtocolFeeBps(new_bps)` via
    /// `request_governance_proposal` → `approve_governance_proposal` →
    /// `apply_governance_proposal`. This direct setter remains the legacy
    /// single-step admin path — it does not itself require an approved
    /// proposal, and callers depend on that; prefer the proposal flow for
    /// high-impact changes.
    ///
    /// **Admin nonce (replay protection)**: `admin_nonce` must equal the
    /// stored `DataKey::AdminNonce` plus one (the first call after
    /// `initialize` uses `1`). A mismatch panics with [`Error::StaleNonce`];
    /// because a panicking call rolls back completely, a rejected attempt
    /// never burns the nonce.
    ///
    /// `new_bps` must be `≤ 10_000` (100%). The fee takes effect immediately for
    /// the next `release_milestone` call.
    ///
    /// See [`docs/escrow/protocol-fees.md`](../../../docs/escrow/protocol-fees.md) for
    /// the basis-point model, fee formula, accrual storage, and withdrawal flow.
    ///
    /// # Events
    /// `(Symbol("protocol_fee_bps"),)` → `(old_bps, new_bps, admin, timestamp)`
    pub fn set_protocol_fee_bps(env: Env, new_bps: u32, admin_nonce: u64) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();
        crate::storage::consume_admin_nonce(&env, admin_nonce);

        // Invariant: the protocol fee must never exceed 100% (10_000 bps).
        // Validate before any state mutation so a rejected call cannot leave
        // a partially-updated configuration behind.
        storage_validation::validate_protocol_fee_bps(&env, new_bps);
        // Redundant with `validate_protocol_fee_bps` but kept as an explicit
        // second guard: the stored value must never exceed 100 %.
        if new_bps > MAX_FEE_BPS {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        let old_bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(DEFAULT_PROTOCOL_FEE_BPS);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBps, &new_bps);

        env.events().publish(
            (Symbol::new(&env, "protocol_fee_bps"),),
            (old_bps, new_bps, admin.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Returns the current protocol fee in basis points.
    ///
    /// Total function: returns the stored value or the compatibility default
    /// [`DEFAULT_PROTOCOL_FEE_BPS`] when the key is unset (never fails).
    pub fn get_protocol_fee_bps(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get::<_, u32>(&DataKey::ProtocolFeeBps)
            .unwrap_or(DEFAULT_PROTOCOL_FEE_BPS)
    }

    /// Set the maximum allowed milestones per contract (admin-controlled).
    ///
    /// The stored admin must authorize the call. The provided
    /// `max_milestones` is validated against compile-time safe bounds and a
    /// typed `LimitOutOfRange` error is returned for invalid values.
    ///
    /// `max_milestones` must be within `[MIN_MAX_MILESTONES, MAX_MAX_MILESTONES]`.
    ///
    /// **Two-step alternative**: a governance proposal of kind
    /// `GovernanceProposalKind::SetMaxMilestones(max_milestones)` can be routed
    /// through `request_governance_proposal` → `approve_governance_proposal` →
    /// `apply_governance_proposal`. This direct setter is the legacy
    /// single-step admin path and does not itself require an approved
    /// proposal; that behavior is preserved for existing callers.
    pub fn set_max_milestones(env: Env, max_milestones: u32) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();

        // Invariant: the configured milestone cap must stay within the
        // compile-time safe bounds. Reject out-of-range values before writing
        // so the stored configuration is never left in an invalid state.
        if max_milestones < MIN_MAX_MILESTONES || max_milestones > MAX_MAX_MILESTONES {
            env.panic_with_error(Error::LimitOutOfRange);
        }

        env.storage()
            .persistent()
            .set(&DataKey::MaxMilestones, &max_milestones);
        true
    }

    /// Read-only accessor for the configured maximum milestones per contract.
    /// Returns the stored value or the compile-time default (`MAX_MILESTONES`).
    pub fn get_max_milestones(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get::<_, u32>(&DataKey::MaxMilestones)
            .unwrap_or(crate::MAX_MILESTONES)
    }

    // ── Two-step admin transfer ───────────────────────────────────────────────

    /// Current admin-rotation revision. Zero is the initial/pre-upgrade value.
    pub fn get_admin_rotation_revision(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::AdminRotationRevision)
            .unwrap_or(0)
    }

    /// Propose only if the observed rotation revision is still current.
    /// A stale or replayed request fails with `StaleNonce` without changing state.
    pub fn propose_admin_checked(env: Env, proposed: Address, expected_revision: u64) -> bool {
        require_rotation_revision(&env, expected_revision);
        Self::propose_admin_impl(&env, proposed)
    }

    /// Accept only the proposal observed at `expected_revision`, with the usual
    /// proposed-admin authorization, timelock, and expiry checks.
    pub fn accept_admin_checked(env: Env, expected_revision: u64) -> bool {
        require_rotation_revision(&env, expected_revision);
        Self::accept_admin_impl(&env)
    }

    /// Cancel only the proposal observed at `expected_revision` (current admin).
    pub fn cancel_admin_checked(env: Env, expected_revision: u64) -> bool {
        require_rotation_revision(&env, expected_revision);
        Self::cancel_admin_impl(&env)
    }

    /// Recover only the expired proposal observed at `expected_revision`.
    pub fn recover_admin_proposal_checked(env: Env, expected_revision: u64) -> bool {
        require_rotation_revision(&env, expected_revision);
        Self::recover_admin_proposal_impl(&env)
    }

    /// Legacy proposal entrypoint; use `propose_admin_checked` for stale-request protection.
    pub fn propose_admin(env: Env, proposed: Address) -> bool {
        Self::propose_admin_impl(&env, proposed)
    }

    /// Propose a new admin. Stores the proposal with a timelock.
    ///
    /// # Errors
    /// * [`Error::NotInitialized`] — `initialize` has not been called.
    /// * [`Error::CannotProposeSelf`] — `proposed` is the current admin.
    ///
    /// # Concurrent re-proposal behaviour
    ///
    /// If a previous proposal is still active (within `ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS`),
    /// the new proposal **overwrites** it and an additional
    /// `(symbol_short!("admin"), Symbol("replaced"))` → `(admin, superseded, timestamp)`
    /// event is emitted before the normal `proposed` event.  This makes it
    /// explicit to off-chain observers that the prior candidate was displaced,
    /// which is the critical signal for any system that monitors pending
    /// transfers.  An expired proposal is silently replaced — it was already
    /// unreachable — so no `replaced` event is emitted in that case.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("proposed"))` → `(admin, proposed, timestamp)`
    ///
    /// Additional event when overwriting a live proposal:
    /// `(symbol_short!("admin"), Symbol("replaced"))` → `(admin, superseded_proposed, timestamp)`
    pub(crate) fn propose_admin_impl(env: &Env, proposed: Address) -> bool {
        Self::require_initialized(env);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        admin.require_auth();

        // Invariant: a self-proposal is a no-op that would let the current
        // admin bypass the reaction window, so reject it before touching
        // PendingAdmin.
        if proposed == admin {
            env.panic_with_error(Error::CannotProposeSelf);
        }

        advance_rotation_revision(env);
        env.storage().persistent().set(
            &DataKey::PendingAdmin,
            &PendingAdminProposal {
                proposed: proposed.clone(),
                proposed_at_ledger: env.ledger().sequence(),
            },
        );

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(env, "proposed")),
            (admin, proposed.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Accept a pending admin proposal, enforcing the timelock and expiry window.
    ///
    /// Public entrypoint that delegates to [`accept_admin_impl`].
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("accepted"))` → `(old_admin, new_admin, timestamp)`
    pub fn accept_admin(env: Env) -> bool {
        Self::accept_admin_impl(&env)
    }

    /// Accept a pending admin proposal, enforcing the timelock and expiry window.
    ///
    /// # Errors
    /// * [`Error::NotInitialized`] — `initialize` has not been called.
    /// * [`Error::InvalidState`] — there is no pending proposal.
    /// * [`Error::TimelockNotElapsed`] — called before
    ///   `ADMIN_ROTATION_MIN_DELAY_LEDGERS` ledgers have elapsed since the
    ///   proposal. Retry after more ledgers have closed.
    /// * [`Error::AdminProposalExpired`] — called after
    ///   `ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS` ledgers have elapsed since the
    ///   proposal. The stale proposal **cannot be cleared inside this call**
    ///   because Soroban panics roll back all storage writes atomically; the
    ///   panicking accept cannot both fail and persist a removal.  The
    ///   deterministic recovery path is:
    ///   1. The current admin calls [`Escrow::recover_admin_proposal`] to
    ///      remove the expired record.
    ///   2. The admin then calls [`Escrow::propose_admin`] with the new
    ///      address to start a fresh rotation.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("accepted"))` → `(old_admin, new_admin, timestamp)`
    pub(crate) fn accept_admin_impl(env: &Env) -> bool {
        Self::require_initialized(env);

        let pending: PendingAdminProposal = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| env.panic_with_error(Error::InvalidState));

        // Invariant: acceptance is only valid inside the [min_delay, ttl]
        // window. Both bounds are checked before authorization and before any
        // state mutation, so a rejected accept cannot consume the proposal.
        let elapsed = env
            .ledger()
            .sequence()
            .saturating_sub(pending.proposed_at_ledger);
        if elapsed < ADMIN_ROTATION_MIN_DELAY_LEDGERS {
            env.panic_with_error(Error::TimelockNotElapsed);
        }
        if elapsed > ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS {
            // The proposal has expired.  Soroban panics roll back all storage
            // writes atomically, so we cannot clear `PendingAdmin` here while
            // simultaneously panicking — the removal would be rolled back.
            // The admin must call `recover_admin_proposal` followed by a fresh
            // `propose_admin` to regain a clean rotation state.
            env.panic_with_error(Error::AdminProposalExpired);
        }

        // Invariant: only the proposed address may accept, and it must
        // authorize this call. The proposal is consumed atomically below so a
        // replay finds nothing pending and fails with InvalidState.
        let pending_admin = pending.proposed;
        pending_admin.require_auth();

        let old_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        advance_rotation_revision(env);
        env.storage()
            .persistent()
            .set(&DataKey::Admin, &pending_admin);
        env.storage().persistent().remove(&DataKey::PendingAdmin);

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(env, "accepted")),
            (old_admin, pending_admin.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Cancel a pending admin proposal, aborting a two-step transfer.
    ///
    /// Public entrypoint that delegates to [`cancel_admin_impl`].
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("cancelled"))` → `(admin, cancelled_proposal, timestamp)`
    pub fn cancel_admin(env: Env) -> bool {
        Self::cancel_admin_impl(&env)
    }

    /// Cancel a pending admin proposal, aborting a two-step transfer.
    ///
    /// Only the current admin (the address stored under [`DataKey::Admin`]) may
    /// cancel, and the contract must be initialized. On success the pending
    /// proposal is removed so the previously proposed address can no longer call
    /// [`Escrow::accept_admin`] — a subsequent accept panics with
    /// [`Error::InvalidState`]. Works on an expired proposal too, since expiry
    /// only bounds *acceptance*, not cancellation.
    ///
    /// # Errors
    /// * [`Error::NotInitialized`] — `initialize` has not been called.
    /// * [`Error::InvalidState`] — there is no pending proposal to cancel.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("cancelled"))` → `(admin, cancelled_proposal, timestamp)`
    pub(crate) fn cancel_admin_impl(env: &Env) -> bool {
        Self::require_initialized(env);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        admin.require_auth();

        // Invariant: cancellation requires a live proposal and is authorized
        // by the current admin only. Removing it here guarantees a subsequent
        // accept_admin call fails with InvalidState (no replay).
        let pending: PendingAdminProposal = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| env.panic_with_error(Error::InvalidState));

        advance_rotation_revision(env);
        env.storage().persistent().remove(&DataKey::PendingAdmin);

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(env, "cancelled")),
            (admin, pending.proposed, env.ledger().timestamp()),
        );
        true
    }

    /// Recover an abandoned admin proposal after its expiry.
    ///
    /// Public entrypoint that delegates to [`recover_admin_proposal_impl`].
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("recovered"))` → `(admin, cancelled_proposal, timestamp)`
    pub fn recover_admin_proposal(env: Env) -> bool {
        Self::recover_admin_proposal_impl(&env)
    }

    /// Recover an abandoned admin proposal after its expiry.
    ///
    /// Only the current admin may recover, and the contract must be initialized.
    /// The proposal must be expired (older than `ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS`).
    ///
    /// # Errors
    /// * [`Error::NotInitialized`] — `initialize` has not been called.
    /// * [`Error::InvalidState`] — there is no pending proposal, or it is still active.
    /// * [`Error::TimelockNotElapsed`] — the proposal is too recent.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("recovered"))` → `(admin, cancelled_proposal, timestamp)`
    pub(crate) fn recover_admin_proposal_impl(env: &Env) -> bool {
        Self::require_initialized(env);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        admin.require_auth();

        let pending: PendingAdminProposal = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| env.panic_with_error(Error::InvalidState));

        let elapsed = env
            .ledger()
            .sequence()
            .saturating_sub(pending.proposed_at_ledger);

        if elapsed < ADMIN_ROTATION_MIN_DELAY_LEDGERS {
            env.panic_with_error(Error::TimelockNotElapsed);
        }
        if elapsed <= ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS {
            env.panic_with_error(Error::InvalidState);
        }

        advance_rotation_revision(env);
        env.storage().persistent().remove(&DataKey::PendingAdmin);

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(env, "recovered")),
            (admin, pending.proposed, env.ledger().timestamp()),
        );
        true
    }

    /// Returns the currently pending admin address, if any.
    ///
    /// Public entrypoint that delegates to [`get_pending_admin_impl`].
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        Self::get_pending_admin_impl(&env)
    }

    /// Internal: return the currently pending admin address, if any.
    pub(crate) fn get_pending_admin_impl(env: &Env) -> Option<Address> {
        let proposal: Option<PendingAdminProposal> =
            env.storage().persistent().get(&DataKey::PendingAdmin);
        proposal.map(|p| p.proposed)
    }

    /// Set both governance parameters at once and update the readiness checklist.
    ///
    /// Sets `protocol_fee_bps` (must be `≤ 10_000`) and `max_escrow_total_stroops`
    /// atomically. Also flips `ReadinessChecklist::governed_params_set` to `true`.
    ///
    /// **Two-step alternative**: the same change can be routed through a
    /// governance proposal of kind
    /// `GovernanceProposalKind::SetGovernedParams(params)` via
    /// `request_governance_proposal` → `approve_governance_proposal` →
    /// `apply_governance_proposal`. This direct setter is the legacy
    /// single-step admin path and does not itself require an approved
    /// proposal; that behavior is preserved for existing callers.
    ///
    /// See [`docs/escrow/protocol-fees.md`](../../../docs/escrow/protocol-fees.md) for
    /// the full basis-point model and fee lifecycle.
    ///
    /// # Events
    /// `(Symbol("governed_parameters"),)` → `(old_parameters, new_parameters, admin, timestamp)`
    pub fn set_governed_params(
        env: Env,
        admin: Address,
        protocol_fee_bps: u32,
        max_escrow_total_stroops: i128,
    ) -> bool {
        let new_parameters = GovernedParameters {
            protocol_fee_bps,
            max_escrow_total_stroops,
        };
        Self::set_governed_parameters(env, admin, new_parameters)
    }

    /// Admin-guarded setter for structured GovernedParameters.
    ///
    /// Validates bounds against compile-time constants (MAX_FEE_BPS, positive stroops),
    /// enforces admin authorization, records old and new parameters in an event,
    /// and marks the readiness checklist.
    ///
    /// **Two-step alternative**: as with [`Escrow::set_governed_params`], the
    /// reviewed path is a `GovernanceProposalKind::SetGovernedParams` proposal
    /// applied via `apply_governance_proposal`; this direct setter remains the
    /// legacy single-step admin path and keeps that behavior for compatibility
    /// (it does not verify an approved proposal).
    ///
    /// # Events
    /// `(Symbol("governed_parameters"),)` → `(old_parameters, new_parameters, admin, timestamp)`
    pub fn set_governed_parameters(
        env: Env,
        admin: Address,
        new_parameters: GovernedParameters,
    ) -> bool {
        Self::require_initialized(&env);

        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));

        if admin != stored_admin {
            env.panic_with_error(Error::UnauthorizedRole);
        }
        admin.require_auth();

        if new_parameters.protocol_fee_bps > MAX_FEE_BPS {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        storage_validation::validate_escrow_total_cap(
            &env,
            new_parameters.max_escrow_total_stroops,
        );
        if new_parameters.max_escrow_total_stroops <= 0 {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        let old_parameters: Option<GovernedParameters> =
            env.storage().persistent().get(&DataKey::GovernedParameters);

        env.storage()
            .persistent()
            .set(&DataKey::GovernedParameters, &new_parameters);

        ttl::extend_governed_parameters_ttl(&env);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default();
        checklist.governed_params_set = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (Symbol::new(&env, "governed_parameters"),),
            (
                old_parameters,
                new_parameters,
                admin,
                env.ledger().timestamp(),
            ),
        );

        true
    }

    /// Retrieve the current governed parameters with persistent TTL renewal.
    ///
    /// Total function on empty data: returns `None` (never fails) when no
    /// parameters have been stored, keeping pre-configuration reads safe for
    /// indexers and post-upgrade readers alike.
    pub fn get_governed_parameters(env: Env) -> Option<GovernedParameters> {
        let params: Option<GovernedParameters> =
            env.storage().persistent().get(&DataKey::GovernedParameters);
        if params.is_some() {
            ttl::extend_governed_parameters_ttl(&env);
        }
        params
    }

    // ── Fee withdrawal rate-limiting ────────────────────────────────────────

    /// Set the maximum fraction of accumulated protocol fees that can be
    /// withdrawn in a single call, expressed in basis points.
    ///
    /// Admin-gated, must be initialized.  A value of `0` disables the cap
    /// (unlimited withdrawals, subject to the cooldown).  Values above
    /// [`MAX_FEE_WITHDRAWAL_CAP_BPS`] (100 %) are rejected with
    /// [`Error::InvalidProtocolParameters`].
    ///
    /// Stored under [`DataKey::FeeWithdrawalCap`].  Default is
    /// [`DEFAULT_FEE_WITHDRAWAL_CAP_BPS`] (50 %).
    ///
    /// # Events
    /// `(Symbol("fee_cap"),)` → `(old_cap, new_cap, admin, timestamp)`
    pub fn set_fee_withdrawal_cap(env: Env, cap_bps: u32) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();

        if cap_bps > MAX_FEE_WITHDRAWAL_CAP_BPS {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        let old_cap: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::FeeWithdrawalCap)
            .unwrap_or(DEFAULT_FEE_WITHDRAWAL_CAP_BPS);

        env.storage()
            .persistent()
            .set(&DataKey::FeeWithdrawalCap, &cap_bps);

        env.events().publish(
            (Symbol::new(&env, "fee_cap"),),
            (old_cap, cap_bps, admin.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Return the current fee-withdrawal cap in basis points.
    ///
    /// Returns the stored value, or the compatibility default
    /// [`DEFAULT_FEE_WITHDRAWAL_CAP_BPS`] (50 %) when no value has been
    /// explicitly set.
    pub fn get_fee_withdrawal_cap(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::FeeWithdrawalCap)
            .unwrap_or(DEFAULT_FEE_WITHDRAWAL_CAP_BPS)
    }

    /// Set the minimum number of ledgers that must elapse between successful
    /// protocol-fee withdrawals.
    ///
    /// Admin-gated, must be initialized.  A value of `0` disables the cooldown
    /// (unlimited frequency, subject to the cap).  Values above
    /// [`MAX_FEE_WITHDRAWAL_COOLDOWN_LEDGERS`] (≈150 days at 5 s ledgers) are
    /// rejected with [`Error::InvalidProtocolParameters`].
    ///
    /// Stored under [`DataKey::FeeWithdrawalCooldownLedgers`].
    /// Default is [`DEFAULT_FEE_WITHDRAWAL_COOLDOWN_LEDGERS`] (≈1 day at
    /// 5 s ledgers).
    ///
    /// # Events
    /// `(Symbol("fee_cooldown"),)` → `(old_cooldown, new_cooldown, admin, timestamp)`
    pub fn set_fee_withdrawal_cooldown(env: Env, cooldown_ledgers: u32) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();

        // Cap at ~150 days to prevent accidental permanent lockout.
        if cooldown_ledgers > MAX_FEE_WITHDRAWAL_COOLDOWN_LEDGERS {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        let old_cooldown: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::FeeWithdrawalCooldownLedgers)
            .unwrap_or(DEFAULT_FEE_WITHDRAWAL_COOLDOWN_LEDGERS);

        env.storage()
            .persistent()
            .set(&DataKey::FeeWithdrawalCooldownLedgers, &cooldown_ledgers);

        env.events().publish(
            (Symbol::new(&env, "fee_cooldown"),),
            (
                old_cooldown,
                cooldown_ledgers,
                admin.clone(),
                env.ledger().timestamp(),
            ),
        );
        true
    }

    /// Return the current fee-withdrawal cooldown in ledgers.
    ///
    /// Returns the stored value, or the compatibility default
    /// [`DEFAULT_FEE_WITHDRAWAL_COOLDOWN_LEDGERS`] (≈1 day at 5 s ledgers)
    /// when no value has been explicitly set.
    pub fn get_fee_withdrawal_cooldown(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::FeeWithdrawalCooldownLedgers)
            .unwrap_or(DEFAULT_FEE_WITHDRAWAL_COOLDOWN_LEDGERS)
    }

    /// Return the ledger sequence of the last successful protocol-fee
    /// withdrawal, or `0` if no withdrawal has occurred yet.
    pub fn get_last_fee_withdrawal_ledger(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::LastFeeWithdrawalLedger)
            .unwrap_or(0u32)
    }
}
