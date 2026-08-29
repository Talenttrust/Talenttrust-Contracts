//! Governance and protocol-configuration entrypoints.
//!
//! This module owns admin-controlled persistent configuration:
//! `DataKey::Admin` for authorization, `ProtocolFeeBps` for release fees,
//! `GovernedParameters` for escrow caps, `ReadinessChecklist` for deployment
//! readiness state, and `PendingAdmin` for two-step admin rotation proposals.
//! Money movement for protocol-fee withdrawal remains in the crate root because
//! it performs settlement-token transfers.

use crate::storage_validation;
use crate::ttl::{self, ADMIN_ROTATION_MIN_DELAY_LEDGERS};
use crate::{
    DataKey, Error, Escrow, EscrowArgs, EscrowClient, GovernedParameters, PendingAdminProposal,
    ReadinessChecklist, MAX_FEE_BPS, MAX_MAX_MILESTONES, MIN_MAX_MILESTONES,
};
use soroban_sdk::{contractimpl, symbol_short, Address, Env, Symbol};

#[contractimpl]
impl Escrow {
    /// Set the protocol fee in basis points.
    ///
    /// Admin-gated: the stored admin (under [`DataKey::Admin`]) must authorize
    /// the call and the contract must be initialized.
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

        storage_validation::validate_protocol_fee_bps(&env, new_bps);
        if new_bps > 10_000 {
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

    /// Set the maximum allowed milestones per contract (admin-controlled).
    ///
    /// The stored admin must authorize the call. The provided
    /// `max_milestones` is validated against compile-time safe bounds and a
    /// typed `LimitOutOfRange` error is returned for invalid values.
    pub fn set_max_milestones(env: Env, max_milestones: u32) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();

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

    /// Propose a new governance admin. Stores the proposal with a timelock.
    ///
    /// Public entrypoint that delegates to [`propose_governance_admin_impl`].
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("proposed"))` → `(admin, proposed, timestamp)`
    pub fn propose_governance_admin(env: Env, proposed: Address) -> bool {
        Self::propose_governance_admin_impl(&env, proposed)
    }

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
    /// Public entrypoint that delegates to [`accept_governance_admin_impl`].
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("accepted"))` → `(old_admin, new_admin, timestamp)`
    pub fn accept_governance_admin(env: Env) -> bool {
        Self::accept_governance_admin_impl(&env)
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

        storage_validation::validate_escrow_total_cap(&env, new_parameters.max_escrow_total_stroops);
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
    /// `10_000` (100 %) are rejected with [`Error::InvalidProtocolParameters`].
    ///
    /// Stored under [`DataKey::FeeWithdrawalCap`].  Default is `5_000` (50 %).
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

        if cap_bps > 10_000 {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        let old_cap: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::FeeWithdrawalCap)
            .unwrap_or(5_000u32);

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
    /// Returns the stored value, or the default of `5_000` (50 %) when
    /// no value has been explicitly set.
    pub fn get_fee_withdrawal_cap(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::FeeWithdrawalCap)
            .unwrap_or(5_000u32)
    }

    /// Set the minimum number of ledgers that must elapse between successful
    /// protocol-fee withdrawals.
    ///
    /// Admin-gated, must be initialized.  A value of `0` disables the cooldown
    /// (unlimited frequency, subject to the cap).  Values above
    /// `2_592_000` (≈150 days at 5 s ledgers) are rejected with
    /// [`Error::InvalidProtocolParameters`].
    ///
    /// Stored under [`DataKey::FeeWithdrawalCooldownLedgers`].
    /// Default is `17_280` (≈1 day at 5 s ledgers).
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
        if cooldown_ledgers > 2_592_000 {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        let old_cooldown: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::FeeWithdrawalCooldownLedgers)
            .unwrap_or(17_280u32);

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
    /// Returns the stored value, or the default of `17_280` (≈1 day at
    /// 5 s ledgers) when no value has been explicitly set.
    pub fn get_fee_withdrawal_cooldown(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::FeeWithdrawalCooldownLedgers)
            .unwrap_or(17_280u32)
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
