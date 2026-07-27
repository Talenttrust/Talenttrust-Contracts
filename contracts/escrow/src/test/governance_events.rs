#![cfg(test)]

use super::register_client;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, Symbol, TryFromVal};

#[test]
fn protocol_fee_bps_change_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);

    let admin = Address::generate(&env);
    // initialize sets the admin for the contract
    client.initialize(&admin);

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
    env.mock_all_auths();

    let client = register_client(&env);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let next_admin = Address::generate(&env);
    client.propose_governance_admin(&next_admin);

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
