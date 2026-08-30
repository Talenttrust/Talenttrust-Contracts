#![cfg(test)]

use super::register_client;
use soroban_sdk::testutils::Events;
use soroban_sdk::{Env, Symbol, TryFromVal};

#[test]
fn protocol_fee_bps_change_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    // register_client already calls initialize with a generated admin.
    let client = register_client(&env);

    // Change protocol fee bps
    assert!(client.set_protocol_fee_bps(&100u32, &1u64));

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

// Admin propose/accept/cancel event coverage lives in `test/governance.rs`
// alongside the rest of the two-step admin transfer suite.
