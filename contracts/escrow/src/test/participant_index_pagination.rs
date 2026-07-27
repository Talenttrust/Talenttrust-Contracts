use super::{default_milestones, generated_participants, register_client};

use soroban_sdk::{testutils::Address as _, Address, Env};

fn make_client_freelancer(env: &Env) -> (Address, Address) {
    generated_participants(env)
}

#[test]
fn participant_index_empty_returns_empty_page() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let participant = Address::generate(&env);

    let page_client = client.list_contracts_by_participant(&participant, &0u8, &0u32, &10u32);
    assert_eq!(page_client.len(), 0);

    let page_freelancer =
        client.list_contracts_by_participant(&participant, &1u8, &0u32, &10u32);
    assert_eq!(page_freelancer.len(), 0);
}

#[test]
fn participant_index_client_and_freelancer_lists_are_correct_and_paginated() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client1, freelancer1) = make_client_freelancer(&env);
    let (client2, freelancer2) = make_client_freelancer(&env);

    // Create two contracts.
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

    // start out of range -> empty
    let page = escrow.list_contracts_by_participant(&client1, &0u8, &5u32, &10u32);
    assert_eq!(page.len(), 0);

    // limit cap behavior: request more than available; should return remaining only.
    let page = escrow.list_contracts_by_participant(&client1, &0u8, &0u32, &1000u32);
    assert_eq!(page.len(), 1);
}

