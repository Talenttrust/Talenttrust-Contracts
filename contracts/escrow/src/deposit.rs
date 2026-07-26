use crate::{ttl, utils::emit_status_changed, Contract, ContractStatus, DataKey, Error, Milestone};
use soroban_sdk::{Address, Env, Symbol, Vec};

/// Deposits funds into the contract. Transitions to Funded status when fully funded.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `caller` - The address of the caller (must be the client)
/// * `amount` - The amount to deposit (in stroops)
///
/// # Returns
/// `true` if deposit was successful
///
/// # Errors
/// * `AmountMustBePositive` - If amount is <= 0
/// * `ContractNotFound` - If contract doesn't exist
/// * `InvalidState` - If contract is not in Created state
/// * `UnauthorizedRole` - If caller is not the client
pub fn deposit_funds_impl(env: &Env, contract_id: u32, caller: Address, amount: i128) -> bool {
    if amount <= 0 {
        env.panic_with_error(Error::AmountMustBePositive);
    }

    let mut contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    ttl::extend_contract_ttl(&env, contract_id);

    if caller != contract.client {
        env.panic_with_error(Error::UnauthorizedRole);
    }
    caller.require_auth();

    if contract.status != ContractStatus::Created {
        env.panic_with_error(Error::InvalidState);
    }

    contract.funded_amount += amount;

    let milestones: Vec<Milestone> = ttl::load_milestones(&env, contract_id);

    let total_amount: i128 = milestones.iter().map(|m| m.amount).sum();

    if contract.funded_amount >= total_amount && contract.status == ContractStatus::Created {
        let old_status = contract.status.clone();
        contract.status = ContractStatus::Funded;
        emit_status_changed(env, contract_id, old_status, ContractStatus::Funded);
    }

    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), &contract);

    ttl::extend_contract_ttl(&env, contract_id);

    true
}
