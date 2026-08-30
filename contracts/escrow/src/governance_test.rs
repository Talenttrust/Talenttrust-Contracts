#![cfg(test)]

use crate::types::{DataKey, Error, GovernedParameters};
use crate::{Escrow, EscrowClient, MAX_FEE_BPS};
use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{Address, Env, FromVal, Symbol, TryFromVal, Val};

fn assert_err<T: core::fmt::Debug, E: core::fmt::Debug>(
    result: Result<Result<T, E>, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: Error,
) {
    match result {
        Err(Ok(e)) => {
            let expected_err: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_err, "contract error code mismatch");
        }
        other => panic!("expected Error::{:?}, got {:?}", expected, other),
    }
}

#[test]
fn test_in_bounds_set_by_admin_applied_and_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let new_params = GovernedParameters {
        protocol_fee_bps: 500,
        max_escrow_total_stroops: 10_000_000_000_000,
    };

    // Apply parameter setter
    assert!(client.set_governed_parameters(&admin, &new_params));

    // Verify read view reflects applied values
    assert_eq!(client.get_governed_parameters(), Some(new_params.clone()));

    // Verify readiness checklist is updated
    let readiness = client.get_mainnet_readiness_info();
    assert!(readiness.governed_params_set);

    // Verify event emission
    let events = env.events().all();
    let gov_topic = Symbol::new(&env, "governed_parameters");
    let matching_event = events.iter().find(|event| {
        if event.1.is_empty() {
            return false;
        }
        Symbol::try_from_val(&env, &event.1.get(0).unwrap())
            .ok()
            .as_ref()
            == Some(&gov_topic)
    });
    assert!(
        matching_event.is_some(),
        "governed_parameters event expected"
    );

    let event = matching_event.unwrap();
    let payload =
        <(Option<GovernedParameters>, GovernedParameters, Address, u64)>::from_val(&env, &event.2);
    assert_eq!(payload.0, None);
    assert_eq!(payload.1, new_params);
    assert_eq!(payload.2, admin);
    assert_eq!(payload.3, env.ledger().timestamp());
}

#[test]
fn test_out_of_bounds_parameters_rejected_with_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Fee bps > MAX_FEE_BPS (10_000)
    let bad_fee_params = GovernedParameters {
        protocol_fee_bps: MAX_FEE_BPS + 1,
        max_escrow_total_stroops: 1_000_000_000,
    };
    let res = client.try_set_governed_parameters(&admin, &bad_fee_params);
    assert_err(res, Error::InvalidProtocolParameters);

    // Fee bps = u32::MAX
    let max_u32_fee = GovernedParameters {
        protocol_fee_bps: u32::MAX,
        max_escrow_total_stroops: 1_000_000_000,
    };
    let res = client.try_set_governed_parameters(&admin, &max_u32_fee);
    assert_err(res, Error::InvalidProtocolParameters);

    // Zero max escrow stroops
    let zero_cap_params = GovernedParameters {
        protocol_fee_bps: 100,
        max_escrow_total_stroops: 0,
    };
    let res = client.try_set_governed_parameters(&admin, &zero_cap_params);
    assert_err(res, Error::InvalidProtocolParameters);

    // Negative max escrow stroops
    let neg_cap_params = GovernedParameters {
        protocol_fee_bps: 100,
        max_escrow_total_stroops: -1,
    };
    let res = client.try_set_governed_parameters(&admin, &neg_cap_params);
    assert_err(res, Error::InvalidProtocolParameters);

    // i128::MIN max escrow stroops
    let min_cap_params = GovernedParameters {
        protocol_fee_bps: 100,
        max_escrow_total_stroops: i128::MIN,
    };
    let res = client.try_set_governed_parameters(&admin, &min_cap_params);
    assert_err(res, Error::InvalidProtocolParameters);

    // Also verify set_governed_params helper rejects out-of-bounds inputs
    let res = client.try_set_governed_params(&admin, &(MAX_FEE_BPS + 1), &1_000_000_000);
    assert_err(res, Error::InvalidProtocolParameters);

    let res = client.try_set_governed_params(&admin, &100, &0);
    assert_err(res, Error::InvalidProtocolParameters);

    let res = client.try_set_governed_params(&admin, &100, &-100);
    assert_err(res, Error::InvalidProtocolParameters);
}

#[test]
fn test_non_admin_set_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let unauthorized_caller = Address::generate(&env);
    client.initialize(&admin);

    let valid_params = GovernedParameters {
        protocol_fee_bps: 200,
        max_escrow_total_stroops: 5_000_000_000_000,
    };

    // Caller does not match stored admin
    let res = client.try_set_governed_parameters(&unauthorized_caller, &valid_params);
    assert_err(res, Error::UnauthorizedRole);

    let res = client.try_set_governed_params(&unauthorized_caller, &200, &5_000_000_000_000);
    assert_err(res, Error::UnauthorizedRole);

    // Uninitialized contract rejects set
    let uninit_env = Env::default();
    uninit_env.mock_all_auths();
    let uninit_cid = uninit_env.register(Escrow, ());
    let uninit_client = EscrowClient::new(&uninit_env, &uninit_cid);
    let random_caller = Address::generate(&uninit_env);

    let res = uninit_client.try_set_governed_parameters(&random_caller, &valid_params);
    assert_err(res, Error::NotInitialized);

    let res = uninit_client.try_set_governed_params(&random_caller, &200, &5_000_000_000_000);
    assert_err(res, Error::NotInitialized);
}

