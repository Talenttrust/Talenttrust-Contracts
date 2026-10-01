//! Versioned migration path for reputation storage.
//!
//! ## Storage schema versions
//!
//! | Version | Key written | Description |
//! |---------|-------------|-------------|
//! | v1 (absent) | — | Original layout. Only [`DataKey::Reputation(address)`] is present. No version marker is stored. This is the "legacy" state: any address whose [`DataKey::ReputationStorageVersion`] is missing is considered v1. |
//! | v2 (current) | [`DataKey::ReputationStorageVersion(address)`] = `2` | Same [`Reputation`] struct, but a version marker is written alongside it. The marker allows future migrations to distinguish "freshly written by a v2-aware build" from "written before versioning existed". |
//!
//! ## Migration semantics
//!
//! * **No-op for current version**: if the version marker already equals
//!   [`REPUTATION_STORAGE_VERSION`] (`2`), `migrate_reputation_storage_impl`
//!   returns `false` immediately without touching storage.
//! * **No-op when absent**: if no reputation record exists for the address,
//!   there is nothing to migrate — returns `false` and leaves storage untouched.
//! * **v1 → v2**: reads the existing [`Reputation`] value, re-writes it to
//!   refresh its TTL, then writes the version marker. All field values are
//!   preserved exactly.
//! * **Getters stay read-only**: `get_reputation` never mutates storage (RPC
//!   simulation safety). State writes belong exclusively in
//!   `migrate_reputation_storage` and `issue_reputation`. Use
//!   [`read_reputation_with_migration`] only as an opt-in helper, never inside
//!   getters.
//!
//! ## State invariants (protected)
//!
//! * Absent record → `false`, zero storage writes (no record, no marker).
//! * Already-current or future version (`>= REPUTATION_STORAGE_VERSION`) →
//!   `false`, storage untouched.
//! * v1 with record → `true`, fields preserved exactly, both keys TTL-bumped.
//! * Retries / concurrent calls are idempotent: first call migrates, rest no-op.
//! * Permissionless: no auth required; migration never escalates privilege and
//!   never deletes or alters reputation field values.
//!
//! ## Compatibility contract (preserved)
//!
//! * v1 records (marker absent, `0`, or `1`) stay readable through
//!   `get_reputation`, `get_average_rating`, and `get_reputations_page` with
//!   identical results before and after migration (except the marker itself).
//! * Empty/absent data → `None` (single reads) or empty `Vec` (pages), never a
//!   panic or host trap.
//! * Future markers (`> REPUTATION_STORAGE_VERSION`) are forward-compatible
//!   no-ops: data untouched, `false` returned, no downgrade.
//! * Corrupted/unexpected markers (`0`, out-of-range, or a value that does not
//!   decode as `u32`) never panic and never regress state: with a record they
//!   heal forward to v2, without a record they no-op with zero writes. Markers
//!   are read as a raw `Val` and decoded with `try_from_val`, so a wrongly
//!   typed marker can never trap the transaction.
//! * Undecodable records: a `DataKey::Reputation` value that does not decode as
//!   [`Reputation`] is left byte-for-byte untouched (no re-write, no marker) and
//!   reported as [`MigrationOutcome::CorruptRecord`]. Overwriting it would
//!   destroy the only copy of the data; trapping would make the address
//!   permanently unmigratable. Re-issuing reputation repairs it, after which a
//!   retry migrates normally.
//! * All query paths are side-effect free for reputation data and markers;
//!   explicit `migrate_reputation_storage` and `issue_reputation` own writes.
//!
//! ## Failure recovery and atomicity
//!
//! Every attempt is classified into exactly one [`MigrationOutcome`], computed
//! only from the stored marker and record, so the same storage always yields
//! the same outcome. Soroban invocations are atomic: if anything traps after
//! the record re-write, the host reverts the whole transaction, so a partially
//! migrated address (record bumped, marker missing) can never be persisted.
//! The marker is written last and read back before `Migrated` is reported.
//!
//! ## Observability
//!
//! Outcomes that change state or need operator attention emit one event with
//! topics `("rep_mig", <outcome>, address)` and data
//! `(from_version: u32, to_version: u32)`:
//!
//! | Outcome symbol | When | `from_version` |
//! |----------------|------|----------------|
//! | `migrated` | v1/corrupt marker healed to v2 | decoded marker, `1` if absent, `0` if undecodable |
//! | `future` | marker newer than this build; left untouched | stored marker |
//! | `corrupt` | record undecodable; left untouched | decoded marker, `1` if absent, `0` if undecodable |
//!
//! `AlreadyCurrent` and `NoRecord` are silent so idempotent retries stay
//! cheap. Events carry only the address and version numbers, never reputation
//! values.
//!
//! ## Append-only error codes
//!
//! No new `EscrowError` variants are required; the function returns `false`
//! for the no-op path and `true` for an actual migration, keeping the ABI
//! minimal.

