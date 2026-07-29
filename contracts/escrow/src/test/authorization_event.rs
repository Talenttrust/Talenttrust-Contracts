//! Tests for `set_release_authorization` and its `auth_chg` indexed event.
//!
//! # Coverage
//!
//! ## Happy-path (payload correctness)
//! 1. `test_auth_chg_event_emitted` — verifies the `auth_chg` event is emitted
//!    after a valid mode change and that the `contract_id` topic is correct.
//! 2. `test_auth_chg_event_payload_old_new` — asserts that `old_auth` and
//!    `new_auth` in the payload match the discriminants of the changed modes.
//! 3. `test_auth_chg_all_mode_transitions` — exercises every `ClientOnly → X`
//!    and `X → ClientOnly` round-trip to confirm the discriminant mapping
//!    (`0=ClientOnly, 1=ClientAndArbiter, 2=ArbiterOnly, 3=MultiSig`) is stable.
//! 4. `test_auth_chg_storage_updated` — confirms `get_contract` reflects the
//!    new authorization after the call.
//!
//! ## No-topic-collision
//! 5. `test_auth_chg_topic_no_collision` — asserts the `auth_chg` topic
//!    is distinct from every other topic used by the contract.
//!
//! ## Edge cases
//! 6. `test_auth_chg_same_mode_still_emits` — even a no-op change (same mode)
//!    records an event so indexers see the explicit acknowledgement.
//! 7. `test_auth_chg_multiple_changes_ordered` — consecutive changes produce
//!    events in declaration order with correct old/new discriminants.
//!
//! ## Negative (fail-closed)
//! 8. `test_auth_chg_rejects_non_client` — freelancer and arbiter callers are
//!    rejected with `UnauthorizedRole`.
//! 9. `test_auth_chg_rejects_terminal_state` — changes in `Completed`,
//!    `Cancelled`, and `Refunded` states are rejected with `InvalidState`.
//! 10. `test_auth_chg_rejects_contract_not_found` — unknown `contract_id`
//!     panics with `ContractNotFound`.

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Env, FromVal, Symbol, TryFromVal,
};

use super::register_client;
use crate::{ContractStatus, DataKey, Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization, Contract};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Register and initialize the escrow contract, returning a client.
fn make_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    client
}

/// Create a minimal contract using `ClientOnly` auth and return
/// `(contract_id, client_addr, freelancer_addr, arbiter_addr)`.
fn create_base_contract(
    env: &Env,
    client: &EscrowClient<'_>,
) -> (u32, Address, Address, Address) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    let milestones = vec![env, 1_000_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    (contract_id, client_addr, freelancer_addr, arbiter_addr)
}

/// Collect the `auth_chg` topic symbol for comparison.
fn auth_chg_topic(env: &Env) -> Symbol {
    soroban_sdk::symbol_short!("auth_chg")
}

/// Returns whether any event in `env` has `symbol_short!("auth_chg")` as its
/// first topic and `contract_id` as its second topic.
fn has_auth_chg_event_for(env: &Env, contract_id: u32) -> bool {
    let topic = auth_chg_topic(env);
    env.events().all().iter().any(|evt| {
        let topics = &evt.1;
        if topics.len() < 2 {
            return false;
        }
        let t0 = Symbol::try_from_val(env, &topics.get(0).unwrap()).ok();
        let t1 = u32::try_from_val(env, &topics.get(1).unwrap()).ok();
        t0.as_ref() == Some(&topic) && t1 == Some(contract_id)
    })
}

/// Assert that a `try_*` result is the expected contract-level error.
fn assert_contract_err<T: core::fmt::Debug, IE: core::fmt::Debug, E: Into<soroban_sdk::Error> + core::fmt::Debug>(
    result: Result<Result<T, IE>, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: E,
) {
    match result {
        Err(Ok(e)) => {
            let expected_err: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_err, "contract error code mismatch");
        }
        _other => panic!(
            "expected contract error {:?}, got {:?}",
            expected, _other
        ),
    }
}

// ---------------------------------------------------------------------------
// 1. Happy-path: event is emitted
// ---------------------------------------------------------------------------

#[test]
fn test_auth_chg_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();

    let client = make_client(&env);
    let (contract_id, client_addr, _, _) = create_base_contract(&env, &client);

    let result = client.set_release_authorization(
        &contract_id,
        &client_addr,
        &ReleaseAuthorization::MultiSig,
    );
    assert!(result, "set_release_authorization should return true");

    assert!(
        has_auth_chg_event_for(&env, contract_id),
        "expected auth_chg event for contract_id={contract_id}"
    );
}

