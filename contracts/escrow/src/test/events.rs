//! Event payload and ordering tests.
//!
//! Each test verifies:
//!
//! 1. **Existence** — the expected event is present in `env.events().all()`.
//! 2. **Topic correctness** — both namespace and operation symbols match.
//! 3. **Payload correctness** — every field decoded from the data tuple matches.
//! 4. **Ordering** — when multiple events are emitted (e.g. `release` + `complete`),
//!    their relative order is exact.
//! 5. **Absence** — failed operations must emit *no* events.

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    vec, Address, Env, Symbol, TryFromVal,
};

use super::{
    default_milestones, generated_participants, register_client, total_milestone_amount,
    MILESTONE_ONE, MILESTONE_THREE, MILESTONE_TWO,
};
use crate::{Escrow, EscrowClient};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Registers a fresh contract instance and returns both its client and its on-chain
/// address (needed for `assert_eq!(contract, addr)` checks).
fn fresh(env: &Env) -> (EscrowClient<'_>, Address) {
    let addr = env.register(Escrow, ());
    (EscrowClient::new(env, &addr), addr)
}

/// Asserts that an event's `topics[0]` == `ns_sym` and `topics[1]` == `op_sym`.
fn assert_topics(
    env: &Env,
    event_topics: &soroban_sdk::Vec<soroban_sdk::Val>,
    ns: Symbol,
    op: Symbol,
) {
    assert_eq!(
        event_topics.len(),
        2,
        "events must have exactly 2 topics (namespace, operation)"
    );
    let t0 = Symbol::try_from_val(env, &event_topics.get(0).unwrap()).unwrap();
    let t1 = Symbol::try_from_val(env, &event_topics.get(1).unwrap()).unwrap();
    assert_eq!(t0, ns, "topic[0] (namespace) mismatch");
    assert_eq!(t1, op, "topic[1] (operation) mismatch");
}

// ════════════════════════════════════════════════════════════════════════════
// Pause-control events
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_initialize_emits_pause_init() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(&env, &topics, symbol_short!("pause"), symbol_short!("init"));
    // data = admin address
    assert_eq!(Address::try_from_val(&env, &data).unwrap(), admin);
}

#[test]
fn test_pause_emits_pause_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.pause();

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(
        &env,
        &topics,
        symbol_short!("pause"),
        symbol_short!("pause"),
    );
    assert_eq!(Address::try_from_val(&env, &data).unwrap(), admin);
}

#[test]
fn test_unpause_emits_pause_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();

    client.unpause();

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(
        &env,
        &topics,
        symbol_short!("pause"),
        symbol_short!("unpause"),
    );
    assert_eq!(Address::try_from_val(&env, &data).unwrap(), admin);
}

#[test]
fn test_activate_emergency_emits_pause_emerg() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.activate_emergency_pause();

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(
        &env,
        &topics,
        symbol_short!("pause"),
        symbol_short!("emerg"),
    );
    assert_eq!(Address::try_from_val(&env, &data).unwrap(), admin);
}

#[test]
fn test_resolve_emergency_emits_pause_resolv() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.activate_emergency_pause();

    client.resolve_emergency();

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(
        &env,
        &topics,
        symbol_short!("pause"),
        symbol_short!("resolv"),
    );
    assert_eq!(Address::try_from_val(&env, &data).unwrap(), admin);
}

// ════════════════════════════════════════════════════════════════════════════
// Governance events
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_initialize_governance_emits_gov_init() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let admin = Address::generate(&env);

    client.initialize_protocol_governance(&admin, &1_i128, &16_u32, &1_i128, &5_i128);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(&env, &topics, symbol_short!("gov"), symbol_short!("init"));
    assert_eq!(Address::try_from_val(&env, &data).unwrap(), admin);
}

#[test]
fn test_update_parameters_emits_gov_params_with_correct_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let admin = Address::generate(&env);
    client.initialize_protocol_governance(&admin, &1_i128, &16_u32, &1_i128, &5_i128);

    client.update_protocol_parameters(&10_i128, &8_u32, &2_i128, &4_i128);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(&env, &topics, symbol_short!("gov"), symbol_short!("params"));
    // payload: (min_milestone_amount, max_milestones, min_rating, max_rating)
    let (min_m, max_m, min_r, max_r): (i128, u32, i128, i128) =
        <(i128, u32, i128, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(min_m, 10);
    assert_eq!(max_m, 8);
    assert_eq!(min_r, 2);
    assert_eq!(max_r, 4);
}