use crate::{
    ttl::{PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS},
    DataKey, Reputation, REPUTATION_STORAGE_VERSION,
};
use soroban_sdk::{symbol_short, Address, Env, Symbol, TryFromVal, Val};

// ── Outcome model ────────────────────────────────────────────────────────────

/// Deterministic classification of one migration attempt.
///
/// The public `migrate_reputation_storage` entrypoint keeps its `bool` ABI
/// (`true` only for [`MigrationOutcome::Migrated`]); this enum gives callers
/// inside the crate and the emitted events the precise reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MigrationOutcome {
    /// A legacy (or corrupt-marker) record was re-written and the marker sealed
    /// at [`REPUTATION_STORAGE_VERSION`].
    Migrated,
    /// The marker already equals [`REPUTATION_STORAGE_VERSION`]; no writes.
    AlreadyCurrent,
    /// The marker is newer than this build understands; no writes, no downgrade.
    FutureVersion,
    /// No reputation record exists; nothing to migrate, no writes.
    NoRecord,
    /// A record exists but does not decode as [`Reputation`]; left untouched.
    CorruptRecord,
    /// The marker could not be confirmed at the current version after writing.
    /// Reported instead of `Migrated` so a caller never sees false success.
    SealFailed,
}

/// The version marker as stored, before any interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StoredVersion {
    /// No marker: the pre-versioning (v1) layout.
    Absent,
    /// A marker that decodes as `u32`.
    Valid(u32),
    /// A marker of the wrong type; treated as legacy so it can heal forward.
    Undecodable,
}

impl StoredVersion {
    /// Version number reported in events and used for the legacy gate.
    fn as_reported(self) -> u32 {
        match self {
            StoredVersion::Absent => 1,
            StoredVersion::Valid(v) => v,
            StoredVersion::Undecodable => 0,
        }
    }
}

fn read_stored_version(env: &Env, address: &Address) -> StoredVersion {
    let raw: Option<Val> = env
        .storage()
        .persistent()
        .get(&DataKey::ReputationStorageVersion(address.clone()));
    match raw {
        None => StoredVersion::Absent,
        Some(val) => match u32::try_from_val(env, &val) {
            Ok(v) => StoredVersion::Valid(v),
            Err(_) => StoredVersion::Undecodable,
        },
    }
}

fn emit_outcome(env: &Env, outcome: Symbol, address: &Address, from_version: u32) {
    env.events().publish(
        (symbol_short!("rep_mig"), outcome, address.clone()),
        (from_version, REPUTATION_STORAGE_VERSION),
    );
}

// ── Version helpers ──────────────────────────────────────────────────────────

/// Read the stored schema version for `address`.
///
/// Returns `1` when the version key is absent (pre-versioning layout) and `1`
/// when the stored marker does not decode as `u32`, so a corrupted marker is
/// treated as legacy and healed forward instead of trapping.
pub(crate) fn read_reputation_version(env: &Env, address: &Address) -> u32 {
    match read_stored_version(env, address) {
        StoredVersion::Valid(v) => v,
        StoredVersion::Absent | StoredVersion::Undecodable => 1,
    }
}

/// Persist the current schema version marker for `address` with the standard
/// persistent TTL, then bump it via the threshold policy.
///
/// Shared by the migration path and `issue_reputation` so fresh writes never
/// regress to marker-less v1.
pub(crate) fn write_reputation_version(env: &Env, address: &Address) {
    let key = DataKey::ReputationStorageVersion(address.clone());
    env.storage()
        .persistent()
        .set(&key, &REPUTATION_STORAGE_VERSION);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS);
}

// ── Core migration ───────────────────────────────────────────────────────────

/// Upgrade the reputation record for `address` from any older schema to the
/// current version.
///
/// Returns `true` only when an actual migration was performed and sealed; see
/// [`migrate_reputation_storage_outcome`] for the precise reason on `false`.
pub(crate) fn migrate_reputation_storage_impl(env: &Env, address: &Address) -> bool {
    migrate_reputation_storage_outcome(env, address) == MigrationOutcome::Migrated
}

