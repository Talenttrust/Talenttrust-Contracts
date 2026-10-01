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
//!
//! # Authorization validation boundaries
//!
//! Every entry point in this module is a **pure, read-only predicate** over an
//! `(caller, contract)` pair. It either returns a verdict (`Some`/`None`,
//! `Result`, or a `ParticipantRole`) or fails closed with
//! [`Error::UnauthorizedRole`]. No helper writes to storage, so a rejected
//! authorization can never leave a partial state change behind.
//!
//! ## Valid party (accepted input)
//!
//! A caller is a *valid party* iff it equals one of the contract's resolved
//! participants. Resolution is deterministic and first-match-wins:
//!
//! 1. `caller == contract.client`      → [`ParticipantRole::Client`]
//! 2. `caller == contract.freelancer`  → [`ParticipantRole::Freelancer`]
//! 3. `contract.arbiter == Some(a)` and `caller == a` → [`ParticipantRole::Arbiter`]
//! 4. otherwise                        → `None` (unknown party)
//!
//! The ordering in rules 1–3 is the boundary for aliased addresses: if the same
//! address is stored in more than one participant slot, the earlier slot wins.
//!
//! ## Invalid / unknown party (rejected input)
//!
//! Any address that matches none of the participant slots is *unknown*.
//! `get_caller_role` returns `None`, `try_require_release_authorization`
//! returns `Err(UnauthorizedRole)`, and the panicking wrappers abort. An
//! unknown party is rejected in **every** [`ReleaseAuthorization`] mode,
//! independently of the mode's role rules.
//!
//! ## Empty / missing authorization
//!
//! There is no "empty authorization" grant: absent state is never treated as
//! permission.
//! - `contract.arbiter == None` means no arbiter slot exists, so no address can
//!   ever resolve to [`ParticipantRole::Arbiter`]. Under
//!   [`ReleaseAuthorization::ArbiterOnly`] a contract without an arbiter
//!   therefore authorizes *nobody*, including the client.
//! - `require_admin` compares against the stored admin value; an unset or
//!   mismatched admin authorizes nobody.
//!
//! ## Duplicate authorization
//!
//! Because the helpers are pure, an authorization check is idempotent:
//! evaluating the same `(caller, contract)` pair repeatedly yields the same
//! verdict and never mutates the contract. This is the boundary that makes
//! retries safe — a duplicate (replayed) authorization check cannot advance
//! state, and only the first successful *entrypoint* may do so.
//!
//! ## Release-mode role matrix (the core boundary)
//!
//! | `ReleaseAuthorization` | Client | Freelancer | Arbiter | Unknown |
//! |---|---|---|---|---|
//! | `ClientOnly`       | ✅ | ❌ | ❌ | ❌ |
//! | `ArbiterOnly`      | ❌ | ❌ | ✅ | ❌ |
//! | `ClientAndArbiter` | ✅ | ❌ | ✅ | ❌ |
//! | `MultiSig`         | ✅ | ✅ | ❌ | ❌ |
//!
//! A ✅ means `try_require_release_authorization` returns `Ok(())`; a ❌ means
//! it returns `Err(Error::UnauthorizedRole)`.
//!
//! ## Invariants enforced
//!
//! 1. **Fail-closed** — every failure path yields `UnauthorizedRole`; no
//!    helper returns a silent default or `Ok` for an unknown party.
//! 2. **Determinism** — the verdict depends only on equality comparisons over
//!    the input; there is no clock, oracle, or storage read.
//! 3. **No partial state change** — helpers never write storage, so a rejected
//!    check cannot corrupt or partially mutate contract state.
//! 4. **Monotonicity of the matrix** — removing the arbiter only ever narrows
//!    the set of authorized callers; it never widens it.

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
    try_require_release_authorization(caller, contract)
        .unwrap_or_else(|err| env.panic_with_error(err));
}

