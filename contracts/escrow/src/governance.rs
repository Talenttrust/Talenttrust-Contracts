//! Governance and protocol-configuration entrypoints.
//!
//! This module owns admin-controlled persistent configuration:
//! `DataKey::Admin` for authorization, `ProtocolFeeBps` for release fees,
//! `GovernedParameters` for escrow caps, `ReadinessChecklist` for deployment
//! readiness state, and `PendingAdmin` for two-step admin rotation proposals.
//! Money movement for protocol-fee withdrawal remains in the crate root because
//! it performs settlement-token transfers.

use crate::ttl::ADMIN_ROTATION_MIN_DELAY_LEDGERS;
use crate::{
    DataKey, Error, Escrow, EscrowArgs, EscrowClient, GovernedParameters, PendingAdminProposal,
    ReadinessChecklist,
};
use soroban_sdk::{symbol_short, Address, Env, Symbol};

#[soroban_sdk::contractimpl]
impl Escrow {
    /// Set the protocol fee in basis points.
    ///
    /// Admin-gated: the stored admin (under [`DataKey::Admin`]) must authorize
    /// the call and the contract must be initialized.
    ///
    /// ## Bounds
    ///
    /// `new_bps` must be strictly less than `10_000` (i.e. `0 ≤ new_bps ≤ 9_999`).
    /// Values `≥ 10_000` are rejected with [`Error::InvalidProtocolParameters`]
    /// because they would make the computed fee equal to or exceed the released
    /// milestone amount, leaving the freelancer with zero or negative net payout.
    ///
    /// * `0 bps` — fee collection disabled (default).
    /// * `1–9_999 bps` — fractional fee between 0.01 % and 99.99 %.
    /// * `≥ 10_000 bps` — **rejected** with `InvalidProtocolParameters` (code 49).
    ///
    /// The fee takes effect immediately for the next `release_milestone` call.
    ///
    /// See [`docs/escrow/protocol-fees.md`](../../../docs/escrow/protocol-fees.md) for
    /// the basis-point model, fee formula, accrual storage, and withdrawal flow.
    ///
    /// ## Events
    ///
    /// `(Symbol("protocol_fee_bps"),)` → `(old_bps: u32, new_bps: u32, admin: Address, timestamp: u64)`
    ///
    /// ## Errors
    ///
    /// * [`Error::NotInitialized`] — `initialize` has not been called.
    /// * [`Error::InvalidProtocolParameters`] (code 49) — `new_bps ≥ 10_000`.
    pub fn set_protocol_fee_bps(env: Env, new_bps: u32) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();

        // Reject fee rates >= 10_000 bps (100%).  A fee that equals or exceeds
        // the milestone amount would leave the freelancer with zero or negative
        // net payout — a critical invariant violation.
        if new_bps >= 10_000 {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        let old_bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBps, &new_bps);

        env.events().publish(
            (Symbol::new(&env, "protocol_fee_bps"),),
            (old_bps, new_bps, admin.clone(), env.ledger().timestamp()),
        );
        true
    }

    pub fn get_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    /// Returns the current protocol fee in basis points.
    pub fn get_protocol_fee_bps(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get::<_, u32>(&DataKey::ProtocolFeeBps)
            .unwrap_or(0)
    }

    // ── Two-step admin transfer ───────────────────────────────────────────────

    /// Propose a new governance admin. Stores the proposal with a timelock.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("proposed"))` → `(admin, proposed, timestamp)`
    pub(crate) fn propose_governance_admin_impl(env: &Env, proposed: Address) -> bool {
        Self::require_initialized(env);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        admin.require_auth();

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

    /// Accept a pending admin proposal, enforcing the timelock.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("accepted"))` → `(old_admin, new_admin, timestamp)`
    pub(crate) fn accept_governance_admin_impl(env: &Env) -> bool {
        Self::require_initialized(env);

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

        let pending_admin = pending.proposed;
        pending_admin.require_auth();

        let old_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

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

    /// Cancel a pending governance admin proposal, aborting a two-step transfer.
    ///
    /// Only the current admin (the address stored under [`DataKey::Admin`]) may
    /// cancel, and the contract must be initialized. On success the pending
    /// proposal is removed so the previously proposed address can no longer call
    /// [`Escrow::accept_governance_admin`] — a subsequent accept panics with
    /// [`Error::InvalidState`].
    ///
    /// # Errors
    /// * [`Error::NotInitialized`] — `initialize` has not been called.
    /// * [`Error::InvalidState`] — there is no pending proposal to cancel.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("cancelled"))` → `(admin, cancelled_proposal, timestamp)`
    pub(crate) fn cancel_governance_admin_proposal_impl(env: &Env) -> bool {
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

        env.storage().persistent().remove(&DataKey::PendingAdmin);

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(env, "cancelled")),
            (admin, pending.proposed, env.ledger().timestamp()),
        );
        true
    }

    /// Internal: return the currently pending admin address, if any.
    pub(crate) fn get_pending_governance_admin_impl(env: &Env) -> Option<Address> {
        let proposal: Option<PendingAdminProposal> =
            env.storage().persistent().get(&DataKey::PendingAdmin);
        proposal.map(|p| p.proposed)
    }

    /// Internal: return the current admin address.
    pub(crate) fn get_governance_admin_impl(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    /// Set both governance parameters at once and update the readiness checklist.
    ///
    /// Sets `protocol_fee_bps` (must be `≤ 10_000`) and `max_escrow_total_stroops`
    /// atomically. Also flips `ReadinessChecklist::governed_params_set` to `true`.
    ///
    /// See [`docs/escrow/protocol-fees.md`](../../../docs/escrow/protocol-fees.md) for
    /// the full basis-point model and fee lifecycle.
    pub fn set_governed_params(
        env: Env,
        admin: Address,
        protocol_fee_bps: u32,
        max_escrow_total_stroops: i128,
    ) -> bool {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&crate::DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::NotInitialized);
        }

        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));

        if admin != stored_admin {
            env.panic_with_error(Error::UnauthorizedRole);
        }
        admin.require_auth();

        if protocol_fee_bps > 10_000 {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        let params = GovernedParameters {
            protocol_fee_bps,
            max_escrow_total_stroops,
        };
        env.storage()
            .persistent()
            .set(&DataKey::GovernedParameters, &params);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default();
        checklist.governed_params_set = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        true
    }

    /// Retrieve the current governed parameters.
    pub fn get_governed_parameters(env: Env) -> Option<GovernedParameters> {
        env.storage().persistent().get(&DataKey::GovernedParameters)
    }
}
