//! Client migration and per-contract storage schema upgrade logic.
//!
//! ## Client migration (two-step transfer)
//!
//! Client identity migration follows a **propose → accept** flow so that both
//! the current client and the proposed client must independently authorise the
//! change.  A pending record is written to **temporary** storage with a 21-day
//! TTL; auto-eviction enforces the acceptance window without any cleanup code
//! path.
//!
//! ### Invariants
//! 1. Only the stored `contract.client` may propose a migration.
//! 2. The proposed address must not overlap any existing role (client,
//!    freelancer, arbiter, escrow contract address).
//! 3. Migrations are blocked on terminal contract statuses: `Completed`,
//!    `Cancelled`, `Refunded`, `Disputed`.
//! 4. At most one pending migration may exist per contract at any time.
//! 5. Only the proposed address may accept the migration.
//! 6. Role-overlap is re-validated at acceptance time (roles may have changed
//!    between proposal and acceptance).
//! 7. Only the current client may cancel a pending migration.
//! 8. Cancel is blocked on terminal contract statuses (same guard as propose).
//! 9. TTL is bumped on every read of the pending record to prevent
//!    eviction during an active usage window.
//!
//! ## Per-contract storage schema migration
//!
//! The `Contract` struct has evolved over time.  To avoid a forced global
//! re-write every time a new field is added, contract records carry a
//! per-record schema version stored under `DataKey::ContractSchemaVersion(id)`.
//!
//! The canonical read helper (`load_contract_migrated`) applies a
//! **migrate-on-read** strategy:
//!
//! * If the version marker is absent (legacy record written before versioning),
//!   the record is decoded as `ContractV1`, the missing fields are filled with
//!   their safe defaults, and the upgraded `Contract` (v2) is written back in
//!   place.
//! * If the version marker equals `CONTRACT_STORAGE_SCHEMA_VERSION`, the record
//!   is returned as-is (fast path, no write).
//!
//! This is idempotent and safe under concurrent reads: a redundant upgrade
//! (two invocations arriving simultaneously) writes the same value and leaves
//! storage in the same correct state.

use crate::storage;
use crate::ttl::{
    extend_if_below_threshold, read_if_live, remove_transient, store_with_ttl,
    PENDING_MIGRATION_BUMP_THRESHOLD, PENDING_MIGRATION_TTL_LEDGERS, PERSISTENT_BUMP_THRESHOLD,
    PERSISTENT_TTL_LEDGERS,
};
use crate::{Contract, ContractStatus, DataKey, Error, Escrow, EscrowError};
use soroban_sdk::{contracttype, Address, Env, Symbol};

// ── ContractV1 (pre-reputation_issued layout) ────────────────────────────────

/// Legacy `Contract` layout written before the `reputation_issued` field was
/// added.  Retained so that migrate-on-read can decode old records without
/// requiring a global re-write of all existing contracts.
///
/// Do **not** add new fields here; bump `CONTRACT_STORAGE_SCHEMA_VERSION` and
/// create `ContractV2` (→ `ContractV3`, etc.) instead.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractV1 {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub status: ContractStatus,
    pub total_deposited: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub release_authorization: crate::ReleaseAuthorization,
}

/// The contract storage schema version written by this WASM build.
///
/// Increment this when the `Contract` struct gains or loses fields.  Every new
/// version must have a corresponding migration step inside
/// `load_contract_migrated`.
pub const CONTRACT_STORAGE_SCHEMA_VERSION: u32 = 2;

// ── PendingClientMigration record ─────────────────────────────────────────────

/// A pending client migration proposal, stored under
/// `DataKey::PendingClientMigration(contract_id)` in **temporary** storage.
///
/// The record is auto-evicted after `PENDING_MIGRATION_TTL_LEDGERS` ledgers
/// (≈21 days) if not accepted or cancelled first.  Any reader that touches a
/// live record calls `extend_if_below_threshold` to renew the TTL and prevent
/// eviction during an active usage window.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingClientMigration {
    pub current_client: Address,
    pub proposed_client: Address,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

/// Record of a completed migration, used to make recovery deterministic.
///
/// This is written in the same logical step as the contract update and the
/// pending-migration removal, so a retry or partial failure can always observe
/// whether the migration already completed and avoid double-applying it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedClientMigration {
    pub previous_client: Address,
    pub current_client: Address,
    pub completed_at_ledger: u32,
}

impl Escrow {
    // ── Storage key helpers ──────────────────────────────────────────────────

    pub(crate) fn pending_migration_key(contract_id: u32) -> DataKey {
        DataKey::PendingClientMigration(contract_id)
    }

    // ── Contract loading ─────────────────────────────────────────────────────

