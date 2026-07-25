#![cfg(test)]

use super::{assert_contract_error, register_client};
use crate::{DataKey, Error, Escrow, EscrowClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_with_admin(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

// -----------------------------------------------------------------------
// Default preserves current behaviour (u32::MAX)
// -----------------------------------------------------------------------

#[test]
fn get_contracts_limit_defaults_to_u32_max() {
    let env = Env::default();
    let (client, _admin) = setup_with_admin(&env);
    assert_eq!(client.get_contracts_limit(), u32::MAX);
}

// -----------------------------------------------------------------------
// Admin can set an in-bounds limit
// -----------------------------------------------------------------------

#[test]
fn admin_can_set_in_bounds_contracts_limit() {
    let env = Env::default();
    let (client, admin) = setup_with_admin(&env);
    env.mock_all_auths();

    assert!(client.set_contracts_limit(&admin, &1000u32));
    assert_eq!(client.get_contracts_limit(), 1000);
}

#[test]
fn set_contracts_limit_emits_event() {
    let env = Env::default();
    let (client, admin) = setup_with_admin(&env);
    env.mock_all_auths();

    client.set_contracts_limit(&admin, &500u32);

    let events = env.events().all();
    assert!(!events.is_empty());
    let contracts_limit_topic = soroban_sdk::Symbol::new(&env, "contracts_limit");
    let found = events.iter().any(|event| {
        event.1.len() > 0
            && soroban_sdk::Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&contracts_limit_topic)
    });
    assert!(found, "contracts_limit event should be emitted");
}

// -----------------------------------------------------------------------
// Reject out-of-range values with typed error
// -----------------------------------------------------------------------

#[test]
fn reject_zero_contracts_limit() {
    let env = Env::default();
    let (client, admin) = setup_with_admin(&env);
    env.mock_all_auths();

    let result = client.try_set_contracts_limit(&admin, &0u32);
    assert_contract_error(result, Error::ContractsLimitExceeded);
}

#[test]
fn admin_can_set_maximum_contracts_limit() {
    let env = Env::default();
    let (client, admin) = setup_with_admin(&env);
    env.mock_all_auths();

    assert!(client.set_contracts_limit(&admin, &u32::MAX));
    assert_eq!(client.get_contracts_limit(), u32::MAX);
}

#[test]
fn admin_can_set_minimum_contracts_limit() {
    let env = Env::default();
    let (client, admin) = setup_with_admin(&env);
    env.mock_all_auths();

    assert!(client.set_contracts_limit(&admin, &1u32));
    assert_eq!(client.get_contracts_limit(), 1);
}

// -----------------------------------------------------------------------
// Non-admin is rejected
// -----------------------------------------------------------------------

#[test]
fn non_admin_rejected_when_uninitialized() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &id);
    let non_admin = Address::generate(&env);

    let result = client.try_set_contracts_limit(&non_admin, &100u32);
    assert_contract_error(result, Error::NotInitialized);
}

#[test]
fn non_admin_rejected_when_initialized() {
    let env = Env::default();
    let (client, _admin) = setup_with_admin(&env);
    env.mock_all_auths();
    let non_admin = Address::generate(&env);

    let result = client.try_set_contracts_limit(&non_admin, &100u32);
    assert_contract_error(result, Error::UnauthorizedRole);
}

// -----------------------------------------------------------------------
// Contracts limit is enforced during create_contract
// -----------------------------------------------------------------------

#[test]
fn create_contract_rejects_when_limit_reached() {
    let env = Env::default();
    let (client, admin) = setup_with_admin(&env);
    env.mock_all_auths();

    assert!(client.set_contracts_limit(&admin, &2u32));

    let (client_a, freelancer_a) = super::generated_participants(&env);
    let (client_b, freelancer_b) = super::generated_participants(&env);
    let milestones = super::default_milestones(&env);

    // First two creates succeed (limit = 2).
    let id1 = client.create_contract(
        &client_a,
        &freelancer_a,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id1, 1);

    let id2 = client.create_contract(
        &client_b,
        &freelancer_b,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id2, 2);

    // Third create must be rejected.
    let (client_c, freelancer_c) = super::generated_participants(&env);
    let result = client.try_create_contract(
        &client_c,
        &freelancer_c,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    assert_contract_error(result, Error::ContractsLimitExceeded);
}

#[test]
fn create_contract_succeeds_at_exact_limit() {
    let env = Env::default();
    let (client, admin) = setup_with_admin(&env);
    env.mock_all_auths();

    assert!(client.set_contracts_limit(&admin, &1u32));

    let (client_a, freelancer_a) = super::generated_participants(&env);
    let milestones = super::default_milestones(&env);

    let id = client.create_contract(
        &client_a,
        &freelancer_a,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);
}

// -----------------------------------------------------------------------
// get_contracts_limit is read-only (no TTL side effects)
// -----------------------------------------------------------------------

#[test]
fn get_contracts_limit_does_not_extend_ttl() {
    let env = Env::default();
    let (client, admin) = setup_with_admin(&env);
    env.mock_all_auths();

    assert!(client.set_contracts_limit(&admin, &100u32));

    let ttl_before: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::ContractsLimit)
            .unwrap()
    });

    let _ = client.get_contracts_limit();

    let ttl_after: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::ContractsLimit)
            .unwrap()
    });

    assert_eq!(ttl_before, ttl_after, "get_contracts_limit must not extend TTL");
}

// -----------------------------------------------------------------------
// Set-then-get round-trip
// -----------------------------------------------------------------------

#[test]
fn set_and_get_contracts_limit_round_trip() {
    let env = Env::default();
    let (client, admin) = setup_with_admin(&env);
    env.mock_all_auths();

    for limit in [1u32, 100, 1000, u32::MAX] {
        assert!(client.set_contracts_limit(&admin, &limit));
        assert_eq!(client.get_contracts_limit(), limit);
    }
}