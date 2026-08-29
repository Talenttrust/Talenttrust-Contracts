//! Storage Schema Versioning and Migration Engine for Escrow.
//!
//! Provides a safe, versioned, admin-guarded upgrade path for escrow contract storage.
//!
//! ## Invariants
//! - Layout versions are monotonically increasing (1 -> 2 -> ...).
//! - Upgrades are in-place, atomic, and idempotent.
//! - Downgrades or jumps beyond known versions are strictly rejected with typed errors.
//! - Admin authentication is required for all schema mutations.
//! - Emits `escrow_schema_migrated` event on successful version transition.

use crate::ttl::{PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS};
use crate::types::{DataKey, Error};
use crate::Escrow;
use soroban_sdk::{Address, Env, Symbol};

/// Baseline storage schema version for fresh deployments.
pub const INITIAL_STORAGE_SCHEMA_VERSION: u32 = 1;

/// Highest supported storage schema version implemented by this WASM build.
pub const CURRENT_STORAGE_SCHEMA_VERSION: u32 = 2;

impl Escrow {
    /// Read the current on-ledger storage schema version.
    ///
    /// If no schema version is stored (legacy state), returns `INITIAL_STORAGE_SCHEMA_VERSION` (1).
    pub(crate) fn get_schema_version_impl(env: &Env) -> u32 {
        let version: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SchemaVersion)
            .unwrap_or(INITIAL_STORAGE_SCHEMA_VERSION);

        env.storage().persistent().extend_ttl(
            &DataKey::SchemaVersion,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_TTL_LEDGERS,
        );

        version
    }

    /// Internal setter for the storage schema version with persistent TTL bump.
    pub(crate) fn set_schema_version_impl(env: &Env, version: u32) {
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion, &version);

        env.storage().persistent().extend_ttl(
            &DataKey::SchemaVersion,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_TTL_LEDGERS,
        );
    }

    /// Execute storage schema upgrade from current version to `target_version`.
    ///
    /// # Access Control
    /// - Requires admin signature (`admin.require_auth()`).
    /// - Caller must match stored contract admin.
    ///
    /// # Error Semantics
    /// - `Error::InvalidMigrationVersion`: `target_version` is 0, exceeds current WASM support, or attempts a downgrade.
    pub(crate) fn migrate_escrow_storage_impl(
        env: &Env,
        admin: Address,
        target_version: u32,
    ) -> Result<u32, Error> {
        Self::require_initialized(env);
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));

        admin.require_auth();
        if admin != stored_admin {
            return Err(Error::UnauthorizedRole);
        }

        let current_version = Self::get_schema_version_impl(env);

        // Idempotency: if already at target_version, return Ok without error
        if current_version == target_version {
            return Ok(current_version);
        }

        // Reject downgrades
        if target_version < current_version {
            return Err(Error::InvalidMigrationVersion);
        }

        // Reject targets beyond supported WASM version
        if target_version > CURRENT_STORAGE_SCHEMA_VERSION {
            return Err(Error::InvalidMigrationVersion);
        }

        // Execute step-by-step sequential migrations
        let mut running_version = current_version;

        if running_version == 1 && target_version >= 2 {
            // v1 -> v2 migration logic: establish explicit schema version marker and bump persistent TTL
            running_version = 2;
        }

        // Persist final version
        Self::set_schema_version_impl(env, running_version);

        // Emit migration event: topics = ("escrow_schema_migrated", current_version), data = (running_version, admin, timestamp)
        env.events().publish(
            (
                Symbol::new(env, "escrow_schema_migrated"),
                current_version,
            ),
            (
                running_version,
                admin,
                env.ledger().timestamp(),
            ),
        );

        Ok(running_version)
    }
}
