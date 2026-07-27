use super::*;
use soroban_sdk::{symbol_short, testutils::Events, vec, Symbol, Val};

/// Helper to extract all indexed contract events `(symbol_short!("contract"), contract_id)`.
fn get_contract_indexed_events(
    env: &Env,
    target_contract_id: u32,
) -> Vec<(u32, i128, i128, i128, i128)> {
    let mut matching_events = Vec::new(env);
    let expected_topic_0: Val = symbol_short!("contract").into();
    let expected_topic_1: Val = target_contract_id.into();

    for event in env.events().all().iter() {
        let topics = event.1;
        if topics.len() == 2
            && topics.get(0).unwrap() == expected_topic_0
            && topics.get(1).unwrap() == expected_topic_1
        {
            if let Ok(data) = <(u32, i128, i128, i128, i128)>::try_from_val(env, &event.2) {
                matching_events.push_back(data);
            }
        }
    }
    matching_events
}

#[test]
fn test_indexed_event_emitted_on_create_contract() {
    let env = Env::default();
    env.mock_all_signatures();

    let contract_id = EscrowClient::new(&env, &env.register_contract(None, Escrow))
        .initialize(&Address::generate(&env), &Address::generate(&env));

    let client = EscrowClient::new(&env, &contract_id);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let new_contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_0000000],
        &ReleaseAuthorization::ClientOnly,
    );

    let events = get_contract_indexed_events(&env, new_contract_id);
    assert!(!events.is_empty());

    let (status, funded, released, refunded, total_deposited) = events.get(0).unwrap();
    assert_eq!(status, ContractStatus::Created as u32);
    assert_eq!(funded, 0);
    assert_eq!(released, 0);
    assert_eq!(refunded, 0);
    assert_eq!(total_deposited, 0);
}

#[test]
fn test_indexed_event_emitted_on_deposit() {
    let env = Env::default();
    env.mock_all_signatures();

    let escrow_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = StellarAssetClient::new(&env, &token_contract.address);
    client.bind_settlement_token(&token_contract.address, &admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    token_client.mint(&client_addr, &1000_0000000);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_0000000],
        &ReleaseAuthorization::ClientOnly,
    );

    client.deposit_funds(&id, &client_addr, &100_0000000);

    let events = get_contract_indexed_events(&env, id);
    // Should have creation event and deposit event
    assert!(events.len() >= 2);

    let latest_event = events.get(events.len() - 1).unwrap();
    let (status, funded, released, refunded, total_deposited) = latest_event;
    assert_eq!(status, ContractStatus::Funded as u32);
    assert_eq!(funded, 100_0000000);
    assert_eq!(released, 0);
    assert_eq!(refunded, 0);
    assert_eq!(total_deposited, 100_0000000);
}

#[test]
fn test_indexed_event_emitted_on_milestone_release() {
    let env = Env::default();
    env.mock_all_signatures();

    let escrow_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &escrow_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = StellarAssetClient::new(&env, &token_contract.address);
    client.bind_settlement_token(&token_contract.address, &admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    token_client.mint(&client_addr, &1000_0000000);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_0000000],
        &ReleaseAuthorization::ClientOnly,
    );

    client.deposit_funds(&id, &client_addr, &100_0000000);
    client.release_milestone(&id, &client_addr, &0);

    let events = get_contract_indexed_events(&env, id);
    let latest_event = events.get(events.len() - 1).unwrap();
    let (status, funded, released, refunded, total_deposited) = latest_event;
    assert_eq!(status, ContractStatus::Completed as u32);
    assert_eq!(funded, 100_0000000);
    assert_eq!(released, 100_0000000);
    assert_eq!(refunded, 0);
    assert_eq!(total_deposited, 100_0000000);
}

#[test]
fn test_no_topic_collision_with_existing_events() {
    let indexed_topic = symbol_short!("contract");

    // Existing event topics in the contract
    let existing_topics = [
        symbol_short!("init"),
        symbol_short!("created"),
        symbol_short!("mlstn_rls"),
        symbol_short!("ctrct_cmp"),
        symbol_short!("refunded"),
        symbol_short!("pause"),
        symbol_short!("unpaused"),
        symbol_short!("cancelled"),
        symbol_short!("evidence"),
        symbol_short!("fee"),
        symbol_short!("dispute"),
        symbol_short!("admin"),
        symbol_short!("finalized"),
    ];

    for existing in existing_topics.iter() {
        assert_ne!(
            indexed_topic, *existing,
            "Topic collision detected between 'contract' and existing topic"
        );
    }
}
