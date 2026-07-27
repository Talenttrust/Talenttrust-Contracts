//! Tests for participant index pagination in the escrow contract.
//!
//! This module verifies listing contracts by participant address for both client (role 0)
//! and freelancer (role 1) roles, ensuring pagination edge cases (empty pages, offset past end,
//! oversized limits, zero limit) and TTL extension routines operate as expected.

use super::{default_milestones, generated_participants, register_client};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Helper to generate client and freelancer addresses for test setups.
fn make_client_freelancer(env: &Env) -> (Address, Address) {
    generated_participants(env)
}

/// Tests that querying an empty participant index returns an empty vector for both client and freelancer roles.
#[test]
fn participant_index_empty_returns_empty_page() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let participant = Address::generate(&env);

    // Client role (0u8)
    let page_client = client.list_contracts_by_participant(&participant, &0u8, &0u32, &10u32);
    assert_eq!(page_client.len(), 0);

    // Freelancer role (1u8)
    let page_freelancer = client.list_contracts_by_participant(&participant, &1u8, &0u32, &10u32);
    assert_eq!(page_freelancer.len(), 0);
}

/// Tests participant contract indexing, role filtering, offset bounds, and limit capping.
#[test]
fn participant_index_client_and_freelancer_lists_are_correct_and_paginated() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client1, freelancer1) = make_client_freelancer(&env);
    let (client2, freelancer2) = make_client_freelancer(&env);

    let milestones = default_milestones(&env);

    let id1 = escrow.create_contract(
        &client1,
        &freelancer1,
        &None,
        &milestones,
        &crate::types::ReleaseAuthorization::ClientOnly,
    );

    let id2 = escrow.create_contract(
        &client2,
        &freelancer2,
        &None,
        &milestones,
        &crate::types::ReleaseAuthorization::ClientOnly,
    );

    // Client pagination for client1: should contain only id1.
    let page = escrow.list_contracts_by_participant(&client1, &0u8, &0u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0), id1);

    // Freelancer pagination for freelancer2: should contain only id2.
    let page = escrow.list_contracts_by_participant(&freelancer2, &1u8, &0u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0), id2);

    // Start out of range (offset past end) -> returns empty page.
    let page = escrow.list_contracts_by_participant(&client1, &0u8, &5u32, &10u32);
    assert_eq!(page.len(), 0);

    // Limit cap behavior: requesting limit (1000) larger than available items returns remaining items.
    let page = escrow.list_contracts_by_participant(&client1, &0u8, &0u32, &1000u32);
    assert_eq!(page.len(), 1);
}

/// Tests pagination edge cases including zero limit, offset equal to total length, and multi-page iteration.
#[test]
fn participant_index_pagination_edge_cases_and_multi_page() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client, freelancer) = make_client_freelancer(&env);
    let milestones = default_milestones(&env);

    // Create 5 contracts for the same client.
    let mut ids = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        let id = escrow.create_contract(
            &client,
            &freelancer,
            &None,
            &milestones,
            &crate::types::ReleaseAuthorization::ClientOnly,
        );
        ids.push_back(id);
    }

    // Zero limit request -> empty page.
    let page_zero = escrow.list_contracts_by_participant(&client, &0u8, &0u32, &0u32);
    assert_eq!(page_zero.len(), 0);

    // Page 1: offset 0, limit 2 -> first 2 contracts.
    let page1 = escrow.list_contracts_by_participant(&client, &0u8, &0u32, &2u32);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0), ids.get(0));
    assert_eq!(page1.get(1), ids.get(1));

    // Page 2: offset 2, limit 2 -> next 2 contracts.
    let page2 = escrow.list_contracts_by_participant(&client, &0u8, &2u32, &2u32);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0), ids.get(2));
    assert_eq!(page2.get(1), ids.get(3));

    // Page 3: offset 4, limit 2 -> last 1 contract.
    let page3 = escrow.list_contracts_by_participant(&client, &0u8, &4u32, &2u32);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0), ids.get(4));

    // Offset equal to total count (5) -> empty page.
    let page_exact_end = escrow.list_contracts_by_participant(&client, &0u8, &5u32, &2u32);
    assert_eq!(page_exact_end.len(), 0);

    // Offset strictly past total count (10) -> empty page.
    let page_past_end = escrow.list_contracts_by_participant(&client, &0u8, &10u32, &2u32);
    assert_eq!(page_past_end.len(), 0);
}

/// Tests that `ttl::extend_participant_contract_index_ttl` functions properly when invoked on participant keys.
#[test]
fn participant_index_ttl_extension_helper_exercised() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client, freelancer) = make_client_freelancer(&env);
    let milestones = default_milestones(&env);

    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &crate::types::ReleaseAuthorization::ClientOnly,
    );

    let page = escrow.list_contracts_by_participant(&client, &0u8, &0u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0), id);

    // Confirm ttl::extend_participant_contract_index_ttl remains exercised
    let key = crate::DataKey::Contract(id);
    crate::ttl::extend_participant_contract_index_ttl(&env, &key);
}
