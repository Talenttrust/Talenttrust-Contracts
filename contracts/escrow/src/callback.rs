use crate::{ContractStatus, DataKey, Error};
use soroban_sdk::{contracttype, Address, Env};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallbackRecord {
    pub origin: Address,
    pub contract_id: u32,
    pub phase: u32,
    pub nonce: u64,
}

/// Validate and consume a callback once, binding it to the current contract
/// instance and lifecycle phase. Any attempt to replay the same identifier, or to
/// reuse it against a different contract/phase/origin, is rejected.
pub fn validate_callback(
    env: &Env,
    origin: &Address,
    contract_id: u32,
    expected_phase: u32,
    nonce: u64,
) -> CallbackRecord {
    let key = DataKey::Callback(contract_id, expected_phase);
    let per_nonce_key = DataKey::CallbackNonce(origin.clone(), nonce);

    if env.storage().persistent().has(&per_nonce_key) {
        let previous: CallbackRecord = env
            .storage()
            .persistent()
            .get(&per_nonce_key)
            .unwrap_or_else(|| env.panic_with_error(Error::InvalidState));
        if previous.origin == *origin
            && previous.contract_id == contract_id
            && previous.phase == expected_phase
            && previous.nonce == nonce
        {
            env.panic_with_error(Error::InvalidState);
        }
        env.panic_with_error(Error::InvalidState);
    }

    if let Some(previous) = env.storage().persistent().get::<_, CallbackRecord>(&key) {
        if previous.origin != *origin
            || previous.contract_id != contract_id
            || previous.phase != expected_phase
            || previous.nonce != nonce
        {
            env.panic_with_error(Error::InvalidState);
        }
        env.panic_with_error(Error::InvalidState);
    }

    let contract: crate::Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
    if contract.status == ContractStatus::Cancelled {
        env.panic_with_error(Error::InvalidState);
    }

    let record = CallbackRecord {
        origin: origin.clone(),
        contract_id,
        phase: expected_phase,
        nonce,
    };

    env.storage().persistent().set(&key, &record);
    env.storage().persistent().set(&per_nonce_key, &record);
    record
}

/// Consume a callback exactly once after validating the origin, contract, phase,
/// and nonce have all been bound to the current instance.
pub fn consume_callback(
    env: &Env,
    origin: &Address,
    contract_id: u32,
    expected_phase: u32,
    nonce: u64,
) -> CallbackRecord {
    validate_callback(env, origin, contract_id, expected_phase, nonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn contract_address(env: &Env) -> Address {
        env.register(crate::Escrow, ())
    }

    fn with_contract<T>(env: &Env, contract: &Address, f: impl FnOnce() -> T) -> T {
        env.as_contract(contract, f)
    }

    fn make_active_contract(env: &Env, contract: &Address, contract_id: u32) {
        with_contract(env, contract, || {
            env.storage().persistent().set(
                &DataKey::Contract(contract_id),
                &crate::Contract {
                    client: Address::generate(env),
                    freelancer: Address::generate(env),
                    arbiter: None,
                    status: ContractStatus::Funded,
                    total_deposited: 100,
                    funded_amount: 100,
                    released_amount: 0,
                    refunded_amount: 0,
                    release_authorization: crate::ReleaseAuthorization::ClientOnly,
                    reputation_issued: false,
                },
            );
        });
    }

    fn make_cancelled_contract(env: &Env, contract: &Address, contract_id: u32) {
        with_contract(env, contract, || {
            env.storage().persistent().set(
                &DataKey::Contract(contract_id),
                &crate::Contract {
                    client: Address::generate(env),
                    freelancer: Address::generate(env),
                    arbiter: None,
                    status: ContractStatus::Cancelled,
                    total_deposited: 0,
                    funded_amount: 0,
                    released_amount: 0,
                    refunded_amount: 0,
                    release_authorization: crate::ReleaseAuthorization::ClientOnly,
                    reputation_issued: false,
                },
            );
        });
    }

    #[test]
    fn valid_callback_is_bound_and_consumed_once() {
        let env = Env::default();
        let origin = Address::generate(&env);
        let contract_id = 7_u32;
        let contract = contract_address(&env);
        make_active_contract(&env, &contract, contract_id);

        let record = with_contract(&env, &contract, || validate_callback(&env, &origin, contract_id, 3, 11));

        assert_eq!(record.origin, origin);
        assert_eq!(record.contract_id, contract_id);
        assert_eq!(record.phase, 3);
        assert_eq!(record.nonce, 11);

        with_contract(&env, &contract, || {
            assert!(env.storage().persistent().has(&DataKey::Callback(contract_id, 3)));
            assert!(env
                .storage()
                .persistent()
                .has(&DataKey::CallbackNonce(origin.clone(), 11)));
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn replay_callback_is_rejected() {
        let env = Env::default();
        let origin = Address::generate(&env);
        let contract_id = 7_u32;
        let contract = contract_address(&env);
        make_active_contract(&env, &contract, contract_id);

        with_contract(&env, &contract, || {
            validate_callback(&env, &origin, contract_id, 3, 11);
            validate_callback(&env, &origin, contract_id, 3, 11);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn wrong_origin_is_rejected() {
        let env = Env::default();
        let origin = Address::generate(&env);
        let other = Address::generate(&env);
        let contract_id = 7_u32;
        let contract = contract_address(&env);
        make_active_contract(&env, &contract, contract_id);

        with_contract(&env, &contract, || {
            validate_callback(&env, &origin, contract_id, 3, 11);
            validate_callback(&env, &other, contract_id, 3, 11);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn wrong_nonce_is_rejected() {
        let env = Env::default();
        let origin = Address::generate(&env);
        let contract_id = 7_u32;
        let contract = contract_address(&env);
        make_active_contract(&env, &contract, contract_id);

        with_contract(&env, &contract, || {
            validate_callback(&env, &origin, contract_id, 3, 11);
            validate_callback(&env, &origin, contract_id, 3, 12);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn callback_after_cancellation_is_rejected() {
        let env = Env::default();
        let origin = Address::generate(&env);
        let contract_id = 7_u32;
        let contract = contract_address(&env);

        with_contract(&env, &contract, || {
            make_cancelled_contract(&env, &contract, contract_id);
            validate_callback(&env, &origin, contract_id, 3, 11);
        });
    }
}
