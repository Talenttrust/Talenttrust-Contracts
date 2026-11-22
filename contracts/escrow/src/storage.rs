//! Storage and TTL helpers.
//! Public re-exported from the parent `lib.rs` module.

use soroban_sdk::Env;

/// Extends a persistent entry's TTL if it exists.
pub fn extend_if_present(env: &Env, key: &types::DataKey) -> bool {
    if env.storage().persistent().has(key) {
        env.storage().persistent().extend_ttl(key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        true
    } else {
        false
    }
}

/// Bumps the instance TTL.
pub fn bump_instance(env: &Env) {
    env.storage().instance().extend_ttl(constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
}

/// Loads an escrow: the versioned `ContractV` entry first, falling back to the
/// legacy `Contract` key. Never writes; the next `save` performs the lazy migration.
pub fn load(env: &Env, id: u64) -> Result<types::EscrowContract, errors::EscrowError> {
    if let Some(stored) = env.storage().persistent().get::<_, types::StoredEscrow>(&types::DataKey::ContractV(id)) {
        return Ok(types::upgrade_to_latest(stored));
    }
    env.storage().persistent().get(&DataKey::Contract(id)).ok_or(errors::EscrowError::ContractNotFound)
}

/// Whether an escrow entry exists under either the versioned or the legacy key.
pub fn escrow_exists(env: &Env, id: u64) -> bool {
    env.storage().persistent().has(&types::DataKey::ContractV(id))
        || env.storage().persistent().has(&DataKey::Contract(id))
}

/// Writes the newest envelope variant under `ContractV` (TTL bumped on the new
/// key only) and drops any legacy entry: this is the lazy migration.
pub fn save(env: &Env, c: &types::EscrowContract) {
    let key = types::DataKey::ContractV(c.id);
    env.storage().persistent().set(&key, &types::StoredEscrow::V1(c.clone()));
    env.storage().persistent().extend_ttl(&key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
    let legacy = DataKey::Contract(c.id);
    if env.storage().persistent().has(&legacy) {
        env.storage().persistent().remove(&legacy);
    }
    bump_instance(env);
}

/// Extends a persistent entry's TTL if it exists.
pub fn extend_if_present(env: &Env, key: &types::DataKey) -> bool {
    if env.storage().persistent().has(key) {
        env.storage().persistent().extend_ttl(key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        true
    } else {
        false
    }
}

/// Bumps the instance TTL.
pub fn bump_instance(env: &Env) {
    env.storage().instance().extend_ttl(constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
}