// ---------------------------------------------------------------------------
// 2. Payload: old_auth and new_auth discriminants
// ---------------------------------------------------------------------------

#[test]
fn test_auth_chg_event_payload_old_new() {
    let env = Env::default();
    env.mock_all_auths();

    let client = make_client(&env);
    let (contract_id, client_addr, _, _) = create_base_contract(&env, &client);

    // ClientOnly (0) → MultiSig (3)
    client.set_release_authorization(
        &contract_id,
        &client_addr,
        &ReleaseAuthorization::MultiSig,
    );

    let topic = auth_chg_topic(&env);

    let mut found = false;
    for evt in env.events().all().iter() {
        let topics = &evt.1;
        if topics.len() < 2 {
            continue;
        }
        let t0 = Symbol::try_from_val(&env, &topics.get(0).unwrap()).ok();
        let t1 = u32::try_from_val(&env, &topics.get(1).unwrap()).ok();
        if t0.as_ref() != Some(&topic) || t1 != Some(contract_id) {
            continue;
        }
        // Decode the payload: (old_auth: u32, new_auth: u32, caller: Address, timestamp: u64)
        let data = &evt.2;
        let old_auth = u32::try_from_val(&env, &data.get(0).unwrap()).expect("old_auth");
        let new_auth = u32::try_from_val(&env, &data.get(1).unwrap()).expect("new_auth");
        assert_eq!(old_auth, ReleaseAuthorization::ClientOnly as u32, "old_auth mismatch");
        assert_eq!(new_auth, ReleaseAuthorization::MultiSig as u32, "new_auth mismatch");
        found = true;
    }
    assert!(found, "auth_chg event not found in event log");
}

// ---------------------------------------------------------------------------
// 3. All mode transitions
// ---------------------------------------------------------------------------

#[test]
fn test_auth_chg_all_mode_transitions() {
    // Discriminants: ClientOnly=0, ClientAndArbiter=1, ArbiterOnly=2, MultiSig=3
    let transitions: &[(ReleaseAuthorization, ReleaseAuthorization)] = &[
        (ReleaseAuthorization::ClientOnly,      ReleaseAuthorization::ClientAndArbiter),
        (ReleaseAuthorization::ClientAndArbiter, ReleaseAuthorization::ArbiterOnly),
        (ReleaseAuthorization::ArbiterOnly,     ReleaseAuthorization::MultiSig),
        (ReleaseAuthorization::MultiSig,        ReleaseAuthorization::ClientOnly),
    ];

    for (from, to) in transitions {
        let env = Env::default();
        env.mock_all_auths();

        let client = make_client(&env);
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let arbiter_addr = Address::generate(&env);
        let milestones = vec![&env, 500_i128];

        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &Some(arbiter_addr.clone()),
            &milestones,
            from,
        );

        client.set_release_authorization(&contract_id, &client_addr, to);

        let topic = auth_chg_topic(&env);
        let mut found = false;
        for evt in env.events().all().iter() {
            let topics = &evt.1;
            if topics.len() < 2 {
                continue;
            }
            let t0 = Symbol::try_from_val(&env, &topics.get(0).unwrap()).ok();
            let t1 = u32::try_from_val(&env, &topics.get(1).unwrap()).ok();
            if t0.as_ref() != Some(&topic) || t1 != Some(contract_id) {
                continue;
            }
            let data = &evt.2;
            let old_auth = u32::try_from_val(&env, &data.get(0).unwrap()).expect("old_auth");
            let new_auth = u32::try_from_val(&env, &data.get(1).unwrap()).expect("new_auth");
            assert_eq!(old_auth, *from as u32, "old_auth mismatch for transition {:?} → {:?}", from, to);
            assert_eq!(new_auth, *to as u32, "new_auth mismatch for transition {:?} → {:?}", from, to);
            found = true;
        }
        assert!(found, "auth_chg event not found for transition {:?} → {:?}", from, to);
    }
}

// ---------------------------------------------------------------------------
// 4. Storage is updated
// ---------------------------------------------------------------------------

#[test]
fn test_auth_chg_storage_updated() {
    let env = Env::default();
    env.mock_all_auths();

    let client = make_client(&env);
    let (contract_id, client_addr, _, _) = create_base_contract(&env, &client);

    client.set_release_authorization(
        &contract_id,
        &client_addr,
        &ReleaseAuthorization::ArbiterOnly,
    );

    let stored = client.get_contract(&contract_id);
    assert_eq!(
        stored.release_authorization,
        ReleaseAuthorization::ArbiterOnly,
        "storage must reflect the new authorization mode"
    );
}

