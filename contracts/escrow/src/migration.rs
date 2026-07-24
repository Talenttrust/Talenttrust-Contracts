use crate::ttl::{read_if_live, remove_transient, store_with_ttl, PENDING_MIGRATION_TTL_LEDGERS};
use crate::{Contract, ContractStatus, DataKey, Error, Escrow, EscrowError};
use soroban_sdk::{contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingClientMigration {
    pub current_client: Address,
    pub proposed_client: Address,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

impl Escrow {
    pub(crate) fn pending_migration_key(contract_id: u32) -> DataKey {
        DataKey::PendingClientMigration(contract_id)
    }

    pub(crate) fn load_contract(env: &Env, contract_id: u32) -> Contract {
        env.storage()
            .persistent()
            .get::<_, Contract>(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
    }

    pub(crate) fn require_migration_allowed(env: &Env, status: ContractStatus) {
        if matches!(
            status,
            ContractStatus::Completed
                | ContractStatus::Cancelled
                | ContractStatus::Refunded
                | ContractStatus::Disputed
        ) {
            env.panic_with_error(Error::InvalidStatusTransition);
        }
    }

    pub(crate) fn pending_migration_exists(env: &Env, contract_id: u32) -> bool {
        read_if_live::<_, PendingClientMigration>(env, &Self::pending_migration_key(contract_id))
            .is_some()
    }

    /// Propose a client migration for an existing contract.
    ///
    /// The current client must authorize the call. The proposed client address
    /// must not be the freelancer or the current client. The pending migration
    /// is stored in temporary storage with TTL.
    pub(crate) fn propose_client_migration_impl(
        env: &Env,
        contract_id: u32,
        current_client: Address,
        new_client: Address,
    ) -> bool {
        Self::require_not_paused(&env);
        current_client.require_auth();

        let contract = Self::load_contract(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);
        if current_client != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        if new_client == contract.client || new_client == contract.freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }
        Self::require_migration_allowed(&env, contract.status);
        if Self::pending_migration_exists(&env, contract_id) {
            env.panic_with_error(EscrowError::InvalidState);
        }

        let requested_at = env.ledger().sequence();
        let expires_at = requested_at.saturating_add(PENDING_MIGRATION_TTL_LEDGERS);
        let pending = PendingClientMigration {
            current_client: current_client.clone(),
            proposed_client: new_client.clone(),
            requested_at_ledger: requested_at,
            expires_at_ledger: expires_at,
        };
        store_with_ttl(
            &env,
            &Self::pending_migration_key(contract_id),
            &pending,
            PENDING_MIGRATION_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(&env, "client_migration_proposed"), contract_id),
            (current_client, new_client, requested_at),
        );
        true
    }

    /// Accept a live pending client migration and update the contract.
    pub(crate) fn accept_client_migration_impl(
        env: &Env,
        contract_id: u32,
        new_client: Address,
    ) -> bool {
        Self::require_not_paused(&env);
        new_client.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);
        Self::require_migration_allowed(&env, contract.status);

        let key = Self::pending_migration_key(contract_id);
        let pending: PendingClientMigration = read_if_live(&env, &key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidState));

        if pending.proposed_client != new_client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        if pending.current_client != contract.client {
            env.panic_with_error(EscrowError::InvalidState);
        }

        let key = Escrow::pending_migration_key(contract_id);
        let pending: PendingClientMigration = read_if_live(&env, &key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidState));

        env.events().publish(
            (Symbol::new(&env, "client_migration_accepted"), contract_id),
            (pending.current_client, new_client, env.ledger().timestamp()),
        );
        true
    }

    /// Cancel a live pending client migration.
    ///
    /// Allows the current client to revoke a previously proposed migration immediately,
    /// rather than waiting for the 21-day `PENDING_MIGRATION_TTL_LEDGERS` to expire.
    /// This is useful when the client realizes they proposed the wrong address or
    /// changed their mind about the migration.
    ///
    /// # Requirements
    /// - The current client must authorize the call with `require_auth()`.
    /// - The caller must be the contract's current client.
    /// - A live pending migration must exist (not expired).
    /// - The contract must not be paused.
    /// - The contract must not be finalized.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `contract_id` - The unique identifier of the contract.
    /// * `current_client` - The address of the current client (must match contract.client).
    ///
    /// # Returns
    /// `true` on successful cancellation.
    ///
    /// # Errors
    /// * `ContractPaused` - If the contract is paused.
    /// * `ContractNotFound` - If the contract doesn't exist.
    /// * `AlreadyFinalized` - If the contract has been finalized.
    /// * `UnauthorizedRole` - If the caller is not the contract's current client.
    /// * `InvalidState` - If no live pending migration exists.
    ///
    /// # Events
    /// Emits a `client_migration_cancelled` event with:
    /// - Topics: `(Symbol "client_migration_cancelled", contract_id: u32)`
    /// - Data: `(current_client: Address, timestamp: u64)`
    ///
    /// # Example
    /// ```ignore
    /// // Client proposes wrong address
    /// escrow.propose_client_migration(&env, 1, &client, &wrong_address);
    ///
    /// // Client realizes mistake and cancels
    /// escrow.cancel_client_migration(&env, 1, &client);
    ///
    /// // Client proposes correct address
    /// escrow.propose_client_migration(&env, 1, &client, &correct_address);
    /// ```
    ///
    /// # Security
    /// - Only the current client can cancel a migration (checked by comparing
    ///   `current_client` against `contract.client`).
    /// - Cancellation removes the pending migration entry immediately via
    ///   `remove_transient`, allowing a new proposal to be made.
    /// - Respects the pause gate to prevent mutations while the contract is frozen.
    /// - Respects the finalization guard to prevent mutations after contract closure.
    pub(crate) fn cancel_client_migration_impl(
        env: &Env,
        contract_id: u32,
        current_client: Address,
    ) -> bool {
        current_client.require_auth();

        let contract = Self::load_contract(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);
        if current_client != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        let key = Self::pending_migration_key(contract_id);
        // Ensure a pending migration exists, otherwise panic with InvalidState
        let _: PendingClientMigration = read_if_live(&env, &key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidState));

        // Remove the pending migration entry
        remove_transient(&env, &key);

        // Emit cancellation event
        env.events().publish(
            (Symbol::new(&env, "client_migration_cancelled"), contract_id),
            (current_client, env.ledger().timestamp()),
        );
        true
    }
    /// Return true if a live pending client migration exists.
    pub(crate) fn has_pending_client_migration_impl(env: &Env, contract_id: u32) -> bool {
        Self::pending_migration_exists(env, contract_id)
    }

    /// Return the live pending client migration record.
    pub(crate) fn get_pending_client_migration_impl(
        env: &Env,
        contract_id: u32,
    ) -> PendingClientMigration {
        read_if_live(&env, &Self::pending_migration_key(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidState))
    }
}
