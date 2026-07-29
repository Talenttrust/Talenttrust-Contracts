//! Centralized storage key definitions and constructors for escrow milestones.

use soroban_sdk::{Env, Symbol};

use crate::types::DataKey;

/// Returns the persistent storage key tuple for a contract's milestones vector:
/// `(DataKey::Contract(contract_id), Symbol::new(env, "milestones"))`.
pub fn milestone_key(env: &Env, contract_id: u32) -> (DataKey, Symbol) {
    (DataKey::Contract(contract_id), milestone_symbol(env))
}

/// Returns the `Symbol` key for milestones: `"milestones"`.
pub fn milestone_symbol(env: &Env) -> Symbol {
    Symbol::new(env, "milestones")
}

/// Returns the temporary storage key for milestone release approvals:
/// `DataKey::MilestoneApprovals(contract_id, milestone_index)`.
pub fn milestone_approval_key(contract_id: u32, milestone_index: u32) -> DataKey {
    DataKey::MilestoneApprovals(contract_id, milestone_index)
}
