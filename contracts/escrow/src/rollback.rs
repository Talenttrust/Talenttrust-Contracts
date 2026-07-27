use crate::ttl::{PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS};
use crate::{ttl, Contract, ContractStatus, DataKey, Error, Escrow, Milestone};
use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRollbackRecord {
    pub contract: Contract,
    pub milestones: Vec<Milestone>,
}

fn rollback_key(contract_id: u32) -> DataKey {
    DataKey::DisputeRollback(contract_id)
}

pub(crate) fn store_dispute_rollback(
    env: &Env,
    contract_id: u32,
    contract: &Contract,
    milestones: &Vec<Milestone>,
) {
    let key = rollback_key(contract_id);
    env.storage().persistent().set(
        &key,
        &DisputeRollbackRecord {
            contract: contract.clone(),
            milestones: milestones.clone(),
        },
    );
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS);
}

pub(crate) fn clear_dispute_rollback(env: &Env, contract_id: u32) {
    env.storage()
        .persistent()
        .remove(&rollback_key(contract_id));
}

pub(crate) fn rollback_dispute_impl(env: &Env, contract_id: u32) -> bool {
    Escrow::require_initialized(env);
    Escrow::require_not_paused(env);

    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
    admin.require_auth();

    let mut contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    Escrow::require_not_finalized(env, contract_id);
    if contract.status != ContractStatus::Disputed {
        env.panic_with_error(Error::RollbackNotAllowed);
    }

    let record: DisputeRollbackRecord = env
        .storage()
        .persistent()
        .get(&rollback_key(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::RollbackNotAllowed));

    if !matches!(
        record.contract.status,
        ContractStatus::Funded | ContractStatus::PartiallyFunded
    ) {
        env.panic_with_error(Error::RollbackNotAllowed);
    }

    let mut expected_contract = record.contract.clone();
    expected_contract.status = ContractStatus::Disputed;
    let milestones = ttl::load_milestones(env, contract_id);
    if contract != expected_contract || milestones != record.milestones {
        env.panic_with_error(Error::RollbackStateChanged);
    }

    let restored_status = record.contract.status;
    contract.status = restored_status;
    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), &contract);
    clear_dispute_rollback(env, contract_id);
    ttl::extend_contract_and_milestones_ttl(env, contract_id);

    env.events().publish(
        (symbol_short!("rollback"), contract_id),
        (
            admin,
            ContractStatus::Disputed,
            restored_status,
            env.ledger().timestamp(),
        ),
    );

    true
}