    /// Load a contract from persistent storage, panicking with
    /// `ContractNotFound` if absent.
    ///
    /// Does **not** perform schema migration; use `load_contract_migrated` when
    /// the caller may encounter pre-v2 records.
    pub(crate) fn load_contract(env: &Env, contract_id: u32) -> Contract {
        env.storage()
            .persistent()
            .get::<_, Contract>(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
    }

    /// Load a contract and transparently upgrade it from `ContractV1` to the
    /// current layout when the per-record schema version marker is absent or < 2.
    ///
    /// On a first-time read of a legacy record the upgraded `Contract` and its
    /// version marker are written back so subsequent reads take the fast path.
    ///
    /// # Panics
    /// `ContractNotFound` if no record exists at all (not even a v1 record).
    pub(crate) fn load_contract_migrated(env: &Env, contract_id: u32) -> Contract {
        let version: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ContractSchemaVersion(contract_id))
            .unwrap_or(1);

        if version >= CONTRACT_STORAGE_SCHEMA_VERSION {
            // Fast path: record is already at the current version.
            return env
                .storage()
                .persistent()
                .get::<_, Contract>(&DataKey::Contract(contract_id))
                .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        }

        // Slow path: decode the legacy v1 layout and upgrade in place.
        // If the v1 decode also fails, the record is truly absent.
        let v1: ContractV1 = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        let upgraded = Contract {
            client: v1.client,
            freelancer: v1.freelancer,
            arbiter: v1.arbiter,
            status: v1.status,
            total_deposited: v1.total_deposited,
            funded_amount: v1.funded_amount,
            released_amount: v1.released_amount,
            refunded_amount: v1.refunded_amount,
            release_authorization: v1.release_authorization,
            // New field added in v2: default false (reputation not yet issued for legacy records).
            reputation_issued: false,
        };

        // Persist upgraded record and version marker atomically.
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &upgraded);
        env.storage()
            .persistent()
            .set(&DataKey::ContractSchemaVersion(contract_id), &CONTRACT_STORAGE_SCHEMA_VERSION);

        // Bump TTL so the freshly-upgraded record survives as long as any
        // new record written today.
        env.storage().persistent().extend_ttl(
            &DataKey::Contract(contract_id),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_TTL_LEDGERS,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::ContractSchemaVersion(contract_id),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_TTL_LEDGERS,
        );

        upgraded
    }

    /// Write a contract version marker for a newly created contract.
    ///
    /// Called by `create_contract` after the initial `Contract` is persisted so
    /// that the record is immediately recognised as current-version by
    /// `load_contract_migrated`.
    pub(crate) fn set_contract_schema_version(env: &Env, contract_id: u32) {
        env.storage().persistent().set(
            &DataKey::ContractSchemaVersion(contract_id),
            &CONTRACT_STORAGE_SCHEMA_VERSION,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::ContractSchemaVersion(contract_id),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_TTL_LEDGERS,
        );
    }

    // ── Guard helpers ────────────────────────────────────────────────────────

    /// Reject migration operations on terminal contract statuses.
    ///
    /// Terminal contracts have no pending work and no future state transitions;
    /// changing the client address on them would be misleading and could
    /// interfere with post-terminal accounting queries.
    ///
    /// # Panics
    /// `InvalidStatusTransition` for `Completed`, `Cancelled`, `Refunded`,
    /// `Disputed`.
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

    /// Return `true` if a **live** pending migration exists for `contract_id`.
    ///
    /// Bumps the TTL of the pending record if it is live and within
    /// `PENDING_MIGRATION_BUMP_THRESHOLD` of expiry, preventing eviction during
    /// an active usage window.
    pub(crate) fn pending_migration_exists(env: &Env, contract_id: u32) -> bool {
        let key = Self::pending_migration_key(contract_id);
        // extend_if_below_threshold returns false when the key is absent / evicted.
        extend_if_below_threshold(
            env,
            &key,
            PENDING_MIGRATION_BUMP_THRESHOLD,
            PENDING_MIGRATION_TTL_LEDGERS,
        )
    }

    /// Load the live pending migration record for `contract_id`.