/// Classify and, when appropriate, perform the migration for `address`.
///
/// # Behaviour by stored state
///
/// | Marker | Record | Outcome | Writes |
/// |--------|--------|---------|--------|
/// | `> CURRENT` | any | `FutureVersion` | none |
/// | `== CURRENT` | any | `AlreadyCurrent` | none |
/// | absent / `< CURRENT` / undecodable | absent | `NoRecord` | none |
/// | absent / `< CURRENT` / undecodable | undecodable | `CorruptRecord` | none |
/// | absent / `< CURRENT` / undecodable | valid | `Migrated` | record re-write + marker |
///
/// The function never panics on stored data, so retries and concurrent calls
/// converge: the first successful call migrates, every later call is
/// `AlreadyCurrent`.
pub(crate) fn migrate_reputation_storage_outcome(env: &Env, address: &Address) -> MigrationOutcome {
    let stored = read_stored_version(env, address);

    // Defensive version gate: never panic, never regress.
    if let StoredVersion::Valid(v) = stored {
        if v > REPUTATION_STORAGE_VERSION {
            // Future schema — leave untouched for a newer build to handle.
            emit_outcome(env, symbol_short!("future"), address, v);
            return MigrationOutcome::FutureVersion;
        }
        if v == REPUTATION_STORAGE_VERSION {
            return MigrationOutcome::AlreadyCurrent;
        }
    }

    // Legacy path (absent, `0`, `1`, or undecodable marker).
    let rep_key = DataKey::Reputation(address.clone());
    let raw: Option<Val> = env.storage().persistent().get(&rep_key);
    let Some(raw) = raw else {
        // No reputation history: nothing to migrate, zero writes.
        return MigrationOutcome::NoRecord;
    };
    let Ok(rep) = Reputation::try_from_val(env, &raw) else {
        // Never overwrite the only copy of the data and never trap: report it
        // so an operator can re-issue, after which a retry migrates normally.
        emit_outcome(env, symbol_short!("corrupt"), address, stored.as_reported());
        return MigrationOutcome::CorruptRecord;
    };

    // Re-write the reputation record to refresh its TTL alongside the version
    // marker. Field values are preserved exactly.
    env.storage().persistent().set(&rep_key, &rep);
    env.storage().persistent().extend_ttl(
        &rep_key,
        PERSISTENT_BUMP_THRESHOLD,
        PERSISTENT_TTL_LEDGERS,
    );

    // The marker is written last; the invocation is atomic, so a trap anywhere
    // above reverts the record re-write as well.
    write_reputation_version(env, address);

    // Verify the seal so callers get a deterministic result: `Migrated` only
    // when the marker is confirmed at CURRENT, so a retry stays safe instead of
    // a caller being told it succeeded.
    if read_stored_version(env, address) != StoredVersion::Valid(REPUTATION_STORAGE_VERSION) {
        return MigrationOutcome::SealFailed;
    }
    emit_outcome(
        env,
        symbol_short!("migrated"),
        address,
        stored.as_reported(),
    );
    MigrationOutcome::Migrated
}

// ── Migration-on-read (opt-in helper, NOT wired to getters) ──────────────────

/// Read the [`Reputation`] for `address`, transparently migrating a legacy v1
/// record to v2 before returning it.
///
/// Returns `None` when no reputation record exists (neither v1 nor v2). The
/// migration step is a no-op for absent records, so `None` is returned cleanly.
///
/// NOTE: getters (`get_reputation`) must stay read-only for RPC simulation
/// safety and do NOT call this helper. State writes belong exclusively in
/// `migrate_reputation_storage` and `issue_reputation`.
pub(crate) fn read_reputation_with_migration(env: &Env, address: &Address) -> Option<Reputation> {
    // Attempt a silent migration first; this is a no-op for current-version
    // records and also a no-op for absent records.
    migrate_reputation_storage_impl(env, address);

    // Decode defensively: an undecodable record reads as `None` rather than
    // trapping (it was left untouched and reported by the migration above).
    let raw: Option<Val> = env
        .storage()
        .persistent()
        .get(&DataKey::Reputation(address.clone()));
    raw.and_then(|val| Reputation::try_from_val(env, &val).ok())
}