#[test]
fn test_propose_governance_admin_emits_gov_propose() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize_protocol_governance(&admin, &1_i128, &16_u32, &1_i128, &5_i128);

    client.propose_governance_admin(&new_admin);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(
        &env,
        &topics,
        symbol_short!("gov"),
        symbol_short!("propose"),
    );
    assert_eq!(Address::try_from_val(&env, &data).unwrap(), new_admin);
}

#[test]
fn test_accept_governance_admin_emits_gov_accept() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize_protocol_governance(&admin, &1_i128, &16_u32, &1_i128, &5_i128);
    client.propose_governance_admin(&new_admin);

    client.accept_governance_admin();

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(&env, &topics, symbol_short!("gov"), symbol_short!("accept"));
    assert_eq!(Address::try_from_val(&env, &data).unwrap(), new_admin);
}

// ════════════════════════════════════════════════════════════════════════════
// Escrow core events
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_create_contract_emits_escrow_create_with_correct_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let milestones = default_milestones(&env);
    let expected_total = total_milestone_amount();

    client.create_contract(&c, &f, &milestones);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(
        &env,
        &topics,
        symbol_short!("escrow"),
        symbol_short!("create"),
    );
    // payload: (contract_id: u32, client: Address, freelancer: Address, total: i128)
    let (id, client_ev, freelancer_ev, total): (u32, Address, Address, i128) =
        <(u32, Address, Address, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(id, 1_u32);
    assert_eq!(client_ev, c);
    assert_eq!(freelancer_ev, f);
    assert_eq!(total, expected_total);
}

#[test]
fn test_create_contract_id_increments_in_event_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let milestones = default_milestones(&env);

    client.create_contract(&c, &f, &milestones);
    client.create_contract(&c, &f, &milestones);

    // The last invocation emitted a single event with contract_id == 2.
    let events = env.events().all();
    let (_, _, data) = events.get(0).unwrap();
    let (id, _, _, _): (u32, Address, Address, i128) =
        <(u32, Address, Address, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(id, 2_u32);
}

#[test]
fn test_deposit_funds_emits_escrow_deposit_with_correct_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));
    let deposit_amount = MILESTONE_ONE;

    client.deposit_funds(&contract_id, &deposit_amount);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(
        &env,
        &topics,
        symbol_short!("escrow"),
        symbol_short!("deposit"),
    );
    // payload: (contract_id, amount, funded_amount)
    let (id, amount, funded): (u32, i128, i128) =
        <(u32, i128, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(id, contract_id);
    assert_eq!(amount, deposit_amount);
    assert_eq!(funded, deposit_amount); // first deposit, so funded == amount
}

#[test]
fn test_deposit_funded_amount_accumulates_in_event_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));

    // First deposit covers MILESTONE_ONE
    client.deposit_funds(&contract_id, &MILESTONE_ONE);
    // Second deposit covers MILESTONE_TWO
    client.deposit_funds(&contract_id, &MILESTONE_TWO);

    // Last invocation's event shows accumulated funded_amount.
    let events = env.events().all();
    let (_, _, data) = events.get(0).unwrap();
    let (_, amount, funded): (u32, i128, i128) =
        <(u32, i128, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(amount, MILESTONE_TWO);
    assert_eq!(funded, MILESTONE_ONE + MILESTONE_TWO);
}

#[test]
fn test_release_milestone_emits_escrow_release_with_correct_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_milestone_amount());

    // Release milestone 0 (NOT the last one).
    client.release_milestone(&contract_id, &0);

    let events = env.events().all();
    assert_eq!(
        events.len(),
        1,
        "non-final release must emit exactly 1 event"
    );
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(
        &env,
        &topics,
        symbol_short!("escrow"),
        symbol_short!("release"),
    );
    // payload: (contract_id, milestone_id, amount)
    let (id, mid, amount): (u32, u32, i128) =
        <(u32, u32, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(id, contract_id);
    assert_eq!(mid, 0_u32);
    assert_eq!(amount, MILESTONE_ONE);
}

#[test]
fn test_release_last_milestone_emits_release_then_complete_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_milestone_amount());
    client.release_milestone(&contract_id, &0);
    client.release_milestone(&contract_id, &1);

    // Final milestone — must emit `release` then `complete` in that order.
    client.release_milestone(&contract_id, &2);

    let events = env.events().all();
    assert_eq!(
        events.len(),
        2,
        "final release must emit both 'release' and 'complete'"
    );

    // First: release event.
    let (c0, t0, d0) = events.get(0).unwrap();
    assert_eq!(c0, addr);
    assert_topics(&env, &t0, symbol_short!("escrow"), symbol_short!("release"));
    let (id0, mid0, amount0): (u32, u32, i128) =
        <(u32, u32, i128)>::try_from_val(&env, &d0).unwrap();
    assert_eq!(id0, contract_id);
    assert_eq!(mid0, 2_u32);
    assert_eq!(amount0, MILESTONE_THREE);

    // Second: complete event.
    let (c1, t1, d1) = events.get(1).unwrap();
    assert_eq!(c1, addr);
    assert_topics(
        &env,
        &t1,
        symbol_short!("escrow"),
        symbol_short!("complete"),
    );
    let complete_id = u32::try_from_val(&env, &d1).unwrap();
    assert_eq!(complete_id, contract_id);
}

