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
//! * **Migration-on-read** ([`read_reputation_with_migration`]): called from
//!   `get_reputation` so every read transparently upgrades legacy records.
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
fn write_reputation_version(env: &Env, address: &Address) {
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

    if current_version >= REPUTATION_STORAGE_VERSION {
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

    true
}

// ── Migration-on-read ────────────────────────────────────────────────────────

/// Read the [`Reputation`] for `address`, transparently migrating a legacy v1
/// record to v2 before returning it.
///
/// Returns `None` when no reputation record exists (neither v1 nor v2). The
/// migration step is a no-op for absent records, so `None` is returned cleanly.
///
/// This is the canonical read path used by `get_reputation` so callers always
/// observe up-to-date versioned records without needing an explicit migration
/// call.
pub(crate) fn read_reputation_with_migration(env: &Env, address: &Address) -> Option<Reputation> {
    // Attempt a silent migration first; this is a no-op for current-version
    // records and also a no-op for absent records.
    migrate_reputation_storage_impl(env, address);

    env.storage()
        .persistent()
        .get(&DataKey::Reputation(address.clone()))
}
