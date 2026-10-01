//! Deterministic key-failure recovery primitives.

use crate::types::DataKey;
use soroban_sdk::{contracttype, Env, String};

/// Lifecycle state of a key-recovery operation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyRecoveryStatus {
    /// Recovery intent has been recorded; repair is in progress.
    InProgress = 0,
    /// Repair completed successfully; the contract key is healthy.
    Recovered = 1,
}

/// A recovery record written when an admin initiates key-failure repair.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyRecoveryRecord {
    pub contract_id: u32,
    pub status: KeyRecoveryStatus,
    pub reason: String,
    pub started_at: u64,
    pub completed_at: Option<u64>,
}

pub(crate) fn store_recovery_record(env: &Env, record: &KeyRecoveryRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::KeyRecovery(record.contract_id), record);
}

pub(crate) fn load_recovery_record(env: &Env, contract_id: u32) -> Option<KeyRecoveryRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::KeyRecovery(contract_id))
}

pub(crate) fn clear_recovery_record(env: &Env, contract_id: u32) {
    env.storage()
        .persistent()
        .remove(&DataKey::KeyRecovery(contract_id));
}
