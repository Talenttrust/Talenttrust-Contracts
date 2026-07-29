//! Shared authorization helpers for role validation and release-mode checking.
//!
//! This module centralizes repeated authorization logic across the contract,
//! providing reusable helpers for:
//! - Participant role determination (client, freelancer, arbiter)
//! - Release authorization validation against contract release modes
//! - Admin authorization checks
//!
//! All helpers use consistent error handling with `UnauthorizedRole` for
//! authorization failures, enabling reviewers to reason about access control
//! uniformly across all entrypoints.

use crate::types::{Contract, Error, ReleaseAuthorization};
use soroban_sdk::{Address, Env};

/// Represents the role of a caller in a contract context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParticipantRole {
    /// The client who requested the work.
    Client,
    /// The freelancer providing the work.
    Freelancer,
    /// The arbiter assigned to resolve disputes (if any).
    Arbiter,
}

/// Determines the role of a caller with respect to a contract.
///
/// # Arguments
/// * `caller` - The address to check
/// * `contract` - The contract to check against
///
/// # Returns
/// * `Some(role)` - The caller's role if they are a participant
/// * `None` - If the caller is not a participant in the contract
pub fn get_caller_role(caller: &Address, contract: &Contract) -> Option<ParticipantRole> {
    if caller == &contract.client {
        Some(ParticipantRole::Client)
    } else if caller == &contract.freelancer {
        Some(ParticipantRole::Freelancer)
    } else if let Some(arbiter) = &contract.arbiter {
        if caller == arbiter {
            Some(ParticipantRole::Arbiter)
        } else {
            None
        }
    } else {
        None
    }
}

