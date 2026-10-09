//! Governance-level privileged operations for the escrow contract.
//!
//! This module contains the implementation of the two-step admin transfer
//! protocol (propose → [timelock] → accept / cancel) and the protocol fee
//! management helpers.
//!
//! # Security invariants
//!
//! 1. **NotInitialized gate** — every mutating entrypoint calls
//!    `crate::Escrow::require_initialized` before touching any other state.
//!    This ensures that safety rails (admin, pause, protocol parameters) are
//!    always bound before money-flow operations can reference them.
//!
//! 2. **Self-proposal guard** — `propose_governance_admin_impl` rejects a
//!    proposal where `proposed == current_admin`. A self-proposal would make
//!    the timelock vacuous and could be used to reset the proposal clock.
//!
//! 3. **Timelock enforcement** — `accept_governance_admin_impl` verifies that
//!    at least `ADMIN_ROTATION_MIN_DELAY_LEDGERS` have elapsed since the
//!    matching proposal. Acceptance before this window returns
//!    `EscrowError::TimelockNotElapsed`.
//!
//! 4. **Cancellation guard** — `cancel_governance_admin_proposal_impl`
//!    requires a proposal to be pending. Cancelling a non-existent proposal
//!    returns `Error::NoPendingAdminProposal`.
//!
//! 5. **Parameter bounds** — `set_governed_params_impl` rejects a fee bps
//!    above 10 000 (100%) and a non-positive maximum escrow cap, surfacing
//!    `Error::InvalidProtocolParameters` in both cases.
//!
//! 6. **Deterministic state** — every accepted, cancelled, or overwritten
//!    proposal emits an event. No silent state transitions occur.
//!
//! # Event schema
//!
//! | Topics                                              | Payload                                     | Trigger                                |
//! | --------------------------------------------------- | ------------------------------------------- | -------------------------------------- |
//! | `(symbol_short!("admin"), Symbol("proposed"))`      | `(admin, proposed, timestamp)`              | `propose_governance_admin`             |
//! | `(symbol_short!("admin"), Symbol("accepted"))`      | `(old_admin, new_admin, timestamp)`         | `accept_governance_admin`              |
//! | `(symbol_short!("admin"), Symbol("cancelled"))`     | `(admin, cancelled_proposed, timestamp)`    | `cancel_governance_admin_proposal`     |
//! | `(Symbol("protocol_fee_bps"),)`                     | `(old_bps, new_bps, admin, timestamp)`      | `set_protocol_fee_bps`                 |

use crate::{DataKey, Error, Escrow, EscrowError};
use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

/// Pending admin proposal stored under `DataKey::PendingAdmin`.
///
/// Captures both the proposed address and the ledger at which the proposal
/// was made so the timelock can be checked deterministically.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAdminProposal {
    /// The address being proposed as the new admin.
    pub proposed: Address,
    /// The ledger sequence number when this proposal was stored.
    pub proposed_at_ledger: u32,
}

impl Escrow {
    // ── Protocol fee ─────────────────────────────────────────────────────────

    /// Set the protocol fee in basis points. Emits an event on success.
    ///
    /// # Invariants
    /// - Contract must be initialized; panics `EscrowError::NotInitialized` otherwise.
    /// - Admin must be stored and must authorize the call.
    ///
    /// # Events
    /// `(Symbol("protocol_fee_bps"),)` → `(old_bps, new_bps, admin, timestamp)`
    pub(crate) fn set_protocol_fee_bps_impl(env: &Env, new_bps: u32) -> bool {
        Self::require_initialized(env);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        admin.require_auth();

        let old_bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBps, &new_bps);

