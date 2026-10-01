//! Ledger-scoped contract-creation guard.
//!
//! Prevents concurrent or replayed `create_contract` invocations within the
//! same ledger sequence by recording the ledger at which creation began.  A
//! second invocation in the same ledger observes the in-progress marker and
//! returns `ConcurrentMutation` instead of racing on `NextContractId`.

use crate::types::{DataKey, Error};
use soroban_sdk::Env;

/// Acquire the creation guard for the current ledger.
///
/// Writes the current ledger sequence under [`DataKey::ContractCreationGuard`].
/// Returns `Err(ConcurrentMutation)` if the guard is already held for this
/// ledger (i.e., creation is already in progress).
///
/// On a different ledger the old marker is stale and creation proceeds normally.
pub(crate) fn begin(env: &Env) -> Result<(), Error> {
    let current_ledger = env.ledger().sequence();
    let key = DataKey::ContractCreationGuard;

    if let Some(guard_ledger) = env
        .storage()
        .temporary()
        .get::<_, u32>(&key)
    {
        if guard_ledger == current_ledger {
            return Err(Error::ConcurrentMutation);
        }
    }

    // Record the current ledger; TTL of 1 ledger is sufficient — the guard
    // only needs to last within the current transaction/ledger window.
    env.storage().temporary().set(&key, &current_ledger);
    Ok(())
}

/// Release the creation guard.
///
/// Removes the ledger-scoped creation marker.  Must be called on every success
/// path after the new contract and its milestones have been fully persisted.
pub(crate) fn end(env: &Env) {
    env.storage()
        .temporary()
        .remove(&DataKey::ContractCreationGuard);
}