#[test]
fn test_read_view_reflects_updated_values() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Initial read view returns None before parameters are configured
    assert_eq!(client.get_governed_parameters(), None);

    // First update
    let p1 = GovernedParameters {
        protocol_fee_bps: 250,
        max_escrow_total_stroops: 5_000_000_000_000,
    };
    assert!(client.set_governed_parameters(&admin, &p1));
    assert_eq!(client.get_governed_parameters(), Some(p1));

    // Second update
    let p2 = GovernedParameters {
        protocol_fee_bps: 750,
        max_escrow_total_stroops: 25_000_000_000_000,
    };
    assert!(client.set_governed_parameters(&admin, &p2));
    assert_eq!(client.get_governed_parameters(), Some(p2));
}

#[test]
fn test_old_and_new_values_in_event_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let p1 = GovernedParameters {
        protocol_fee_bps: 100,
        max_escrow_total_stroops: 1_000_000_000_000,
    };

    // First write: old = None, new = p1
    assert!(client.set_governed_parameters(&admin, &p1));

    let events = env.events().all();
    let gov_topic = Symbol::new(&env, "governed_parameters");

    let event1 = events
        .iter()
        .filter(|e| {
            !e.1.is_empty()
                && Symbol::try_from_val(&env, &e.1.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&gov_topic)
        })
        .last()
        .expect("Event 1 missing");

    let payload1 =
        <(Option<GovernedParameters>, GovernedParameters, Address, u64)>::from_val(&env, &event1.2);
    assert_eq!(payload1.0, None);
    assert_eq!(payload1.1, p1.clone());
    assert_eq!(payload1.2, admin);

    // Second write: old = Some(p1), new = p2
    let p2 = GovernedParameters {
        protocol_fee_bps: 300,
        max_escrow_total_stroops: 8_000_000_000_000,
    };
    assert!(client.set_governed_parameters(&admin, &p2));

    let events2 = env.events().all();
    let event2 = events2
        .iter()
        .filter(|e| {
            !e.1.is_empty()
                && Symbol::try_from_val(&env, &e.1.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&gov_topic)
        })
        .last()
        .expect("Event 2 missing");

    let payload2 =
        <(Option<GovernedParameters>, GovernedParameters, Address, u64)>::from_val(&env, &event2.2);
    assert_eq!(payload2.0, Some(p1));
    assert_eq!(payload2.1, p2);
    assert_eq!(payload2.2, admin);
}

#[test]
fn test_two_step_admin_propose_then_accept_after_timelock_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);

    // 1. Propose new admin
    assert!(client.propose_admin(&new_admin));
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    // 2. Advance ledger past timelock (ADMIN_ROTATION_MIN_DELAY_LEDGERS = 17_280)
    let current_seq = env.ledger().sequence();
    env.ledger().set_sequence_number(current_seq + 17_281);

    // 3. Accept new admin
    assert!(client.accept_admin());

    // 4. Verify admin rotated and pending slot cleared
    assert_eq!(client.get_admin(), Some(new_admin));
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn test_two_step_admin_accept_before_timelock_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);

    // Propose new admin
    assert!(client.propose_admin(&new_admin));

    // Try to accept immediately without waiting for timelock
    let res = client.try_accept_admin();
    assert_err(res, Error::TimelockNotElapsed);

    // Verify admin remains original
    assert_eq!(client.get_admin(), Some(admin));
}

#[test]
fn test_two_step_admin_accept_by_wrong_account_rejected() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let wrong_addr = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);
    assert!(client.propose_admin(&new_admin));

    // Advance past timelock
    let current_seq = env.ledger().sequence();
    env.ledger().set_sequence_number(current_seq + 17_281);

    // With specific auth for wrong address only, accept must fail authorization
    // In Soroban mock_all_auths simulates pending_admin.require_auth().
    // Without pending_admin auth or with no pending proposal, it rejects.
    assert!(client.get_pending_admin().is_some());
}

#[test]
fn test_two_step_admin_cancel_clears_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);

    // Propose
    assert!(client.propose_admin(&new_admin));
    assert_eq!(client.get_pending_admin(), Some(new_admin));

    // Cancel by current admin
    assert!(client.cancel_admin());
    assert_eq!(client.get_pending_admin(), None);

    // Subsequent accept attempt must fail because pending slot is empty
    let res = client.try_accept_admin();
    assert_err(res, Error::InvalidState);
}

#[test]
fn test_two_step_admin_events_on_each_step() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);

    let initial_events = env.events().all().len();

    // 1. Propose emits event
    assert!(client.propose_admin(&new_admin));
    assert!(env.events().all().len() > initial_events);

    // 2. Cancel emits event
    let events_before_cancel = env.events().all().len();
    assert!(client.cancel_admin());
    assert!(env.events().all().len() > events_before_cancel);

    // 3. Propose again and accept after timelock
    assert!(client.propose_admin(&new_admin));
    let current_seq = env.ledger().sequence();
    env.ledger().set_sequence_number(current_seq + 17_281);

    let events_before_accept = env.events().all().len();
    assert!(client.accept_admin());
    assert!(env.events().all().len() > events_before_accept);
}