        env.events().publish(
            (Symbol::new(env, "protocol_fee_bps"),),
            (old_bps, new_bps, admin.clone(), env.ledger().timestamp()),
        );
        true
    }

    // ── Two-step admin transfer ───────────────────────────────────────────────

    /// Propose a new governance admin. Stores the proposal with a timelock.
    ///
    /// A proposal where `proposed == current_admin` is rejected immediately
    /// with `Error::CannotProposeSelf`. This prevents the timelock from being
    /// trivialized by a self-transfer.
    ///
    /// Re-proposing while a proposal is already pending silently overwrites
    /// the existing entry. The overwrite resets the timelock clock — the
    /// earliest the new proposal can be accepted is
    /// `ADMIN_ROTATION_MIN_DELAY_LEDGERS` from the re-proposal ledger.
    ///
    /// # Invariants
    /// - Panics `Error::NotInitialized` if contract is not initialized.
    /// - Panics `Error::CannotProposeSelf` if `proposed == current_admin`.
    /// - Current admin must authorize the call.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("proposed"))` → `(admin, proposed, timestamp)`
    pub(crate) fn propose_governance_admin_impl(env: &Env, proposed: Address) -> bool {
        // Gate 1: contract must be initialized
        Self::require_initialized(env);

        // Load current admin — must exist if initialized
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Gate 2: self-proposal guard — proposing yourself is a no-op that
        // would silently bypass the two-step timelock protection.
        // Invariant: proposed ≠ admin.
        if proposed == admin {
            env.panic_with_error(Error::CannotProposeSelf);
        }

        // Gate 3: current admin must authorize
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
    /// # Invariants
    /// - Panics `Error::NotInitialized` if contract is not initialized.
    /// - Panics `Error::InvalidState` if no pending proposal exists.
    /// - Panics `EscrowError::TimelockNotElapsed` if the minimum delay has not
    ///   elapsed since the proposal.
    /// - Proposed admin must authorize the call.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("accepted"))` → `(old_admin, new_admin, timestamp)`
    pub(crate) fn accept_governance_admin_impl(env: &Env) -> bool {
        // Gate 1: contract must be initialized
        Self::require_initialized(env);

        // Gate 2: pending proposal must exist
        let pending: PendingAdminProposal = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| env.panic_with_error(Error::InvalidState));

        // Gate 3: timelock enforcement — elapsed = current_sequence - proposed_at_ledger.
        // `saturating_sub` is safe here: if sequence < proposed_at_ledger (e.g. in tests
        // where the clock is set lower), elapsed = 0 and the check fires normally.
        let elapsed = env
            .ledger()
            .sequence()
            .saturating_sub(pending.proposed_at_ledger);
        if elapsed < crate::ADMIN_ROTATION_MIN_DELAY_LEDGERS {
            env.panic_with_error(EscrowError::TimelockNotElapsed);
        }

        // Gate 4: proposed admin must authorize the transfer
        let pending_admin = pending.proposed;
        pending_admin.require_auth();

        // Read old admin for event payload
        let old_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Commit: promote pending admin, clear the pending entry atomically.
        // Invariant after this block: DataKey::Admin == pending_admin,
        // DataKey::PendingAdmin does not exist.
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

    /// Cancel a pending governance admin proposal.
    ///
    /// Only the current admin can cancel. This is the escape hatch for an
    /// accidental or adversarial proposal — rather than waiting for the
    /// timelock to expire, the current admin can immediately clear it.
    ///
    /// # Invariants
    /// - Panics `Error::NotInitialized` if contract is not initialized.
    /// - Panics `Error::NoPendingAdminProposal` if no proposal is pending.
    /// - Current admin must authorize the call.
    ///
    /// # Events
    /// `(symbol_short!("admin"), Symbol("cancelled"))` → `(admin, cancelled_proposed, timestamp)`
    pub(crate) fn cancel_governance_admin_proposal_impl(env: &Env) -> bool {
        // Gate 1: contract must be initialized
        Self::require_initialized(env);

        // Gate 2: pending proposal must exist
        let pending: PendingAdminProposal = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| env.panic_with_error(Error::NoPendingAdminProposal));

        // Gate 3: current admin must authorize the cancellation
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        admin.require_auth();

        // Commit: remove the pending proposal.
        // Invariant after this block: DataKey::PendingAdmin does not exist.
        let cancelled_proposed = pending.proposed;
        env.storage().persistent().remove(&DataKey::PendingAdmin);

        // Emit cancellation event so indexers can track the full lifecycle
        env.events().publish(
            (symbol_short!("admin"), Symbol::new(env, "cancelled")),
            (admin, cancelled_proposed, env.ledger().timestamp()),
        );
        true
    }

    /// Return the currently pending admin address, if any.
    ///
    /// Returns `None` when no proposal is pending. This is a read-only query
    /// and never panics.
    pub(crate) fn get_pending_governance_admin_impl(env: Env) -> Option<Address> {
        let proposal: Option<PendingAdminProposal> =
            env.storage().persistent().get(&DataKey::PendingAdmin);
        proposal.map(|p| p.proposed)
    }

    /// Return the current admin address.
    ///
    /// Returns `None` if `initialize` has not yet been called. This is a
    /// read-only query and never panics.
    pub(crate) fn get_governance_admin_impl(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }
}