///
/// Returns `None` when no record exists or the record has expired
/// (ledger sequence >= `expires_at_ledger`). This is the single
/// authoritative liveness check used by all mutating entry points.
/// Callers that need a live record must panic with
/// [`EscrowError::InvalidState`] when this returns `None`.
    pub(crate) fn load_live_pending_migration(
        env: &Env,
        contract_id: u32,
    ) -> Option<PendingClientMigration> {
        read_if_live::<_, PendingClientMigration>(
            env,
            &Self::pending_migration_key(contract_id),
        )
    }

    /// Validate that `candidate` does not overlap with any existing contract
    /// role (client, freelancer, arbiter) or the escrow contract's own address.
    ///
    /// Role overlap would collapse two independent authorization parties into
    /// one, defeating the release-authorization and dispute models.
    ///
    /// # Panics
    /// `RoleOverlap` when the candidate matches any existing role or the
    /// contract's own address.
    pub(crate) fn require_no_role_overlap(env: &Env, contract: &Contract, candidate: &Address) {
        if *candidate == contract.client
            || *candidate == contract.freelancer
            || contract.arbiter.as_ref() == Some(candidate)
            || *candidate == env.current_contract_address()
        {
            env.panic_with_error(EscrowError::RoleOverlap);
        }
    }

    // ── Mutating entrypoints ─────────────────────────────────────────────────

    /// Propose a client migration for an existing contract.
    ///
    /// The current client must authorize the call.  The proposed client address
    /// must not overlap with any existing contract role (client, freelancer,
    /// arbiter) or the escrow contract's own address.  The pending migration is
    /// stored in **temporary** storage with a 21-day TTL.
    ///
    /// ## Validation order
    /// 1. `contract_id` bounds (≥ 1).
    /// 2. Contract not paused / in emergency.
    /// 3. `current_client.require_auth()` — caller authorization.
    /// 4. Contract not finalized.
    /// 5. `current_client` matches stored `contract.client`.
    /// 6. Role-overlap check on `new_client`.
    /// 7. Terminal-status guard (`require_migration_allowed`).
    /// 8. No duplicate pending migration.
    ///
    /// # Errors
    /// * `ContractNotFound` — `contract_id` is 0 or not found.
    /// * `ContractPaused` / `EmergencyActive` — system is halted.
    /// * `UnauthorizedRole` — caller is not the current client.
    /// * `AlreadyFinalized` — contract is finalized.
    /// * `RoleOverlap` — proposed address overlaps an existing role.
    /// * `InvalidStatusTransition` — contract is in a terminal status.
    /// * `InvalidState` — a pending migration already exists.
    pub(crate) fn propose_client_migration_impl(
        env: &Env,
        contract_id: u32,
        current_client: Address,
        new_client: Address,
    ) -> bool {
        // 1. bounds
        storage::validate_contract_id_bounds(env, contract_id);
        // 2. pause guard
        Self::require_not_paused(env);
        // 3. caller authorization
        current_client.require_auth();

        let contract = Self::load_contract(env, contract_id);
        // 4. finalization guard
        Self::require_not_finalized(env, contract_id);
        // 5. identity check
        if current_client != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        // 6. role-overlap guard
        Self::require_no_role_overlap(env, &contract, &new_client);
        // 7. terminal-status guard
        Self::require_migration_allowed(env, contract.status);
        // 8. duplicate guard
        if Self::pending_migration_exists(env, contract_id) {
            env.panic_with_error(EscrowError::InvalidState);
        }
        Self::require_no_role_overlap(env, &contract, &new_client);

        let requested_at = env.ledger().sequence();
        let expires_at = requested_at.saturating_add(PENDING_MIGRATION_TTL_LEDGERS);
        let pending = PendingClientMigration {
            current_client: current_client.clone(),
            proposed_client: new_client.clone(),
            requested_at_ledger: requested_at,
            expires_at_ledger: expires_at,
        };
        store_with_ttl(
            env,
            &Self::pending_migration_key(contract_id),
            &pending,
            PENDING_MIGRATION_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(env, "client_migration_proposed"), contract_id),
            (current_client, new_client, requested_at),
        );
        true
    }

    /// Accept a live pending client migration and update the contract.
    ///
    /// Re-validates role-overlap invariants against the **current** contract
    /// state, since roles may have changed between proposal and acceptance.
    ///
    /// ## Validation order
    /// 1. `contract_id` bounds.
    /// 2. Contract not paused / in emergency.
    /// 3. `new_client.require_auth()` — proposed client authorization.
    /// 4. Contract not finalized.
    /// 5. Terminal-status guard.
    /// 6. Live pending record exists; `new_client` matches `pending.proposed_client`.
    /// 7. Proposing client still matches `contract.client` (no interleaved rotation).
    /// 8. Re-check role overlap (roles may have changed after the proposal).
    ///
    /// # Errors
    /// * `ContractNotFound` — `contract_id` is 0 or not found.
    /// * `ContractPaused` / `EmergencyActive` — system is halted.
    /// * `UnauthorizedRole` — caller is not the proposed client.
    /// * `AlreadyFinalized` — contract is finalized.
    /// * `InvalidStatusTransition` — contract is in a terminal status.
    /// * `InvalidState` — no live pending migration, or proposing client mismatch.
    /// * `RoleOverlap` — proposed client now overlaps a role changed after proposal.
    pub(crate) fn accept_client_migration_impl(
        env: &Env,
        contract_id: u32,
        new_client: Address,
    ) -> bool {
        // 1. bounds
        storage::validate_contract_id_bounds(env, contract_id);
        // 2. pause guard
        Self::require_not_paused(env);
        // 3. caller authorization
        new_client.require_auth();

        let mut contract = Self::load_contract(env, contract_id);
        // 4. finalization guard
        Self::require_not_finalized(env, contract_id);
        // 5. terminal-status guard
        Self::require_migration_allowed(env, contract.status);

        let key = Self::pending_migration_key(contract_id);
        // 6a. Bump TTL while reading so the record cannot be evicted partway
        //     through the validation sequence (e.g. if the host batches ledgers).
        extend_if_below_threshold(
            env,
            &key,
            PENDING_MIGRATION_BUMP_THRESHOLD,
            PENDING_MIGRATION_TTL_LEDGERS,
        );
        let pending: PendingClientMigration = read_if_live(env, &key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidState));

        // 6b. Caller must be the named proposed client.
        if pending.proposed_client != new_client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        // 7. Original proposer must still be the contract client.
        if pending.current_client != contract.client {
            env.panic_with_error(EscrowError::InvalidState);
        }
        // No-op invariant: the proposed client must differ from the current
        // client at acceptance time.
        if contract.client == new_client {
            env.panic_with_error(EscrowError::RoleOverlap);
        }

        // 8. Re-check role overlap at acceptance time: roles may have changed
        //    between proposal and acceptance (e.g. arbiter was set, freelancer
        //    address was updated via another mechanism).
        Self::require_no_role_overlap(env, &contract, &new_client);

        // Effects: update client address and clear pending record.
        contract.client = new_client.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);
        remove_transient(env, &key);

        env.events().publish(
            (Symbol::new(env, "client_migration_accepted"), contract_id),
            (pending.current_client, new_client, env.ledger().timestamp()),
        );
        true
    }

    /// Cancel a live pending client migration.
    ///
    /// The current client must authorize the call, be the contract's client, and a live pending
    /// migration must exist. The pending migration entry is removed and a
    /// `client_migration_cancelled` event is emitted.
    ///
    /// # Errors
    /// * [`EscrowError::UnauthorizedRole`] — `current_client` is not the stored contract client.
    /// * [`EscrowError::InvalidState`] — no live pending migration exists.
    pub(crate) fn cancel_client_migration_inner(
        env: &Env,
        contract_id: u32,
        current_client: Address,
    ) -> bool {
        storage::validate_contract_id_bounds(env, contract_id);
        current_client.require_auth();

        let contract = Self::load_contract(env, contract_id);
        Self::require_not_finalized(env, contract_id);
        if current_client != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        // 6. terminal-status guard — cancel on a terminal contract is invalid
        Self::require_migration_allowed(env, contract.status);

        let key = Self::pending_migration_key(contract_id);
        // Ensure a pending migration exists, otherwise panic with InvalidState.
        let _: PendingClientMigration = read_if_live(env, &key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidState));

        // Remove the pending migration entry.
        remove_transient(env, &key);

        // Emit cancellation event.
        env.events().publish(
            (Symbol::new(env, "client_migration_cancelled"), contract_id),
            (current_client, env.ledger().timestamp()),
        );
        true
    }

    // ── Read-only helpers ────────────────────────────────────────────────────

    /// Return `true` if a live pending client migration exists for `contract_id`.
    ///
    /// Bumps the pending record TTL if it is live and within the bump
    /// threshold, extending its lifetime under active usage.
    ///
    /// Read-only entrypoints are not blocked by pause/emergency.
    pub(crate) fn has_pending_client_migration_impl(env: &Env, contract_id: u32) -> bool {
        Self::pending_migration_exists(env, contract_id)
    }

    /// Return the live pending client migration record.
    ///
    /// Bumps the TTL before returning so an immediate follow-up
    /// `accept_client_migration` cannot race the eviction window.
    ///
    /// # Panics
    /// `InvalidState` if no live pending migration exists.
    pub(crate) fn get_pending_client_migration_impl(
        env: &Env,
        contract_id: u32,
    ) -> PendingClientMigration {
        let key = Self::pending_migration_key(contract_id);
        // Bump TTL first: the caller is actively inspecting the record, so
        // keep it alive for the full remaining window.
        extend_if_below_threshold(
            env,
            &key,
            PENDING_MIGRATION_BUMP_THRESHOLD,
            PENDING_MIGRATION_TTL_LEDGERS,
        );
        read_if_live(env, &key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidState))
    }
}

#[cfg(test)]
#[path = "migration_test.rs"]
mod migration_test;