/// Checks if a caller is authorized for release under the contract's release mode.
///
/// This helper combines role determination and release-mode validation, ensuring
/// that both:
/// 1. The caller is a valid participant in the contract.
/// 2. The caller's role is permitted by the contract's `release_authorization` mode.
///
/// # Arguments
/// * `env` - The contract environment (used for error reporting)
/// * `caller` - The address to check
/// * `contract` - The contract data
///
/// # Returns
/// `true` if authorization succeeds (panics on error)
///
/// # Panics
/// * `UnauthorizedRole` - If caller is not authorized for release
///
/// # Examples
/// For a contract with `ReleaseAuthorization::ClientOnly`, only the client can
/// be authorized; both freelancer and arbiter will fail.
///
/// For `ReleaseAuthorization::MultiSig`, the caller must be either client or
/// freelancer (and both are required for approval, but this helper only checks
/// if one caller *can* approve).
pub fn require_release_authorization(env: &Env, caller: &Address, contract: &Contract) {
    let role = get_caller_role(caller, contract);

    if let Some(role) = role {
        // Caller is a participant; now check release mode
        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if role != ParticipantRole::Client {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if role != ParticipantRole::Arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if role != ParticipantRole::Client && role != ParticipantRole::Arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::MultiSig => {
                if role != ParticipantRole::Client && role != ParticipantRole::Freelancer {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
        }
    } else {
        // Not a participant
        env.panic_with_error(Error::UnauthorizedRole);
    }
}

/// Checks if a caller is a valid participant in a contract.
///
/// A valid participant is one of: client, freelancer, or assigned arbiter.
/// This is useful for entrypoints that allow any participant to take action
/// but need to verify the caller is at least a participant.
///
/// # Arguments
/// * `env` - The contract environment (used for error reporting)
/// * `caller` - The address to check
/// * `contract` - The contract data
///
/// # Returns
/// The caller's role if they are a participant
///
/// # Panics
/// * `UnauthorizedRole` - If caller is not a participant
pub fn require_participant(
    env: &Env,
    caller: &Address,
    contract: &Contract,
) -> ParticipantRole {
    get_caller_role(caller, contract).unwrap_or_else(|| {
        env.panic_with_error(Error::UnauthorizedRole);
        unreachable!()
    })
}

/// Checks if a caller is authorized as an admin.
///
/// The admin is stored under `DataKey::Admin` and is typically set during
/// initialization or via a two-step admin rotation flow.
///
/// # Arguments
/// * `env` - The contract environment
/// * `caller` - The address to check
/// * `stored_admin` - The stored admin address
///
/// # Panics
/// * `UnauthorizedRole` - If caller is not the stored admin
pub fn require_admin(env: &Env, caller: &Address, stored_admin: &Address) {
    if caller != stored_admin {
        env.panic_with_error(Error::UnauthorizedRole);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    /// Helper to create a test contract with given participants and release mode
    fn make_test_contract(
        env: &Env,
        client: &Address,
        freelancer: &Address,
        arbiter: Option<&Address>,
        release_auth: ReleaseAuthorization,
    ) -> Contract {
        Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: arbiter.cloned(),
            status: crate::types::ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: release_auth,
            reputation_issued: false,
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // get_caller_role tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_get_caller_role_identifies_client() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        assert_eq!(get_caller_role(&client, &contract), Some(ParticipantRole::Client));
    }

    #[test]
    fn test_get_caller_role_identifies_freelancer() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        assert_eq!(get_caller_role(&freelancer, &contract), Some(ParticipantRole::Freelancer));
    }

    #[test]
    fn test_get_caller_role_identifies_arbiter() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            Some(&arbiter),
            ReleaseAuthorization::ArbiterOnly,
        );

        assert_eq!(get_caller_role(&arbiter, &contract), Some(ParticipantRole::Arbiter));
    }

    #[test]
    fn test_get_caller_role_returns_none_for_non_participant() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let other = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        assert_eq!(get_caller_role(&other, &contract), None);
    }

    #[test]
    fn test_get_caller_role_no_arbiter_set() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let would_be_arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        assert_eq!(get_caller_role(&would_be_arbiter, &contract), None);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_release_authorization tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_require_release_authorization_client_only_allows_client() {
        let env = Env::default();
        env.mock_all_auths();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        // Should not panic
        require_release_authorization(&env, &client, &contract);
    }

    #[test]
    fn test_require_release_authorization_client_only_denies_freelancer() {
        let env = Env::default();
        env.mock_all_auths();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_release_authorization(&env, &freelancer, &contract);
        }));
        assert!(result.is_err(), "Freelancer should not be authorized in ClientOnly mode");
    }

    #[test]
    fn test_require_release_authorization_arbiter_only_allows_arbiter() {
        let env = Env::default();
        env.mock_all_auths();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            Some(&arbiter),
            ReleaseAuthorization::ArbiterOnly,
        );

        // Should not panic
        require_release_authorization(&env, &arbiter, &contract);
    }

    #[test]
    fn test_require_release_authorization_arbiter_only_denies_client() {
        let env = Env::default();
        env.mock_all_auths();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            Some(&arbiter),
            ReleaseAuthorization::ArbiterOnly,
        );

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_release_authorization(&env, &client, &contract);
        }));
        assert!(result.is_err(), "Client should not be authorized in ArbiterOnly mode");
    }

    #[test]
    fn test_require_release_authorization_client_and_arbiter_allows_both() {
        let env = Env::default();
        env.mock_all_auths();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            Some(&arbiter),
            ReleaseAuthorization::ClientAndArbiter,
        );

        // Both should succeed
        require_release_authorization(&env, &client, &contract);
        require_release_authorization(&env, &arbiter, &contract);
    }

    #[test]
    fn test_require_release_authorization_client_and_arbiter_denies_freelancer() {
        let env = Env::default();
        env.mock_all_auths();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            Some(&arbiter),
            ReleaseAuthorization::ClientAndArbiter,
        );

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_release_authorization(&env, &freelancer, &contract);
        }));
        assert!(result.is_err(), "Freelancer should not be authorized in ClientAndArbiter mode");
    }

    #[test]
    fn test_require_release_authorization_multisig_allows_both() {
        let env = Env::default();
        env.mock_all_auths();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::MultiSig,
        );

        // Both should succeed
        require_release_authorization(&env, &client, &contract);
        require_release_authorization(&env, &freelancer, &contract);
    }

    #[test]
    fn test_require_release_authorization_multisig_denies_non_participant() {
        let env = Env::default();
        env.mock_all_auths();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let other = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::MultiSig,
        );

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_release_authorization(&env, &other, &contract);
        }));
        assert!(result.is_err(), "Non-participant should not be authorized");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_participant tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_require_participant_accepts_client() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        let role = require_participant(&env, &client, &contract);
        assert_eq!(role, ParticipantRole::Client);
    }

    #[test]
    fn test_require_participant_accepts_freelancer() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        let role = require_participant(&env, &freelancer, &contract);
        assert_eq!(role, ParticipantRole::Freelancer);
    }

    #[test]
    fn test_require_participant_accepts_arbiter() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            Some(&arbiter),
            ReleaseAuthorization::ArbiterOnly,
        );

        let role = require_participant(&env, &arbiter, &contract);
        assert_eq!(role, ParticipantRole::Arbiter);
    }

    #[test]
    fn test_require_participant_rejects_non_participant() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let other = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_participant(&env, &other, &contract);
        }));
        assert!(result.is_err(), "Non-participant should be rejected");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_admin tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_require_admin_accepts_correct_admin() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);

        // Should not panic
        require_admin(&env, &admin, &admin);
    }

    #[test]
    fn test_require_admin_rejects_wrong_admin() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_admin(&env, &other, &admin);
        }));
        assert!(result.is_err(), "Wrong admin should be rejected");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Edge cases and boundary conditions
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_client_and_freelancer_are_different_roles() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        assert_ne!(
            get_caller_role(&client, &contract),
            get_caller_role(&freelancer, &contract)
        );
    }

    #[test]
    fn test_arbiter_none_means_no_arbiter_role() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let random_addr = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ClientOnly,
        );

        assert_eq!(get_caller_role(&random_addr, &contract), None);
        assert!(matches!(get_caller_role(&random_addr, &contract), None));
    }

    #[test]
    fn test_all_release_modes_respect_non_participants() {
        let env = Env::default();
        env.mock_all_auths();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let non_participant = Address::generate(&env);

        let modes = [
            ReleaseAuthorization::ClientOnly,
            ReleaseAuthorization::ArbiterOnly,
            ReleaseAuthorization::ClientAndArbiter,
            ReleaseAuthorization::MultiSig,
        ];

        for mode in &modes {
            let contract = make_test_contract(&env, &client, &freelancer, Some(&arbiter), *mode);

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                require_release_authorization(&env, &non_participant, &contract);
            }));
            assert!(
                result.is_err(),
                "Non-participant should be rejected in {:?} mode",
                mode
            );
        }
    }
}