// ---------------------------------------------------------------------------
// 5. No topic collision
// ---------------------------------------------------------------------------

/// The `auth_chg` topic must not appear in the set of topics emitted by any
/// other entrypoint. We exercise a variety of entrypoints and confirm that
/// `auth_chg` only appears after an actual `set_release_authorization` call.
#[test]
fn test_auth_chg_topic_no_collision() {
    let known_other_topics: &[&str] = &[
        "created",
        "mlstn_rls",
        "ctrct_cmp",
        "ctrct_st",
        "refunded",
        "cancelled",
        "dispute",
        "fee",
        "pause",
        "unpaused",
        "emergency",
        "init",
        "limits",
    ];

    let env = Env::default();
    let auth_chg = auth_chg_topic(&env);

    for other in known_other_topics {
        let other_sym = Symbol::new(&env, other);
        assert_ne!(
            auth_chg, other_sym,
            "`auth_chg` topic collides with `{other}`"
        );
    }

    // Verify auth_chg is absent from events produced by non-authorization entrypoints.
    env.mock_all_auths();
    let client = make_client(&env);
    let (contract_id, client_addr, _, _) = create_base_contract(&env, &client);

    // Trigger an event from a non-related entrypoint (pause/unpause cycle).
    let _ = client.pause();
    let _ = client.unpause();

    // auth_chg must NOT appear in events emitted so far.
    let has_auth_chg = env.events().all().iter().any(|evt| {
        let topics = &evt.1;
        if topics.is_empty() {
            return false;
        }
        Symbol::try_from_val(&env, &topics.get(0).unwrap())
            .ok()
            .as_ref()
            == Some(&auth_chg)
    });
    assert!(
        !has_auth_chg,
        "auth_chg topic must not appear before set_release_authorization is called"
    );

    // Now emit one and confirm it appears.
    client.set_release_authorization(
        &contract_id,
        &client_addr,
        &ReleaseAuthorization::MultiSig,
    );
    assert!(
        has_auth_chg_event_for(&env, contract_id),
        "auth_chg must appear after set_release_authorization"
    );
}

// ---------------------------------------------------------------------------
// 6. Same-mode change still emits an event
// ---------------------------------------------------------------------------

#[test]
fn test_auth_chg_same_mode_still_emits() {
    let env = Env::default();
    env.mock_all_auths();

    let client = make_client(&env);
    let (contract_id, client_addr, _, _) = create_base_contract(&env, &client);

    // ClientOnly → ClientOnly (no-op change)
    client.set_release_authorization(
        &contract_id,
        &client_addr,
        &ReleaseAuthorization::ClientOnly,
    );

    let topic = auth_chg_topic(&env);
    let mut found = false;
    for evt in env.events().all().iter() {
        let topics = &evt.1;
        if topics.len() < 2 {
            continue;
        }
        let t0 = Symbol::try_from_val(&env, &topics.get(0).unwrap()).ok();
        let t1 = u32::try_from_val(&env, &topics.get(1).unwrap()).ok();
        if t0.as_ref() != Some(&topic) || t1 != Some(contract_id) {
            continue;
        }
        let data = &evt.2;
        let old_auth = u32::try_from_val(&env, &data.get(0).unwrap()).expect("old_auth");
        let new_auth = u32::try_from_val(&env, &data.get(1).unwrap()).expect("new_auth");
        // Both should be ClientOnly == 0
        assert_eq!(old_auth, 0u32, "old_auth for same-mode change");
        assert_eq!(new_auth, 0u32, "new_auth for same-mode change");
        found = true;
    }
    assert!(found, "auth_chg event must fire even for same-mode changes");
}

// ---------------------------------------------------------------------------
// 7. Multiple consecutive changes — correct ordering
// ---------------------------------------------------------------------------