#[test]
fn test_single_milestone_contract_release_emits_both_release_and_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let milestones = vec![&env, 500_i128];
    let contract_id = client.create_contract(&c, &f, &milestones);
    client.deposit_funds(&contract_id, &500_i128);

    // Only one milestone → both events on first release.
    client.release_milestone(&contract_id, &0);

    let events = env.events().all();
    assert_eq!(events.len(), 2);

    let (_, t0, d0) = events.get(0).unwrap();
    assert_topics(&env, &t0, symbol_short!("escrow"), symbol_short!("release"));
    let (id, mid, amount): (u32, u32, i128) = <(u32, u32, i128)>::try_from_val(&env, &d0).unwrap();
    assert_eq!(id, contract_id);
    assert_eq!(mid, 0_u32);
    assert_eq!(amount, 500_i128);

    let (c1, t1, d1) = events.get(1).unwrap();
    assert_eq!(c1, addr);
    assert_topics(
        &env,
        &t1,
        symbol_short!("escrow"),
        symbol_short!("complete"),
    );
    assert_eq!(u32::try_from_val(&env, &d1).unwrap(), contract_id);
}

#[test]
fn test_issue_reputation_emits_escrow_rep_with_correct_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, addr) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_milestone_amount());
    for i in 0_u32..3 {
        client.release_milestone(&contract_id, &i);
    }

    client.issue_reputation(&contract_id, &5);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let (contract, topics, data) = events.get(0).unwrap();
    assert_eq!(contract, addr);
    assert_topics(&env, &topics, symbol_short!("escrow"), symbol_short!("rep"));
    // payload: (contract_id, freelancer, rating)
    let (id, freelancer_ev, rating): (u32, Address, i128) =
        <(u32, Address, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(id, contract_id);
    assert_eq!(freelancer_ev, f);
    assert_eq!(rating, 5_i128);
}

// ════════════════════════════════════════════════════════════════════════════
// Absence guarantees: failed operations must emit NO events
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_failed_create_emits_no_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (addr, _) = generated_participants(&env);

    // Same address for client and freelancer → InvalidParticipants.
    let _ = client.try_create_contract(&addr, &addr, &default_milestones(&env));

    assert_eq!(env.events().all().len(), 0);
}

#[test]
fn test_failed_deposit_emits_no_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);

    // Contract 99 does not exist → ContractNotFound.
    let _ = client.try_deposit_funds(&99_u32, &100_i128);

    assert_eq!(env.events().all().len(), 0);
}

#[test]
fn test_failed_release_emits_no_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));

    // Not funded → InvalidState.
    let _ = client.try_release_milestone(&contract_id, &0);

    assert_eq!(env.events().all().len(), 0);
}

#[test]
fn test_failed_reputation_emits_no_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));

    // Contract not completed → InvalidState.
    let _ = client.try_issue_reputation(&contract_id, &5);

    assert_eq!(env.events().all().len(), 0);
}

