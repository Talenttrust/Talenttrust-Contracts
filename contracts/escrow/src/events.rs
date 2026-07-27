use crate::types::{Contract, MilestoneIndexEvent};
use crate::EscrowError;
use soroban_sdk::{symbol_short, Env};

pub use crate::types::MilestoneIndexEvent;

/// Emits an indexed event on contract state changes to assist off-chain indexers
/// in cheaply reconstructing contract lifecycle history and financial balances.
///
/// # Event Specification
/// - **Topic**: `(symbol_short!("contract"), contract_id: u32)`
/// - **Payload**: `(status: u32, funded_amount: i128, released_amount: i128, refunded_amount: i128, total_deposited: i128)`
///
/// # Panics
/// - `InvalidContractId` if `contract_id` is zero.
/// - `AmountMustBePositive` if any amount field is negative.
pub fn emit_contract_indexed_event(env: &Env, contract_id: u32, contract: &Contract) {
    if contract_id == 0 {
        env.panic_with_error(EscrowError::InvalidContractId);
    }
    env.events().publish(
        (symbol_short!("contract"), contract_id),
        (
            contract.status as u32,
            contract.funded_amount,
            contract.released_amount,
            contract.refunded_amount,
            contract.total_deposited,
        ),
    );

    let next_id: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::NextEventId)
        .unwrap_or(0);

    let entry = EventEntry {
        contract_id,
        status: contract.status as u32,
        funded_amount: contract.funded_amount,
        released_amount: contract.released_amount,
        refunded_amount: contract.refunded_amount,
        total_deposited: contract.total_deposited,
    };

    env.storage()
        .persistent()
        .set(&DataKey::Event(next_id), &entry);

    env.storage()
        .persistent()
        .set(&DataKey::NextEventId, &(next_id + 1));
}

/// Validate that event payload amounts are non-negative.
/// Returns `Ok(())` when all amounts are >= 0.
pub(crate) fn validate_event_amounts(
    funded_amount: i128,
    released_amount: i128,
    refunded_amount: i128,
    total_deposited: i128,
) -> Result<(), crate::EscrowError> {
    if funded_amount < 0 || released_amount < 0 || refunded_amount < 0 || total_deposited < 0 {
        return Err(EscrowError::AmountMustBePositive);
    }
    Ok(())
}

/// Emits an `mlstn_idx` indexed event for off-chain milestone-history
/// reconstruction.
///
/// This event fires on every milestone state change: creation, release,
/// and both refund entrypoints.
///
/// # Event Specification
/// - **Topic**: `(symbol_short!("mlstn_idx"), contract_id: u32, milestone_index: u32)`
/// - **Payload**: [`MilestoneIndexEvent`] — a named struct replacing the previous
///   opaque `(amount, released, refunded, timestamp)` tuple.
pub fn emit_milestone_index_event(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    amount: i128,
    released: bool,
    refunded: bool,
) {
    env.events().publish(
        (symbol_short!("mlstn_idx"), contract_id, milestone_index),
        MilestoneIndexEvent {
            amount,
            released,
            refunded,
            timestamp: env.ledger().timestamp(),
        },
    );
}
