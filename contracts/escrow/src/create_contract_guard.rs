//! Ledger-scoped guard preventing contract ID allocation races.

use crate::Error;
use soroban_sdk::{symbol_short, Env};

/// Reject a second contract-creation attempt in the same ledger. A stale guard
/// from a prior ledger is overwritten, so failures cannot block creation forever.
pub(crate) fn begin(env: &Env) -> Result<(), Error> {
    let ledger = env.ledger().sequence();
    let key = (symbol_short!("ct_guard"),);
    if env
        .storage()
        .persistent()
        .get::<_, u32>(&key)
        .is_some_and(|guard_ledger| guard_ledger == ledger)
    {
        return Err(Error::ContractCreationInProgress);
    }

    env.storage().persistent().set(&key, &ledger);
    Ok(())
}

/// Release the guard after a successful contract creation.
pub(crate) fn end(env: &Env) {
    env.storage()
        .persistent()
        .remove(&(symbol_short!("ct_guard"),));
}