#[test]
fn test_auth_chg_multiple_changes_ordered() {
    let env = Env::default();
    env.mock_all_auths();

    let client = make_client(&env);
    let (contract_id, client_addr, _, _) = create_base_contract(&env, &client);

    // ClientOnly(0) → ClientAndArbiter(1) → MultiSig(3)
    client.set_release_authorization(
        &contract_id,
        &client_addr,
        &ReleaseAuthorization::ClientAndArbiter,
    );
    client.set_release_authorization(
        &contract_id,
        &client_addr,
        &ReleaseAuthorization::MultiSig,
    );

    let topic = auth_chg_topic(&env);
    let auth_events: Vec<(u32, u32)> = env
        .events()
        .all()
        .iter()
        .filter_map(|evt| {
            let topics = &evt.1;
            if topics.len() < 2 {
                return None;
            }
            let t0 = Symbol::try_from_val(&env, &topics.get(0).unwrap()).ok()?;
            let t1 = u32::try_from_val(&env, &topics.get(1).unwrap()).ok()?;
            if t0 != topic || t1 != contract_id {
                return None;
            }
            let data = &evt.2;
            let old = u32::try_from_val(&env, &data.get(0).unwrap()).ok()?;
            let new_ = u32::try_from_val(&env, &data.get(1).unwrap()).ok()?;
            Some((old, new_))
        })
        .collect();

    assert_eq!(auth_events.len(), 2, "expected exactly 2 auth_chg events");
    // First: ClientOnly(0) → ClientAndArbiter(1)
    assert_eq!(auth_events[0], (0, 1), "first auth_chg pair mismatch");
    // Second: ClientAndArbiter(1) → MultiSig(3)
    assert_eq!(auth_events[1], (1, 3), "second auth_chg pair mismatch");
}

// ---------------------------------------------------------------------------
// 8. Negative: non-client callers are rejected
// ---------------------------------------------------------------------------

#[test]
fn test_auth_chg_rejects_non_client() {
    let env = Env::default();
    env.mock_all_auths();

    let client = make_client(&env);
    let (contract_id, _client_addr, freelancer_addr, arbiter_addr) =
        create_base_contract(&env, &client);

    // Freelancer attempt
    let result = client.try_set_release_authorization(
        &contract_id,
        &freelancer_addr,
        &ReleaseAuthorization::MultiSig,
    );
    assert_contract_err(result, Error::UnauthorizedRole);

    // Arbiter attempt
    let result = client.try_set_release_authorization(
        &contract_id,
        &arbiter_addr,
        &ReleaseAuthorization::MultiSig,
    );
    assert_contract_err(result, Error::UnauthorizedRole);

    // Random stranger
    let stranger = Address::generate(&env);
    let result = client.try_set_release_authorization(
        &contract_id,
        &stranger,
        &ReleaseAuthorization::MultiSig,
    );
    assert_contract_err(result, Error::UnauthorizedRole);
}

// ---------------------------------------------------------------------------
// 9. Negative: terminal states are rejected
// ---------------------------------------------------------------------------

/// Directly write a contract record in a terminal state to persistent storage
/// and verify that `set_release_authorization` rejects it.
#[test]
fn test_auth_chg_rejects_terminal_state() {
    let terminal_states = [
        ContractStatus::Completed,
        ContractStatus::Cancelled,
        ContractStatus::Refunded,
        ContractStatus::Disputed,
    ];

    for terminal_status in &terminal_states {
        let env = Env::default();
        env.mock_all_auths();

        let client_addr_ext = Address::generate(&env);
        let freelancer_addr_ext = Address::generate(&env);
        let arbiter_addr_ext = Address::generate(&env);

        let escrow_addr = env.register(Escrow, ());
        let escrow = EscrowClient::new(&env, &escrow_addr);
        let admin = Address::generate(&env);
        escrow.initialize(&admin);

        // Create contract normally (gets Created status).
        let milestones = vec![&env, 500_i128];
        let contract_id = escrow.create_contract(
            &client_addr_ext,
            &freelancer_addr_ext,
            &Some(arbiter_addr_ext.clone()),
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );

        // Forcefully set a terminal status in storage.
        env.as_contract(&escrow_addr, || {
            let mut record: Contract = env
                .storage()
                .persistent()
                .get(&DataKey::Contract(contract_id))
                .unwrap();
            record.status = *terminal_status;
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), &record);
        });

        let result = escrow.try_set_release_authorization(
            &contract_id,
            &client_addr_ext,
            &ReleaseAuthorization::MultiSig,
        );
        assert_contract_err(
            result,
            Error::InvalidState,
        );
    }
}

// ---------------------------------------------------------------------------
// 10. Negative: unknown contract id
// ---------------------------------------------------------------------------

#[test]
fn test_auth_chg_rejects_contract_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let client = make_client(&env);
    let caller = Address::generate(&env);

    let result = client.try_set_release_authorization(
        &99_999u32,
        &caller,
        &ReleaseAuthorization::MultiSig,
    );
    assert_contract_err(result, Error::ContractNotFound);
}
