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
//!
//! ## Hardened admin-triggered per-record upgrade
//!
//! An additional admin-gated entrypoint (`upgrade_contract_schema_impl`)
//! performs the same migration but wraps it in an explicit atomicity protocol
//! with event observability:
//!
//! 1. Admin authentication and `require_auth` are enforced before any storage
//!    mutation.
//! 2. A `ContractMigrationLock(contract_id)` marker is written, recording the
//!    target version.  A concurrent invocation observing the marker fails with
//!    `MigrationAlreadyInProgress` rather than racing.
//! 3. A `contract_schema_migration_started` event is emitted, identifying the
//!    record, the current version, and the target version.
//! 4. Legacy data is fully decoded and validated **before** any key is
//!    rewritten.  A deserialisation failure surfaces as `CorruptedStorageData`
//!    without partial writes.
//! 5. The migrated payload is written, then the version marker, then both
//!    reads are re-checked in a **verification seal**.  Any mismatch here
//!    surfaces as `MigrationVersionMismatch` — the host-atomic transaction
//!    rolls back every intermediate write.
//! 6. The lock is removed and a `contract_schema_migrated` event is emitted
//!    carrying the final confirmed version.
//! 7. Idempotency: if `current_version == target_version` the call returns
//!    `AlreadyMigrated` immediately without touching any storage key.

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
/// `load_contract_migrated` and `upgrade_contract_schema_version_step`.
pub const CONTRACT_STORAGE_SCHEMA_VERSION: u32 = 2;