// ════════════════════════════════════════════════════════════════════════════
// Full-flow ordering: validate each event phase in sequence
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_all_escrow_events_emitted_in_order_across_full_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let milestones = vec![&env, 100_i128];

    // create → 1 event
    let contract_id = client.create_contract(&c, &f, &milestones);
    {
        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (_, t, _) = events.get(0).unwrap();
        assert_topics(&env, &t, symbol_short!("escrow"), symbol_short!("create"));
    }

    // deposit → 1 event
    client.deposit_funds(&contract_id, &100_i128);
    {
        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (_, t, _) = events.get(0).unwrap();
        assert_topics(&env, &t, symbol_short!("escrow"), symbol_short!("deposit"));
    }

    // release (last milestone) → 2 events: release + complete
    client.release_milestone(&contract_id, &0);
    {
        let events = env.events().all();
        assert_eq!(events.len(), 2);
        let (_, t0, _) = events.get(0).unwrap();
        let (_, t1, _) = events.get(1).unwrap();
        assert_topics(&env, &t0, symbol_short!("escrow"), symbol_short!("release"));
        assert_topics(
            &env,
            &t1,
            symbol_short!("escrow"),
            symbol_short!("complete"),
        );
    }

    // reputation → 1 event
    client.issue_reputation(&contract_id, &5);
    {
        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (_, t, _) = events.get(0).unwrap();
        assert_topics(&env, &t, symbol_short!("escrow"), symbol_short!("rep"));
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Payload field detail tests
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn test_release_payload_contains_correct_milestone_amount_per_index() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_milestone_amount());

    // Milestone 0 = MILESTONE_ONE.
    client.release_milestone(&contract_id, &0);
    {
        let events = env.events().all();
        let (_, _, data) = events.get(0).unwrap();
        let (id, mid, amount): (u32, u32, i128) =
            <(u32, u32, i128)>::try_from_val(&env, &data).unwrap();
        assert_eq!(id, contract_id);
        assert_eq!(mid, 0_u32);
        assert_eq!(amount, MILESTONE_ONE);
    }

    // Milestone 1 = MILESTONE_TWO.
    client.release_milestone(&contract_id, &1);
    {
        let events = env.events().all();
        let (_, _, data) = events.get(0).unwrap();
        let (id, mid, amount): (u32, u32, i128) =
            <(u32, u32, i128)>::try_from_val(&env, &data).unwrap();
        assert_eq!(id, contract_id);
        assert_eq!(mid, 1_u32);
        assert_eq!(amount, MILESTONE_TWO);
    }
}

#[test]
fn test_reputation_event_contains_freelancer_address_and_rating() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));
    client.deposit_funds(&contract_id, &total_milestone_amount());
    for i in 0_u32..3 {
        client.release_milestone(&contract_id, &i);
    }

    client.issue_reputation(&contract_id, &4);

    let events = env.events().all();
    let (_, _, data) = events.get(0).unwrap();
    let (id, freelancer, rating): (u32, Address, i128) =
        <(u32, Address, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(id, contract_id);
    assert_eq!(freelancer, f);
    assert_eq!(rating, 4_i128);
}

#[test]
fn test_governance_params_event_reflects_updated_values() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let admin = Address::generate(&env);
    client.initialize_protocol_governance(&admin, &1_i128, &16_u32, &1_i128, &5_i128);

    client.update_protocol_parameters(&50_i128, &10_u32, &2_i128, &5_i128);

    let events = env.events().all();
    let (_, _, data) = events.get(0).unwrap();
    let (min_m, max_m, min_r, max_r): (i128, u32, i128, i128) =
        <(i128, u32, i128, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(min_m, 50_i128);
    assert_eq!(max_m, 10_u32);
    assert_eq!(min_r, 2_i128);
    assert_eq!(max_r, 5_i128);
}

#[test]
fn test_deposit_event_shows_partial_then_full_funding() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let contract_id = client.create_contract(&c, &f, &default_milestones(&env));

    // Partial deposit.
    client.deposit_funds(&contract_id, &MILESTONE_ONE);
    {
        let events = env.events().all();
        let (_, _, data) = events.get(0).unwrap();
        let (_, _, funded): (u32, i128, i128) =
            <(u32, i128, i128)>::try_from_val(&env, &data).unwrap();
        assert_eq!(
            funded, MILESTONE_ONE,
            "partial funded_amount after first deposit"
        );
    }

    // Second deposit brings total to MILESTONE_ONE + MILESTONE_TWO.
    client.deposit_funds(&contract_id, &MILESTONE_TWO);
    {
        let events = env.events().all();
        let (_, _, data) = events.get(0).unwrap();
        let (_, _, funded): (u32, i128, i128) =
            <(u32, i128, i128)>::try_from_val(&env, &data).unwrap();
        assert_eq!(funded, MILESTONE_ONE + MILESTONE_TWO);
    }
}

#[test]
fn test_complete_event_data_is_just_the_contract_id() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = fresh(&env);
    let (c, f) = generated_participants(&env);
    let milestones = vec![&env, 200_i128];
    let contract_id = client.create_contract(&c, &f, &milestones);
    client.deposit_funds(&contract_id, &200_i128);

    client.release_milestone(&contract_id, &0);

    let events = env.events().all();
    assert_eq!(events.len(), 2);
    // complete event is at index 1.
    let (_, t1, d1) = events.get(1).unwrap();
    assert_topics(
        &env,
        &t1,
        symbol_short!("escrow"),
        symbol_short!("complete"),
    );
    // data is just the u32 contract_id (not a tuple).
    let complete_id = u32::try_from_val(&env, &d1).unwrap();
    assert_eq!(complete_id, contract_id);
}
