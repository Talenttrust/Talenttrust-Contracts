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
//! ## Retry / replay invariant (idempotent + atomic)
//!
//! Migration is **idempotent** and **all-or-nothing** under retry, replay, and
//! interleaved concurrent calls:
//!
//! * **Exactly one apply**: the first call that observes a v1 record performs
//!   the upgrade and returns `true`; every replay returns `false` and performs
//!   no further writes. Reputation counters are copied verbatim, never summed
//!   or incremented, so replay can never double-count (duplicate legacy source
//!   markers for the same address are likewise irrelevant — migration is keyed
//!   per address, not per index entry).
//! * **Atomic outcome**: a call either writes both the refreshed record and the
//!   current version marker (returning `true` only after re-reading the marker
//!   to confirm the seal), or it returns before writing anything. There is no
//!   successful return that leaves a record without a current marker.
//! * **Partial states heal, never regress**: if an interrupted run leaves the
//!   record rewritten but the marker unwritten (absent/legacy/`0` marker), a
//!   retry completes the migration; an orphan marker without a record is a
//!   deterministic no-op that never fabricates a record.
//! * **Reads are pure**: `get_reputation` and friends never write, so
//!   simultaneous RPC simulations cannot corrupt migration state. The contract
//!   is single-threaded (no `thread`/`Mutex`), so "concurrency" reduces to
//!   interleaved replay within one ledger and each address converges
//!   independently.
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
//! * Corrupted/unexpected markers (`0`, out-of-range) never panic and never
//!   regress state: with a record they heal forward to v2, without a record
//!   they no-op with zero writes.
//! * All query paths are side-effect free for reputation data and markers;
//!   explicit `migrate_reputation_storage` and `issue_reputation` own writes.
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
use soroban_sdk::{Address, Env};

// ── Version helpers ──────────────────────────────────────────────────────────

/// Read the stored schema version for `address`.
/// Returns `1` when the version key is absent (pre-versioning layout).
pub(crate) fn read_reputation_version(env: &Env, address: &Address) -> u32 {
    env.storage()
        .persistent()
        .get::<_, u32>(&DataKey::ReputationStorageVersion(address.clone()))
        .unwrap_or(1)
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
/// Returns `true` when an actual migration was performed, `false` when the
/// record was already at the current version (no-op) or when no record exists
/// for the address (nothing to migrate).
///
/// # Behaviour by version
///
/// * **v1 → v2 (record present)**: reads the existing [`Reputation`] value,
///   re-writes it to refresh its TTL, then writes the version marker. All
///   field values are preserved exactly.
/// * **v1 (no record)**: returns `false` immediately without touching storage.
///   An address with no reputation history has nothing to migrate.
/// * **v2 (current)**: returns `false` immediately; storage is untouched.
pub(crate) fn migrate_reputation_storage_impl(env: &Env, address: &Address) -> bool {
    let current_version = read_reputation_version(env, address);

    // Defensive version gate: never panic, never regress.
    // - Future markers (> CURRENT) are forward-compatible no-ops (no downgrade).
    // - Current markers are no-ops.
    // - Anything older (absent→1, `0`, `1`) falls through to heal/migrate.
    // - Corrupted markers decode via `unwrap_or(1)` to the legacy path, which
    //   heals forward when a record exists and no-ops with zero writes when
    //   absent.
    if current_version > REPUTATION_STORAGE_VERSION {
        // Future schema — leave untouched for a newer build to handle.
        return false;
    }
    if current_version == REPUTATION_STORAGE_VERSION {
        // Already at current version — nothing to do.
        return false;
    }

    // v1 → v2: preserve the existing reputation record, then write the marker.
    //
    // If there is no reputation record for this address at all, there is nothing
    // to migrate — return false and leave storage completely untouched.
    let rep_key = DataKey::Reputation(address.clone());
    let rep: Reputation = match env.storage().persistent().get(&rep_key) {
        Some(r) => r,
        None => return false,
    };

    // Re-write the reputation record to refresh its TTL alongside the version marker.
    env.storage().persistent().set(&rep_key, &rep);
    env.storage().persistent().extend_ttl(
        &rep_key,
        PERSISTENT_BUMP_THRESHOLD,
        PERSISTENT_TTL_LEDGERS,
    );

    write_reputation_version(env, address);

    // Verify the seal so callers get a deterministic result: `true` only when
    // the marker is confirmed at CURRENT. A failed seal reports `false` so a
    // retry stays safe instead of claiming success.
    read_reputation_version(env, address) == REPUTATION_STORAGE_VERSION
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

    env.storage()
        .persistent()
        .get(&DataKey::Reputation(address.clone()))
}