/// Lowest legacy version recognised by the migration engine.  Records whose
/// on-ledger marker is below this (or absent) are always run through the
/// legacy decode path first.
const MIN_SUPPORTED_CONTRACT_SCHEMA_VERSION: u32 = 1;

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

    /// Per-contract schema-migration lock key.  Presence of this key indicates
    /// an admin-triggered upgrade is in flight for `contract_id`; the stored
    /// value is the `u32` target version so a recovery caller can distinguish
    /// a stale in-progress lock from a freshly acquired one.
    pub(crate) fn contract_migration_lock_key(contract_id: u32) -> DataKey {
        DataKey::ContractMigrationLock(contract_id)
    }

    // ── Admin + auth helpers ─────────────────────────────────────────────────

    /// Load and return the stored admin address, panicking with
    /// `NotInitialized` if the contract has not been initialised.
    pub(crate) fn load_stored_admin(env: &Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized))
    }

    /// Enforce that `caller` both matches the stored admin and has produced a
    /// valid authorisation signature for this invocation.
    ///
    /// Ordering (matches the rest of the codebase): identity check first, then
    /// `require_auth` so a non-admin caller cannot even trigger the auth
    /// probe against a signer it does not control.
    pub(crate) fn require_admin_auth(env: &Env, caller: &Address) {
        let stored_admin = Self::load_stored_admin(env);
        if *caller != stored_admin {
            env.panic_with_error(Error::UnauthorizedRole);
        }
        caller.require_auth();
    }

    /// Reject a client migration whose proposed address equals the existing
    /// client address.  A no-op rotation would be misleading and would waste
    /// a pending-migration TTL slot covering nothing.
    pub(crate) fn require_distinct_client(env: &Env, current: &Address, proposed: &Address) {
        if current == proposed {
            env.panic_with_error(EscrowError::RoleOverlap);
        }
    }

    // ── Contract loading (migrate-on-read) ───────────────────────────────────

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

    /// Read the per-contract schema version marker for `contract_id`.
    ///
    /// Returns `MIN_SUPPORTED_CONTRACT_SCHEMA_VERSION` (1) when the marker is
    /// absent — the legacy, pre-versioning value.  This is a pure read; no
    /// storage mutation or TTL bump is performed so the helper is safe inside
    /// idempotency checks.
    pub(crate) fn read_contract_schema_version(env: &Env, contract_id: u32) -> u32 {
        env.storage()
            .persistent()
            .get::<_, u32>(&DataKey::ContractSchemaVersion(contract_id))
            .unwrap_or(MIN_SUPPORTED_CONTRACT_SCHEMA_VERSION)
    }

    /// Decode a legacy `ContractV1` record from its storage key and
    /// deterministically upgrade it to the current [`Contract`] layout.
    ///
    /// Does **not** write anything to storage.  Panics with
    /// `CorruptedStorageData` if the raw bytes under the legacy key cannot be
    /// deserialised; this preserves the no-partial-writes invariant of the
    /// migration engine.
    pub(crate) fn decode_and_upgrade_contract_v1(
        env: &Env,
        contract_id: u32,
    ) -> Result<Contract, Error> {
        let v1: ContractV1 = match env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
        {
            Some(record) => record,
            None => return Err(Error::ContractNotFound),
        };

        let non_negative = v1.total_deposited >= 0
            && v1.funded_amount >= 0
            && v1.released_amount >= 0
            && v1.refunded_amount >= 0;
        if !non_negative {
            return Err(Error::CorruptedStorageData);
        }
        let payout_sum = v1
            .released_amount
            .checked_add(v1.refunded_amount)
            .ok_or(Error::CorruptedStorageData)?;
        if payout_sum > v1.funded_amount {
            return Err(Error::CorruptedStorageData);
        }

        Ok(Contract {
            client: v1.client,
            freelancer: v1.freelancer,
            arbiter: v1.arbiter,
            status: v1.status,
            total_deposited: v1.total_deposited,
            funded_amount: v1.funded_amount,
            released_amount: v1.released_amount,
            refunded_amount: v1.refunded_amount,
            release_authorization: v1.release_authorization,
            reputation_issued: false,
        })
    }

    /// Apply the version-bump writes for a successfully decoded upgrade.
    ///
    /// Preconditions:
    /// * `upgraded` has already been validated by the decode helper (or by
    ///   having been loaded as a current-version record).
    ///
    /// Postconditions (both writes succeed or the host rolls back both):
    /// * `DataKey::Contract(contract_id)` holds `upgraded` at TTL bumped.
    /// * `DataKey::ContractSchemaVersion(contract_id)` holds
    ///   `CONTRACT_STORAGE_SCHEMA_VERSION` at TTL bumped.
    fn commit_upgraded_contract(env: &Env, contract_id: u32, upgraded: &Contract) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), upgraded);
        env.storage().persistent().set(
            &DataKey::ContractSchemaVersion(contract_id),
            &CONTRACT_STORAGE_SCHEMA_VERSION,
        );

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
    }

    /// Verification seal: re-read the payload and the version marker and
    /// assert they match the expected post-migration state.  Any mismatch
    /// surfaces a typed error rather than letting a torn write go silent.
    ///
    /// This is a read-only step — no storage is mutated.  In a healthy,
    /// host-atomic transaction it always passes; its purpose is to fail-closed
    /// if a future host change or buggy concurrent write somehow interleaves.
    fn seal_contract_upgrade(env: &Env, contract_id: u32, expected: &Contract) -> Result<(), Error> {
        let sealed_version = Self::read_contract_schema_version(env, contract_id);
        if sealed_version != CONTRACT_STORAGE_SCHEMA_VERSION {
            return Err(Error::MigrationVersionMismatch);
        }

        let sealed_record: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .ok_or(Error::InvalidMigrationState)?;

        if sealed_record != *expected {
            return Err(Error::MigrationVersionMismatch);
        }

        Ok(())
    }

    /// Load a contract and transparently upgrade it from `ContractV1` to the
    /// current layout when the per-record schema version marker is absent or < 2.
    ///
    /// On a first-time read of a legacy record the upgraded `Contract` and its
    /// version marker are written back so subsequent reads take the fast path.
    /// The write is ordered **after full validation**: a corrupted legacy
    /// record cannot leave partially-migrated keys in storage.
    ///
    /// # Panics
    /// `ContractNotFound` if no record exists at all (not even a v1 record).
    pub(crate) fn load_contract_migrated(env: &Env, contract_id: u32) -> Contract {
        let version = Self::read_contract_schema_version(env, contract_id);

        // Fast path: record is already at the current version.  Skip every
        // write path and return the canonical deserialisation.
        if version >= CONTRACT_STORAGE_SCHEMA_VERSION {
            let current: Contract = env
                .storage()
                .persistent()
                .get::<_, Contract>(&DataKey::Contract(contract_id))
                .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
            return current;
        }

        // Slow path: fully decode the legacy record and validate its
        // accounting invariants before touching any write path.  A failure
        // here surfaces `CorruptedStorageData` via panic with zero storage
        // mutations — no partial, no stale keys.
        let upgraded = match Self::decode_and_upgrade_contract_v1(env, contract_id) {
            Ok(u) => u,
            Err(e) => env.panic_with_error(e),
        };

        // Commit payload + version marker together (host-atomic), then
        // re-read and seal so the caller can only observe a record whose
        // marker and payload agree.
        Self::commit_upgraded_contract(env, contract_id, &upgraded);
        if let Err(seal_err) = Self::seal_contract_upgrade(env, contract_id, &upgraded) {
            env.panic_with_error(seal_err);
        }

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

    // ── Admin-gated per-contract upgrade ─────────────────────────────────────

    /// Acquire the per-contract schema-migration lock for `contract_id`.
    ///
    /// Fails with `MigrationAlreadyInProgress` if a lock is already present.
    /// On success the stored value is `target_version` so a recovery caller can
    /// observe which version was being applied before a mid-migration panic.
    fn acquire_contract_migration_lock(
        env: &Env,
        contract_id: u32,
        target_version: u32,
    ) -> Result<(), Error> {
        let lock_key = Self::contract_migration_lock_key(contract_id);
        if env.storage().persistent().has(&lock_key) {
            return Err(Error::MigrationAlreadyInProgress);
        }
        env.storage()
            .persistent()
            .set(&lock_key, &target_version);
        Ok(())
    }

    /// Release the per-contract schema-migration lock.
    ///
    /// Idempotent: safe to call after a successful migration, from a
    /// panic-catcher, or from an admin recovery tool.  Never panics.
    fn release_contract_migration_lock(env: &Env, contract_id: u32) {
        env.storage()
            .persistent()
            .remove(&Self::contract_migration_lock_key(contract_id));
    }

    /// Execute a single schema-version step for `contract_id`.
    ///
    /// Today the only supported step is `1 → 2` (upgrade to the
    /// reputation_issued-aware layout).  When a new schema version is added,
    /// extend this match with an additional arm that decodes the prior layout
    /// and returns the new struct.
    ///
    /// The step function returns the upgraded contract WITHOUT writing it;
    /// callers own the commit+seal+lock lifecycle so every transition can be
    /// uniformly wrapped in the atomicity protocol.
    fn upgrade_contract_schema_version_step(
        env: &Env,
        contract_id: u32,
        current_version: u32,
    ) -> Result<Contract, Error> {
        match current_version {
            1 => Self::decode_and_upgrade_contract_v1(env, contract_id),
            // Already handled as a no-op by the idempotency gate above; this
            // arm is a defensive belt-and-suspenders check.
            v if v == CONTRACT_STORAGE_SCHEMA_VERSION => {
                let current: Contract = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Contract(contract_id))
                    .ok_or(Error::ContractNotFound)?;
                Ok(current)
            }
            _ => Err(Error::InvalidMigrationVersion),
        }
    }

    /// Admin-triggered per-contract schema upgrade.
    ///
    /// # Atomicity
    /// Wraps the decode → upgrade → commit → seal pipeline in:
    /// 1. A `ContractMigrationLock` preventing concurrent admin calls for the
    ///    same `contract_id`.
    /// 2. Explicit start / success / failure Soroban events with non-sensitive
    ///    metadata only (no payload bytes in topics or data).
    /// 3. A verification seal that re-reads both the migrated payload and the
    ///    version marker before declaring victory.
    ///
    /// # Idempotency
    /// If the record is already at `target_version` the call returns
    /// `AlreadyMigrated` without writing any key or emitting any event.
    ///
    /// # Errors
    /// * `NotInitialized` — contract has no stored admin.
    /// * `UnauthorizedRole` — `caller` is not the stored admin, or did not
    ///   produce a valid authorisation.
    /// * `ContractPaused` / `EmergencyActive` — system is halted.
    /// * `InvalidContractId` — `contract_id` fails bounds validation.
    /// * `MigrationAlreadyInProgress` — another invocation is (or was) in
    ///   flight for this `contract_id`; the caller should retry after running
    ///   the key-recovery flow.
    /// * `InvalidMigrationVersion` — `target_version` is outside the supported
    ///   range, is a downgrade, or requires an unimplemented step.
    /// * `AlreadyMigrated` — the on-ledger marker already equals
    ///   `target_version`.
    /// * `CorruptedStorageData` — the legacy record cannot be deserialised or
    ///   fails accounting invariants.
    /// * `MigrationVersionMismatch` — the verification seal disagrees with the
    ///   expected post-migration state.
    pub(crate) fn upgrade_contract_schema_impl(
        env: &Env,
        caller: Address,
        contract_id: u32,
        target_version: u32,
    ) -> Result<u32, Error> {
        Self::require_initialized(env);
        Self::require_not_paused(env);
        storage::validate_contract_id_bounds(env, contract_id);

        // 1. Admin authorisation — enforced before any state mutation.
        Self::require_admin_auth(env, &caller);

        // 2. Supported-version guard.  Explicit rather than implicit so a
        //    bump of `CONTRACT_STORAGE_SCHEMA_VERSION` without a matching
        //    step function fails fast.
        if target_version < MIN_SUPPORTED_CONTRACT_SCHEMA_VERSION
            || target_version > CONTRACT_STORAGE_SCHEMA_VERSION
        {
            return Err(Error::InvalidMigrationVersion);
        }

        let current_version = Self::read_contract_schema_version(env, contract_id);

        // 3. Idempotency — already at target; return the explicit typed error
        //    so callers can distinguish "nothing to do" from "migration ran".
        if current_version == target_version {
            return Err(Error::AlreadyMigrated);
        }

        // 4. Monotonicity — downgrades are strictly forbidden.
        if target_version < current_version {
            return Err(Error::InvalidMigrationVersion);
        }

        // Ensure the contract record actually exists.  A missing record at
        // this stage means the caller supplied a valid-but-unused contract_id;
        // fail deterministically rather than writing a version marker for a
        // non-existent payload.
        if !env
            .storage()
            .persistent()
            .has(&DataKey::Contract(contract_id))
        {
            return Err(Error::ContractNotFound);
        }

        // 5. Acquire the migration lock.  If the acquisition fails we return
        //    `MigrationAlreadyInProgress` before emitting any event; a caller
        //    can clear the stale lock via the key-recovery entrypoint.
        Self::acquire_contract_migration_lock(env, contract_id, target_version)?;

        // 6. Emit the start event.  Topics identify the record and the
        //    transition; data carries the admin address for auditability and
        //    the current ledger timestamp.
        env.events().publish(
            (
                Symbol::new(env, "contract_schema_mig_start"),
                contract_id,
                current_version,
                target_version,
            ),
            (caller.clone(), env.ledger().timestamp()),
        );

        // 7. Execute the step function.  This performs zero writes, so a
        //    failure here leaves the lock set but no partial keys.  We emit
        //    the failure event and then clear the lock on the way out.
        let upgraded = match Self::upgrade_contract_schema_version_step(
            env,
            contract_id,
            current_version,
        ) {
            Ok(u) => u,
            Err(step_err) => {
                env.events().publish(
                    (
                        Symbol::new(env, "contract_schema_mig_failed"),
                        contract_id,
                        current_version,
                        target_version,
                    ),
                    (
                        caller.clone(),
                        Into::<u32>::into(step_err),
                        env.ledger().timestamp(),
                    ),
                );
                Self::release_contract_migration_lock(env, contract_id);
                return Err(step_err);
            }
        };

        // 8. Commit writes, then re-seal.  Either failure here leaves the
        //    lock set; the host-atomic rollback will also undo the commit,
        //    but the lock + failure event remain for operator visibility.
        Self::commit_upgraded_contract(env, contract_id, &upgraded);
        if let Err(seal_err) = Self::seal_contract_upgrade(env, contract_id, &upgraded) {
            env.events().publish(
                (
                    Symbol::new(env, "contract_schema_mig_failed"),
                    contract_id,
                    current_version,
                    target_version,
                ),
                (
                    caller.clone(),
                    Into::<u32>::into(seal_err),
                    env.ledger().timestamp(),
                ),
            );
            Self::release_contract_migration_lock(env, contract_id);
            return Err(seal_err);
        }

        // 9. Success path: clear the lock and emit the completion event.
        Self::release_contract_migration_lock(env, contract_id);
        env.events().publish(
            (
                Symbol::new(env, "contract_schema_migrated"),
                contract_id,
                current_version,
            ),
            (
                target_version,
                caller.clone(),
                env.ledger().timestamp(),
            ),
        );

        Ok(target_version)
    }

    /// Return the current per-contract schema version marker for
    /// `contract_id`.  Read-only: never mutates storage.
    ///
    /// Useful for off-chain operators to enumerate which records still need
    /// the admin-gated upgrade before a WASM bump drops support for the v1
    /// decode path.
    pub(crate) fn get_contract_schema_version_impl(env: &Env, contract_id: u32) -> u32 {
        Self::read_contract_schema_version(env, contract_id)
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
        if matches(
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
    pub(crate) fn require_no_role_overlap(
        env: &Env,
        contract: &Contract,
        candidate: &Address,
    ) {
        if *candidate == contract.client
            || *candidate == contract.freelancer
            || contract.arbiter.as_ref() == Some(candidate)
            || *candidate == env.current_contract_address()
        {
            env.panic_with_error(EscrowError::RoleOverlap);
        }
    }

    // ── Mutating entrypoints (client role migration) ─────────────────────────

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

        let contract = Self::load_contract_migrated(env, contract_id);
        // 4. finalization guard
        Self::require_not_finalized(env, contract_id);
        // 5. identity check
        if current_client != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        // 6. distinct-check (prevent self-migration wasting a TTL slot)
        Self::require_distinct_client(env, &contract.client, &new_client);
        // 7. role-overlap guard
        Self::require_no_role_overlap(env, &contract, &new_client);
        // 8. terminal-status guard
        Self::require_migration_allowed(env, contract.status);
        // 9. duplicate guard
        if Self::pending_migration_exists(env, contract_id) {
            env.panic_with_error(EscrowError::InvalidState);
        }
        Self::require_no_role_overlap(env, &contract, &new_client);

        let requested_at = env.ledger.sequence();
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

        let mut contract = Self::load_contract_migrated(env, contract_id);
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
        Self::require_distinct_client(env, &contract.client, &new_client);

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

        let contract = Self::load_contract_migrated(env, contract_id);
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