/// Recoverable authorization check for callers that need to handle a failed
/// release attempt without entering a panic path. The existing
/// `require_release_authorization` wrapper preserves the public behavior used
/// by current entrypoints.
pub fn try_require_release_authorization(
    caller: &Address,
    contract: &Contract,
) -> Result<(), Error> {
    let role = get_caller_role(caller, contract);

    // Caller must be a participant; otherwise reject immediately.
    if let Some(role) = role {
        // Caller is a participant; now check release mode
        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if role != ParticipantRole::Client {
                    return Err(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if role != ParticipantRole::Arbiter {
                    return Err(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if role != ParticipantRole::Client && role != ParticipantRole::Arbiter {
                    return Err(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::MultiSig => {
                if role != ParticipantRole::Client && role != ParticipantRole::Freelancer {
                    return Err(Error::UnauthorizedRole);
                }
            }
        }
    } else {
        // Not a participant
        return Err(Error::UnauthorizedRole);
    }

    Ok(())
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
pub fn require_participant(env: &Env, caller: &Address, contract: &Contract) -> ParticipantRole {
    get_caller_role(caller, contract).unwrap_or_else(|| {
        env.panic_with_error(Error::UnauthorizedRole);
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

/// Tests for authorization helpers.
#[cfg(test)]
mod tests {
    extern crate std;

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

        assert_eq!(
            get_caller_role(&client, &contract),
            Some(ParticipantRole::Client)
        );
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

        assert_eq!(
            get_caller_role(&freelancer, &contract),
            Some(ParticipantRole::Freelancer)
        );
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

        assert_eq!(
            get_caller_role(&arbiter, &contract),
            Some(ParticipantRole::Arbiter)
        );
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
    fn test_try_release_authorization_is_deterministic_for_non_participant() {
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

        assert_eq!(
            try_require_release_authorization(&other, &contract),
            Err(Error::UnauthorizedRole)
        );
        assert!(try_require_release_authorization(&client, &contract).is_ok());
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
        assert!(
            result.is_err(),
            "Freelancer should not be authorized in ClientOnly mode"
        );
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
        assert!(
            result.is_err(),
            "Client should not be authorized in ArbiterOnly mode"
        );
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
        assert!(
            result.is_err(),
            "Freelancer should not be authorized in ClientAndArbiter mode"
        );
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

    // ─────────────────────────────────────────────────────────────────────────
    // Validation boundaries (issue #1397)
    //
    // Criterion map:
    // - valid role/party ......... test_boundary_valid_party_roles_resolve_and_authorize
    // - invalid/unknown party .... test_boundary_unknown_party_rejected_in_all_modes
    // - empty/missing auth ....... test_boundary_missing_arbiter_narrows_authorization
    // - duplicate authorization .. test_boundary_duplicate_authorization_is_idempotent
    // - boundary (role matrix) ... test_boundary_release_mode_role_matrix
    // - alias/role precedence .... test_boundary_role_precedence_for_aliased_addresses
    // - rejected auth, no state .. test_boundary_rejected_authorization_leaves_state_unchanged
    // - admin equality boundary .. test_boundary_admin_requires_exact_match
    // ─────────────────────────────────────────────────────────────────────────

    fn all_release_modes() -> [ReleaseAuthorization; 4] {
        [
            ReleaseAuthorization::ClientOnly,
            ReleaseAuthorization::ArbiterOnly,
            ReleaseAuthorization::ClientAndArbiter,
            ReleaseAuthorization::MultiSig,
        ]
    }

    /// Valid party: every participant slot resolves to exactly one role and is
    /// accepted by `require_participant`.
    #[test]
    fn test_boundary_valid_party_roles_resolve_and_authorize() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            Some(&arbiter),
            ReleaseAuthorization::MultiSig,
        );

        assert_eq!(
            get_caller_role(&client, &contract),
            Some(ParticipantRole::Client)
        );
        assert_eq!(
            get_caller_role(&freelancer, &contract),
            Some(ParticipantRole::Freelancer)
        );
        assert_eq!(
            get_caller_role(&arbiter, &contract),
            Some(ParticipantRole::Arbiter)
        );

        assert_eq!(
            require_participant(&env, &client, &contract),
            ParticipantRole::Client
        );
        assert_eq!(
            require_participant(&env, &freelancer, &contract),
            ParticipantRole::Freelancer
        );
        assert_eq!(
            require_participant(&env, &arbiter, &contract),
            ParticipantRole::Arbiter
        );
    }

    /// Invalid/unknown party: an address matching no participant slot is
    /// rejected in every release mode and by the participant guard.
    #[test]
    fn test_boundary_unknown_party_rejected_in_all_modes() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let unknown = Address::generate(&env);

        for mode in all_release_modes() {
            let contract = make_test_contract(&env, &client, &freelancer, Some(&arbiter), mode);
            assert_eq!(
                get_caller_role(&unknown, &contract),
                None,
                "unknown party must not resolve to a role in {:?}",
                mode
            );
            assert_eq!(
                try_require_release_authorization(&unknown, &contract),
                Err(Error::UnauthorizedRole),
                "unknown party must be rejected in {:?}",
                mode
            );
        }

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            Some(&arbiter),
            ReleaseAuthorization::MultiSig,
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_participant(&env, &unknown, &contract);
        }));
        assert!(result.is_err(), "unknown party must not be a participant");
    }

    /// Empty/missing authorization: `arbiter == None` means the arbiter slot
    /// does not exist, so `ArbiterOnly` authorizes nobody (client included).
    #[test]
    fn test_boundary_missing_arbiter_narrows_authorization() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let would_be_arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::ArbiterOnly,
        );

        assert_eq!(get_caller_role(&would_be_arbiter, &contract), None);
        assert_eq!(
            try_require_release_authorization(&would_be_arbiter, &contract),
            Err(Error::UnauthorizedRole)
        );
        assert_eq!(
            try_require_release_authorization(&client, &contract),
            Err(Error::UnauthorizedRole),
            "ArbiterOnly with no arbiter must authorize nobody"
        );
        assert_eq!(
            try_require_release_authorization(&freelancer, &contract),
            Err(Error::UnauthorizedRole)
        );
    }

    /// Duplicate authorization: repeated checks yield identical verdicts and
    /// never mutate the contract (retries/replays are safe).
    #[test]
    fn test_boundary_duplicate_authorization_is_idempotent() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let unknown = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            None,
            ReleaseAuthorization::MultiSig,
        );
        let snapshot = contract.clone();

        for _ in 0..3 {
            assert!(try_require_release_authorization(&client, &contract).is_ok());
            assert!(try_require_release_authorization(&freelancer, &contract).is_ok());
            assert_eq!(
                try_require_release_authorization(&unknown, &contract),
                Err(Error::UnauthorizedRole)
            );
        }

        assert_eq!(
            contract, snapshot,
            "authorization checks must not mutate contract state"
        );
    }

    /// Boundary: exhaustive `ReleaseAuthorization` x role truth table.
    #[test]
    fn test_boundary_release_mode_role_matrix() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let unknown = Address::generate(&env);

        let callers = [&client, &freelancer, &arbiter, &unknown];
        let cases = [
            (
                ReleaseAuthorization::ClientOnly,
                [true, false, false, false],
            ),
            (
                ReleaseAuthorization::ArbiterOnly,
                [false, false, true, false],
            ),
            (
                ReleaseAuthorization::ClientAndArbiter,
                [true, false, true, false],
            ),
            (ReleaseAuthorization::MultiSig, [true, true, false, false]),
        ];

        for (mode, verdicts) in cases {
            let contract = make_test_contract(&env, &client, &freelancer, Some(&arbiter), mode);
            for (caller, allowed) in callers.iter().zip(verdicts.iter()) {
                let result = try_require_release_authorization(*caller, &contract);
                if *allowed {
                    assert!(result.is_ok(), "{:?} should allow the caller", mode);
                } else {
                    assert_eq!(
                        result,
                        Err(Error::UnauthorizedRole),
                        "{:?} should reject the caller",
                        mode
                    );
                }
            }
        }
    }

    /// Boundary: first-match-wins precedence when an address is stored in more
    /// than one participant slot.
    #[test]
    fn test_boundary_role_precedence_for_aliased_addresses() {
        let env = Env::default();
        let addr = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract =
            make_test_contract(&env, &addr, &addr, None, ReleaseAuthorization::ClientOnly);
        assert_eq!(
            get_caller_role(&addr, &contract),
            Some(ParticipantRole::Client),
            "client slot wins over freelancer slot"
        );

        let contract = make_test_contract(
            &env,
            &addr,
            &freelancer,
            Some(&addr),
            ReleaseAuthorization::ClientAndArbiter,
        );
        assert_eq!(
            get_caller_role(&addr, &contract),
            Some(ParticipantRole::Client),
            "client slot wins over arbiter slot"
        );
    }

    /// Invariant: a rejected authorization (recoverable or panicking) performs
    /// no state change.
    #[test]
    fn test_boundary_rejected_authorization_leaves_state_unchanged() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let contract = make_test_contract(
            &env,
            &client,
            &freelancer,
            Some(&arbiter),
            ReleaseAuthorization::ClientOnly,
        );
        let snapshot = contract.clone();

        assert_eq!(
            try_require_release_authorization(&other, &contract),
            Err(Error::UnauthorizedRole)
        );

        let release_panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_release_authorization(&env, &other, &contract);
        }));
        let participant_panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_participant(&env, &other, &contract);
        }));
        let admin_panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_admin(&env, &other, &admin);
        }));

        assert!(release_panicked.is_err());
        assert!(participant_panicked.is_err());
        assert!(admin_panicked.is_err());
        assert_eq!(
            contract, snapshot,
            "rejected authorization must not change state"
        );
    }

    /// Boundary: admin authorization is exact-match; any other address fails.
    #[test]
    fn test_boundary_admin_requires_exact_match() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);

        require_admin(&env, &admin, &admin);

        let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_admin(&env, &other, &admin);
        }));
        assert!(rejected.is_err(), "non-admin must be rejected");
    }
}
