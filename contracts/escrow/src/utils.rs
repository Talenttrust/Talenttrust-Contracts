use soroban_sdk::{symbol_short, Env};

use crate::ContractStatus;

/// Returns the current ledger timestamp in seconds.
pub fn now_seconds(env: &Env) -> u64 {
    env.ledger().timestamp()
}

/// Emit a status-transition event for indexers.
pub fn emit_status_changed(
    env: &Env,
    contract_id: u32,
    old_status: ContractStatus,
    new_status: ContractStatus,
) {
    env.events().publish(
        (symbol_short!("status"), contract_id),
        (
            old_status as u32,
            new_status as u32,
            env.ledger().timestamp(),
        ),
    );
}
