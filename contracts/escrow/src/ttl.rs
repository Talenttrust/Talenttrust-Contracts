/// TTL (Time To Live) constants for temporary storage
///
/// These constants define the lifetime of approval records in temporary storage.
/// Expired approvals are automatically evicted and treated as absent.
/// Number of ledgers an approval remains valid before expiring
/// At ~5 seconds per ledger, this is approximately 7 days
pub const PENDING_APPROVAL_TTL_LEDGERS: u32 = 120_960;

/// Threshold at which to bump the TTL for an approval
/// Set to 50% of TTL to ensure approvals don't expire unexpectedly
pub const PENDING_APPROVAL_BUMP_THRESHOLD: u32 = 60_480;

/// Minimum TTL for approval records (1 day worth of ledgers)
#[allow(dead_code)]
pub const MIN_APPROVAL_TTL: u32 = 17_280;

// ============================================================================
// PERSISTENT STORAGE TTL (for contracts, milestones, reputation, fees)
// ============================================================================

/// Maximum TTL for persistent entries (approximately 1 year)
/// At ~5 seconds per ledger, this is 365 days
/// This ensures long-running contracts remain accessible
pub const PERSISTENT_MAX_TTL_LEDGERS: u32 = 6_307_200;

/// Threshold at which to bump persistent entry TTL
/// Set to 180 days to ensure contracts are extended well before expiry
/// Any read/write within 180 days of expiry will extend to full year
pub const PERSISTENT_BUMP_THRESHOLD: u32 = 3_110_400;

/// Minimum persistent TTL (30 days worth of ledgers)
/// Used as a safety floor for critical contract data
pub const MIN_PERSISTENT_TTL: u32 = 518_400;

// ============================================================================
// TTL EXTENSION HELPERS
// ============================================================================

/// Extends TTL for a persistent contract entry.
/// 
/// Call this on every read or write to DataKey::Contract(id) to ensure
/// active contracts never expire and lose fund-state.
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// 
/// # Policy
/// - Extends to PERSISTENT_MAX_TTL_LEDGERS (1 year)
/// - Bumps when within PERSISTENT_BUMP_THRESHOLD (180 days) of expiry
/// - Ensures any contract touched within its TTL window remains live
/// 
/// # Security
/// - Prevents persistent-entry eviction for active escrows
/// - Protects fund-state accounting from storage loss
/// - Deterministic: same policy for all contract accesses
pub fn extend_contract_ttl(env: &Env, contract_id: u32) {
    let key = DataKey::Contract(contract_id);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_MAX_TTL_LEDGERS);
}

/// Extends TTL for milestone data associated with a contract.
/// 
/// Call this whenever milestones are read or written to prevent
/// milestone-state loss.
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// 
/// # Policy
/// - Same as contract TTL policy for consistency
/// - Milestones and contract data have synchronized lifetimes
pub fn extend_milestone_ttl(env: &Env, contract_id: u32) {
    let milestone_key = Symbol::new(env, "milestones");
    let key = (DataKey::Contract(contract_id), milestone_key);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_MAX_TTL_LEDGERS);
}

/// Extends TTL for the NextContractId counter.
/// 
/// This counter must never be lost as it ensures unique contract IDs.
/// 
/// # Arguments
/// * `env` - The contract environment
pub fn extend_next_contract_id_ttl(env: &Env) {
    let key = DataKey::NextContractId;
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_MAX_TTL_LEDGERS);
}

/// Extends TTL for all persistent entries related to a contract.
/// 
/// Convenience function that extends TTL for both contract and milestone data.
/// Use this when performing operations that touch multiple persistent entries.
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// 
/// # Usage
/// Call after any mutating operation (create, deposit, release, refund)
/// to ensure all related persistent data remains live.
pub fn extend_contract_and_milestones_ttl(env: &Env, contract_id: u32) {
    extend_contract_ttl(env, contract_id);
    extend_milestone_ttl(env, contract_id);
}
