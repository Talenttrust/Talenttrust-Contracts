#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _, LedgerInfo},
    Address, Env, Symbol, TryFromVal,
};

use crate::{Escrow, EscrowClient, ADMIN_ROTATION_MIN_DELAY_LEDGERS};

fn register_initialized(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

fn advance_ledgers(env: &Env, delta: u32) {
    let info = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: info.sequence_number + delta,
        timestamp: info.timestamp + (delta as u64) * 5,
        protocol_version: info.protocol_version,
        network_id: info.network_id,
        base_reserve: info.base_reserve,
        min_temp_entry_ttl: info.min_temp_entry_ttl,
        min_persistent_entry_ttl: info.min_persistent_entry_ttl,
        max_entry_ttl: info.max_entry_ttl,
    });
}

#[test]
fn protocol_fee_bps_change_emits_event() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);

    // Change protocol fee bps
    assert!(client.set_protocol_fee_bps(&100u32));

    let events = env.events().all();
    assert!(events.len() > 0);

    // Ensure an event with the protocol_fee_bps topic exists
    let fee_topic = soroban_sdk::Symbol::new(&env, "protocol_fee_bps");
    let found = events.iter().any(|event| {
        event.1.len() > 0
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&fee_topic)
    });
    assert!(found);
}

#[test]
fn admin_propose_and_accept_emit_events() {
    let env = Env::default();
    let (client, _admin) = register_initialized(&env);

    let next_admin = Address::generate(&env);
    client.propose_governance_admin(&next_admin);

    // Advance past timelock before accepting
    advance_ledgers(&env, ADMIN_ROTATION_MIN_DELAY_LEDGERS);

    // Accept requires the proposed admin to authorize — mock_all_auths covers this.
    client.accept_governance_admin();

    let events = env.events().all();
    assert!(events.len() > 0);

    // Ensure admin-topic events exist (proposed / accepted)
    let admin_topic = soroban_sdk::symbol_short!("admin");
    let found_admin_topic = events.iter().any(|event| {
        event.1.len() > 0
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&admin_topic)
    });
    assert!(found_admin_topic);
}
