//! Centralized storage precondition checks and contract loading helpers.
//!
//! This module extracts repeated storage validation patterns into a single source
//! of truth, ensuring consistent error handling and reducing code duplication across
//! entrypoints. All contract loading operations should route through these helpers.

use crate::{Contract, DataKey, Error};
use soroban_sdk::{Env, Symbol, Vec};

/// Check if the contract system has been initialized.
///
/// Initialization is a prerequisite for all money-flow operations. This check
/// ensures that the admin-controlled safety rails (pause, emergency controls,
/// protocol fees) are always in scope before any funds can move.
///
/// # Arguments
/// * `env` - The contract environment
///
/// # Panics
/// - `NotInitialized` if initialization has not been completed
///
/// # Returns
/// `true` if initialized, or panics with `NotInitialized`
pub(crate) fn require_initialized(env: &Env) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&DataKey::Initialized)
        .unwrap_or(false)
        .then_some(true)
        .ok_or(Error::NotInitialized)
        .unwrap_or_else(|err| env.panic_with_error(err))
}

/// Load a contract from persistent storage.
///
/// This is the canonical pattern for retrieving a contract. It handles the
/// storage read with consistent error reporting.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID to load
///
/// # Panics
/// - `ContractNotFound` if no contract exists for this ID
///
/// # Returns
/// The loaded `Contract` or panics with `ContractNotFound`
pub(crate) fn load_contract(env: &Env, contract_id: u32) -> Contract {
    env.storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
}

/// Load milestones for a contract from persistent storage.
///
/// Milestones are stored under a composite key combining the contract ID
/// and a "milestones" symbol. This helper centralizes the retrieval pattern.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID whose milestones to load
///
/// # Panics
/// - `ContractNotFound` if no milestone vector exists for this contract
///
/// # Returns
/// The loaded milestone vector or panics with `ContractNotFound`
pub(crate) fn load_milestones(env: &Env, contract_id: u32) -> Vec<crate::Milestone> {
    let milestone_key = Symbol::new(env, "milestones");
    env.storage()
        .persistent()
        .get(&(DataKey::Contract(contract_id), milestone_key))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound))
}

/// Load a contract, optionally with precondition checks for mutation.
///
/// This is the primary helper for loading contracts with optional safety guards:
/// - `check_paused`: If true, verifies pause/emergency flags are not set
/// - `check_finalized`: If true, verifies the contract has not been finalized
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID to load
/// * `check_paused` - Whether to verify pause/emergency states
/// * `check_finalized` - Whether to verify finalization state
///
/// # Panics
/// - `ContractPaused` if `check_paused` is true and pause flag is set
/// - `EmergencyActive` if `check_paused` is true and emergency flag is set
/// - `ContractNotFound` if no contract exists for this ID
/// - `AlreadyFinalized` if `check_finalized` is true and contract is finalized
///
/// # Returns
/// The loaded `Contract` if all preconditions pass
pub(crate) fn load_contract_checked(
    env: &Env,
    contract_id: u32,
    check_paused: bool,
    check_finalized: bool,
) -> Contract {
    if check_paused {
        require_not_paused(env);
    }

    let contract = load_contract(env, contract_id);

    if check_finalized {
        require_not_finalized(env, contract_id);
    }

    contract
}

/// Check if the contract system is paused or in emergency mode.
///
/// # Arguments
/// * `env` - The contract environment
///
/// # Panics
/// - `ContractPaused` if the pause flag is set
/// - `EmergencyActive` if the emergency flag is set
///
/// # Returns
/// `true` if neither pause nor emergency is active, or panics
pub(crate) fn require_not_paused(env: &Env) -> bool {
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        env.panic_with_error(Error::ContractPaused);
    }
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::Emergency)
        .unwrap_or(false)
    {
        env.panic_with_error(Error::EmergencyActive);
    }
    true
}

/// Check if a contract has been finalized.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID to check
///
/// # Returns
/// `true` if the contract is finalized
pub(crate) fn is_finalized(env: &Env, contract_id: u32) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Finalization(contract_id))
}

