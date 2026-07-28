use crate::types::{Contract, MilestoneIndexEvent};
use crate::EscrowError;
use soroban_sdk::{symbol_short, Env};

pub use crate::types::MilestoneIndexEvent;

/// Maximum number of events processed in a batch operations.
pub const MAX_EVENT_BATCH_SIZE: usize = 100;


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

/// Emits an indexed event when a dispute is opened on a contract.
///
/// # Event Specification
/// - **Topic**: `(symbol_short!("dispute"), symbol_short!("opened"))`
/// - **Payload**: `(contract_id: u32, caller: Address, funded_amount: i128, released_amount: i128, refunded_amount: i128)`
pub fn emit_dispute_opened_event(
    env: &Env,
    contract_id: u32,
    caller: &Address,
    contract: &Contract,
) {
    env.events().publish(
        (symbol_short!("dispute"), symbol_short!("opened")),
        (
            contract_id,
            caller.clone(),
            contract.funded_amount,
            contract.released_amount,
            contract.refunded_amount,
        ),
    );
}

/// Emits an indexed event when a dispute is resolved.
///
/// # Event Specification
/// - **Topic**: `(symbol_short!("dispute"), symbol_short!("resolved"))`
/// - **Payload**: `(contract_id: u32, client_payout: i128, freelancer_payout: i128, resolution_code: u32, final_status: u32)`
pub fn emit_dispute_resolved_event(
    env: &Env,
    contract_id: u32,
    client_payout: i128,
    freelancer_payout: i128,
    resolution_code: u32,
    final_status: ContractStatus,
) {
    env.events().publish(
        (symbol_short!("dispute"), symbol_short!("resolved")),
        (
            contract_id,
            client_payout,
            freelancer_payout,
            resolution_code,
            final_status as u32,
        ),
    );
}
