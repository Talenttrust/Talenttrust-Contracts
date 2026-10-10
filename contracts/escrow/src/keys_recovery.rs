//! Admin-visible recovery markers for key recovery operations.

use soroban_sdk::{contracttype, symbol_short, Env, String};

fn recovery_key(contract_id: u32) -> (soroban_sdk::Symbol, u32) {
    (symbol_short!("key_rec"), contract_id)
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyRecoveryStatus {
    InProgress,
    Recovered,
}

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
        .set(&recovery_key(record.contract_id), record);
}

pub(crate) fn load_recovery_record(env: &Env, contract_id: u32) -> Option<KeyRecoveryRecord> {
    env.storage()
        .persistent()
        .get(&recovery_key(contract_id))
}

pub(crate) fn clear_recovery_record(env: &Env, contract_id: u32) {
    env.storage()
        .persistent()
        .remove(&recovery_key(contract_id));
}
