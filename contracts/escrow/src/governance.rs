use crate::{DataKey, Error as EscrowError};
use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Governance-related privileged operations and audit events.
///
/// This module implements a small set of admin-facing functions that
/// produce parseable events for off-chain indexers. Events emitted here
/// follow the existing convention of short `symbol_short!` topics used by
/// other lifecycle events (e.g. `init`, `paused`, `emergency`).
#[allow(dead_code)]
impl super::Escrow {
    /// Set the protocol fee (basis points). Emits an event with
    /// `(old_bps, new_bps, admin, timestamp)` under topic `protocol_fee_bps`.
    ///
    /// Requirements:
    /// - Contract must be initialized.
    /// - Caller must be the stored admin.
    /// - New bps must be <= 1000 (10%).
    pub fn set_protocol_fee_bps(env: Env, new_bps: u32) -> bool {
        // require initialized
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&crate::DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::NotInitialized);
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        admin.require_auth();

        if new_bps > 1000 {
            env.panic_with_error(EscrowError::InvalidFeeBps);
        }

        let old_bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBps, &new_bps);

        // Emit audit-style event for protocol fee change. Topic uses the
        // short symbol to remain consistent with other contract events.
        env.events().publish(
            (Symbol::new(&env, "protocol_fee_bps"),),
            (old_bps, new_bps, admin.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Propose a new admin. Stores the `pending` admin and emits an event
    /// `(current_admin, proposed_admin, timestamp)` under topic
    /// `(admin, "proposed")`.
    pub fn propose_governance_admin(env: Env, proposed: Address) -> bool {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&crate::DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::NotInitialized);
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &proposed);

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(&env, "proposed")),
            (admin, proposed.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Accept a pending admin proposal. The caller must be the proposed
    /// admin. Emits `(old_admin, new_admin, timestamp)` under
    /// `(admin, "accepted")` and clears the pending admin.
    pub fn accept_governance_admin(env: Env) -> bool {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&crate::DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::NotInitialized);
        }

        let pending: Option<Address> = env.storage().persistent().get(&DataKey::PendingAdmin);
        if pending.is_none() {
            env.panic_with_error(EscrowError::InvalidState);
        }
        let pending_admin = pending.unwrap();

        // The proposed admin must authorize acceptance.
        pending_admin.require_auth();

        let old_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));

        env.storage()
            .persistent()
            .set(&DataKey::Admin, &pending_admin);
        // clear pending admin
        env.storage().persistent().remove(&DataKey::PendingAdmin);

        env.events().publish(
            (symbol_short!("admin"), Symbol::new(&env, "accepted")),
            (old_admin, pending_admin.clone(), env.ledger().timestamp()),
        );
        true
    }

    /// Return the currently pending admin, if any.
    pub fn get_pending_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::PendingAdmin)
    }

    /// Return the current admin address.
    pub fn get_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }
}