/// Require that a contract has not been finalized.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID to check
///
/// # Panics
/// - `AlreadyFinalized` if the contract has been finalized
///
/// # Returns
/// `true` if not finalized, or panics
pub(crate) fn require_not_finalized(env: &Env, contract_id: u32) -> bool {
    if is_finalized(env, contract_id) {
        env.panic_with_error(Error::AlreadyFinalized);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Milestone;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    fn setup_test_env() -> (Env, Address) {
        let env = Env::default();
        let admin = Address::generate(&env);
        (env, admin)
    }

    #[test]
    fn test_require_initialized_when_true() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            env.storage().persistent().set(&DataKey::Initialized, &true);
            let result = require_initialized(&env);
            assert!(result);
        });
    }

    #[test]
    #[should_panic(expected = "NotInitialized")]
    fn test_require_initialized_when_false() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            require_initialized(&env);
        });
    }

    #[test]
    #[should_panic(expected = "ContractNotFound")]
    fn test_load_contract_not_found() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            load_contract(&env, 999);
        });
    }

    #[test]
    fn test_load_contract_found() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            let client = Address::generate(&env);
            let freelancer = Address::generate(&env);
            let contract = Contract {
                client: client.clone(),
                freelancer: freelancer.clone(),
                arbiter: None,
                status: crate::ContractStatus::Created,
                release_authorization: crate::ReleaseAuthorization::ClientOnly,
                funded_amount: 0,
                released_amount: 0,
                refunded_amount: 0,
                total_deposited: 0,
                reputation_issued: false,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Contract(42), &contract);

            let loaded = load_contract(&env, 42);
            assert_eq!(loaded.client, client);
            assert_eq!(loaded.freelancer, freelancer);
            assert_eq!(loaded.status, crate::ContractStatus::Created);
        });
    }

    #[test]
    #[should_panic(expected = "ContractNotFound")]
    fn test_load_milestones_not_found() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            load_milestones(&env, 999);
        });
    }

    #[test]
    fn test_load_milestones_found() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            let milestones = Vec::from_array(
                &env,
                [
                    Milestone {
                        amount: 1000,
                        funded_amount: 0,
                        released: false,
                        refunded: false,
                        deadline: None,
                        refunded_amount: 0,
                        work_evidence: None,
                    },
                    Milestone {
                        amount: 2000,
                        funded_amount: 0,
                        released: false,
                        refunded: false,
                        deadline: None,
                        refunded_amount: 0,
                        work_evidence: None,
                    },
                ],
            );

            let milestone_key = Symbol::new(&env, "milestones");
            env.storage()
                .persistent()
                .set(&(DataKey::Contract(42), milestone_key), &milestones);

            let loaded = load_milestones(&env, 42);
            assert_eq!(loaded.len(), 2);
            assert_eq!(loaded.get(0).unwrap().amount, 1000);
            assert_eq!(loaded.get(1).unwrap().amount, 2000);
        });
    }

    #[test]
    fn test_require_not_paused_when_not_paused() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            let result = require_not_paused(&env);
            assert!(result);
        });
    }

    #[test]
    #[should_panic(expected = "ContractPaused")]
    fn test_require_not_paused_when_paused() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            env.storage()
                .persistent()
                .set(&DataKey::Paused, &true);
            require_not_paused(&env);
        });
    }

    #[test]
    #[should_panic(expected = "EmergencyActive")]
    fn test_require_not_paused_when_emergency() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            env.storage()
                .persistent()
                .set(&DataKey::Emergency, &true);
            require_not_paused(&env);
        });
    }

    #[test]
    fn test_is_finalized_when_false() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            let result = is_finalized(&env, 42);
            assert!(!result);
        });
    }

    #[test]
    fn test_is_finalized_when_true() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            env.storage()
                .persistent()
                .set(&DataKey::Finalization(42), &true);

            let result = is_finalized(&env, 42);
            assert!(result);
        });
    }

    #[test]
    fn test_require_not_finalized_when_not_finalized() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            let result = require_not_finalized(&env, 42);
            assert!(result);
        });
    }

    #[test]
    #[should_panic(expected = "AlreadyFinalized")]
    fn test_require_not_finalized_when_finalized() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            env.storage()
                .persistent()
                .set(&DataKey::Finalization(42), &true);

            require_not_finalized(&env, 42);
        });
    }

    #[test]
    fn test_load_contract_checked_all_checks() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            let client = Address::generate(&env);
            let freelancer = Address::generate(&env);
            let contract = Contract {
                client: client.clone(),
                freelancer: freelancer.clone(),
                arbiter: None,
                status: crate::ContractStatus::Created,
                release_authorization: crate::ReleaseAuthorization::ClientOnly,
                funded_amount: 0,
                released_amount: 0,
                refunded_amount: 0,
                total_deposited: 0,
                reputation_issued: false,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Contract(42), &contract);

            let loaded = load_contract_checked(&env, 42, true, true);
            assert_eq!(loaded.client, client);
        });
    }

    #[test]
    #[should_panic(expected = "ContractPaused")]
    fn test_load_contract_checked_paused() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            let client = Address::generate(&env);
            let freelancer = Address::generate(&env);
            let contract = Contract {
                client: client.clone(),
                freelancer: freelancer.clone(),
                arbiter: None,
                status: crate::ContractStatus::Created,
                release_authorization: crate::ReleaseAuthorization::ClientOnly,
                funded_amount: 0,
                released_amount: 0,
                refunded_amount: 0,
                total_deposited: 0,
                reputation_issued: false,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Contract(42), &contract);
            env.storage()
                .persistent()
                .set(&DataKey::Paused, &true);

            load_contract_checked(&env, 42, true, true);
        });
    }

    #[test]
    #[should_panic(expected = "AlreadyFinalized")]
    fn test_load_contract_checked_finalized() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            let client = Address::generate(&env);
            let freelancer = Address::generate(&env);
            let contract = Contract {
                client: client.clone(),
                freelancer: freelancer.clone(),
                arbiter: None,
                status: crate::ContractStatus::Created,
                release_authorization: crate::ReleaseAuthorization::ClientOnly,
                funded_amount: 0,
                released_amount: 0,
                refunded_amount: 0,
                total_deposited: 0,
                reputation_issued: false,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Contract(42), &contract);
            env.storage()
                .persistent()
                .set(&DataKey::Finalization(42), &true);

            load_contract_checked(&env, 42, true, true);
        });
    }

    #[test]
    fn test_load_contract_checked_no_checks() {
        let (env, admin) = setup_test_env();
        env.as_contract(&admin, || {
            let client = Address::generate(&env);
            let freelancer = Address::generate(&env);
            let contract = Contract {
                client: client.clone(),
                freelancer: freelancer.clone(),
                arbiter: None,
                status: crate::ContractStatus::Created,
                release_authorization: crate::ReleaseAuthorization::ClientOnly,
                funded_amount: 0,
                released_amount: 0,
                refunded_amount: 0,
                total_deposited: 0,
                reputation_issued: false,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Contract(42), &contract);
            env.storage()
                .persistent()
                .set(&DataKey::Paused, &true);
            env.storage()
                .persistent()
                .set(&DataKey::Finalization(42), &true);

            // Should succeed because checks are disabled
            let loaded = load_contract_checked(&env, 42, false, false);
            assert_eq!(loaded.client, client);
        });
    }
}
