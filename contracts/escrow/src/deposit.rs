use crate::{
    accumulate_amounts, ttl, Contract, ContractStatus, DataKey, Error, EscrowError, Milestone,
};
use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

/// Validated deposit data that is safe to use before any token transfer.
pub struct ValidatedDeposit {
    pub contract: Contract,
    pub new_funded_amount: i128,
    pub new_total_deposited: i128,
    pub total_amount: i128,
}

/// Validate a deposit without mutating state or moving tokens.
///
/// This preflight must run before the SAC transfer in `deposit_funds` so an
/// invalid deposit cannot debit the client and then fail during escrow state
/// validation.
pub fn validate_deposit(
    env: &Env,
    contract_id: u32,
    caller: &Address,
    amount: i128,
) -> ValidatedDeposit {
    if amount <= 0 {
        env.panic_with_error(Error::AmountMustBePositive);
    }

    let contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    if caller != &contract.client {
        env.panic_with_error(Error::UnauthorizedRole);
    }

    // Terminal-state guards: cancelled/refunded contracts must reject any
    // further value-moving operations such as deposits.
    if contract.status == ContractStatus::Cancelled {
        env.panic_with_error(EscrowError::ContractCancelled);
    }
    if contract.status == ContractStatus::Refunded {
        env.panic_with_error(EscrowError::ContractRefunded);
    }

    if contract.status != ContractStatus::Created
        && contract.status != ContractStatus::PartiallyFunded
    {
        env.panic_with_error(Error::InvalidState);
    }

    let milestone_key = Symbol::new(env, "milestones");
    let milestones: Vec<Milestone> = env
        .storage()
        .persistent()
        .get(&(DataKey::Contract(contract_id), milestone_key))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    /// Calculate the total amount from milestones with checked arithmetic.
    /// This prevents overflow panics that would brick the contract if a malformed
    /// contract with many large milestones were created (unlikely given the
    /// validation in create_contract, but defense-in-depth).
    let total_amount: i128 = accumulate_amounts(milestones.iter().map(|m| m.amount))
        .unwrap_or_else(|err| env.panic_with_error(err));
    let new_funded_amount = contract
        .funded_amount
        .checked_add(amount)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
    let new_total_deposited = contract
        .total_deposited
        .checked_add(amount)
        .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

    if new_funded_amount > total_amount {
        env.panic_with_error(Error::InvalidDepositAmount);
    }

    ValidatedDeposit {
        contract,
        new_funded_amount,
        new_total_deposited,
        total_amount,
    }
}

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
    let validated = validate_deposit(env, contract_id, &caller, amount);
    apply_validated_deposit(env, contract_id, caller, validated)
}

/// Apply a deposit after the caller has been validated and the token transfer succeeded.
pub fn apply_validated_deposit(
    env: &Env,
    contract_id: u32,
    caller: Address,
    validated: ValidatedDeposit,
) -> bool {
    let ValidatedDeposit {
        mut contract,
        new_funded_amount,
        new_total_deposited,
        total_amount,
    } = validated;

    ttl::extend_contract_ttl(&env, contract_id);

    caller.require_auth();

    let prev_total_deposited = contract.total_deposited;
    contract.funded_amount = new_funded_amount;
    contract.total_deposited = new_total_deposited;

    ttl::extend_milestone_ttl(&env, contract_id);

    if contract.funded_amount == total_amount {
        contract.status = ContractStatus::Funded;
    } else {
        contract.status = ContractStatus::PartiallyFunded;
    }

    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), &contract);

    ttl::extend_contract_ttl(&env, contract_id);

    let deposit_amount = new_total_deposited - prev_total_deposited;

    env.events().publish(
        (symbol_short!("deposit"), contract_id),
        (
            caller,
            deposit_amount,
            contract.status,
            env.ledger().timestamp(),
        ),
    );

    true
}
