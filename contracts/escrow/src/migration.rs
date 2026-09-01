use crate::storage;
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
        // Block client migration when the contract holds escrowed funds.  Once
        // funds have been deposited (PartiallyFunded or Funded), the client
        // address is security-sensitive: the current client can cancel or
        // refund, so swapping the client after funding would allow the
        // original client to transfer cancellation rights to an accomplice
        // and drain escrowed funds.  Migration is only safe before any
        // deposit has been made (Created).
        if matches!(
            status,
            ContractStatus::Completed
                | ContractStatus::Cancelled
                | ContractStatus::Refunded
                | ContractStatus::Disputed
                | ContractStatus::PartiallyFunded
                | ContractStatus::Funded
        ) {
            env.panic_with_error(Error::InvalidStatusTransition);
        }
    }

    pub(crate) fn pending_migration_exists(env: &Env, contract_id: u32) -> bool {
        read_if_live::<_, PendingClientMigration>(env, &Self::pending_migration_key(contract_id))
            .is_some()
    }

    /// Validate that `candidate` does not overlap with any existing contract
    /// role (client, freelancer, arbiter) or the escrow contract's own address.
    ///
    /// Role overlap would collapse two independent authorization parties into
    /// one, defeating the release-authorization and dispute models.
    ///
    /// # Panics
    /// Panics with [`EscrowError::RoleOverlap`] when the candidate matches any
    /// existing role or the contract's own address.
    pub(crate) fn require_no_role_overlap(env: &Env, contract: &Contract, candidate: &Address) {
        if *candidate == contract.client
            || *candidate == contract.freelancer
            || contract.arbiter.as_ref() == Some(candidate)
            || *candidate == env.current_contract_address()
        {
            env.panic_with_error(EscrowError::RoleOverlap);
        }
    }

    /// Propose a client migration for an existing contract.
    ///
    /// The current client must authorize the call. The proposed client address
    /// must not overlap with any existing contract role (client, freelancer,
    /// arbiter) or the escrow contract's own address. The pending migration
    /// is stored in temporary storage with TTL.
    ///
    /// # Errors
    /// * [`EscrowError::UnauthorizedRole`] — caller is not the current client.
    /// * [`EscrowError::RoleOverlap`] — proposed address overlaps an existing role.
    /// * [`EscrowError::InvalidState`] — a pending migration already exists.
    pub(crate) fn propose_client_migration_impl(
        env: &Env,
        contract_id: u32,
        current_client: Address,
        new_client: Address,
    ) -> bool {
        storage::validate_contract_id_bounds(env, contract_id);
        Self::require_not_paused(&env);
        current_client.require_auth();

        let contract = Self::load_contract(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);
        if current_client != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        Self::require_no_role_overlap(env, &contract, &new_client);
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
    ///
    /// Re-validates role-overlap invariants against the **current** contract
    /// state, since roles may have changed between proposal and acceptance.
    ///
    /// # Errors
    /// * [`EscrowError::InvalidState`] — no live pending migration, or the
    ///   proposing client no longer matches `contract.client`.
    /// * [`EscrowError::UnauthorizedRole`] — caller is not the proposed client.
    /// * [`EscrowError::RoleOverlap`] — the proposed client now overlaps with
    ///   a contract role that changed after the proposal was created.
    pub(crate) fn accept_client_migration_impl(
        env: &Env,
        contract_id: u32,
        new_client: Address,
    ) -> bool {
        storage::validate_contract_id_bounds(env, contract_id);
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

        // Re-check role overlap at acceptance time: roles may have changed
        // between proposal and acceptance (e.g. arbiter was set, freelancer
        // address was updated via another mechanism).
        Self::require_no_role_overlap(env, &contract, &new_client);

        // Persist the updated client address
        contract.client = new_client.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        // Clear the pending migration record
        remove_transient(&env, &key);

        env.events().publish(
            (Symbol::new(&env, "client_migration_accepted"), contract_id),
            (pending.current_client, new_client, env.ledger().timestamp()),
        );
        true
    }

    /// Cancel a live pending client migration.
    ///
    /// The current client must authorize the call, be the contract's client, and a live pending migration must exist.
    /// The pending migration entry is removed and a `client_migration_cancelled` event is emitted.
    pub fn cancel_client_migration(env: Env, contract_id: u32, current_client: Address) -> bool {
        storage::validate_contract_id_bounds(&env, contract_id);
        Self::require_not_paused(&env);
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
