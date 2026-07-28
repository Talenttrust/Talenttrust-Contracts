use super::{create_contract, register_client};

use crate::{ContractEntry, PAGE_CEILING};

use soroban_sdk::Env;

#[test]
fn no_contracts_returns_empty_page() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let page = client.get_contracts_page(&0u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn full_page_of_created_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, id1) = create_contract(&env, &client);
    let (_, _, id2) = create_contract(&env, &client);
    let (_, _, id3) = create_contract(&env, &client);

    let page = client.get_contracts_page(&0u32, &10u32);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0).unwrap().id, id1);
    assert_eq!(page.get(1).unwrap().id, id2);
    assert_eq!(page.get(2).unwrap().id, id3);
    for i in 0..3 {
        let entry: ContractEntry = page.get(i).unwrap();
        // Freshly created contracts are unfunded (status 0 == Created).
        assert_eq!(entry.status, 0);
        assert_eq!(entry.funded_amount, 0);
        assert_eq!(entry.released_amount, 0);
    }
}

#[test]
fn start_beyond_end_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    create_contract(&env, &client);

    let page = client.get_contracts_page(&100u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn start_at_last_contract_returns_one() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    create_contract(&env, &client);
    create_contract(&env, &client);
    let (_, _, id3) = create_contract(&env, &client);

    let page = client.get_contracts_page(&2u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().id, id3);
}

#[test]
fn limit_clamped_to_page_ceiling() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    for _ in 0..3 {
        create_contract(&env, &client);
    }

    // Requesting far more than PAGE_CEILING never panics and never returns
    // more than what actually exists (3 here, well under the ceiling).
    let page = client.get_contracts_page(&0u32, &(PAGE_CEILING * 10));
    assert_eq!(page.len(), 3);
}

#[test]
fn zero_limit_returns_empty_page() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    create_contract(&env, &client);

    let page = client.get_contracts_page(&0u32, &0u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn continuation_page_fetches_remaining() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, id1) = create_contract(&env, &client);
    let (_, _, id2) = create_contract(&env, &client);
    let (_, _, id3) = create_contract(&env, &client);

    let page1 = client.get_contracts_page(&0u32, &1u32);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap().id, id1);

    let page2 = client.get_contracts_page(&1u32, &1u32);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().id, id2);

    let page3 = client.get_contracts_page(&2u32, &1u32);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().id, id3);

    let page4 = client.get_contracts_page(&3u32, &1u32);
    assert_eq!(page4.len(), 0);
}

#[test]
fn exact_page_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    for _ in 0..3 {
        create_contract(&env, &client);
    }

    let page = client.get_contracts_page(&0u32, &3u32);
    assert_eq!(page.len(), 3);
    let page_next = client.get_contracts_page(&3u32, &3u32);
    assert_eq!(page_next.len(), 0);
}

#[test]
fn funded_contract_reflects_status_and_amount() {
    let fixture = super::EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let page = escrow.get_contracts_page(&0u32, &10u32);
    assert_eq!(page.len(), 1);
    let entry = page.get(0).unwrap();
    assert_eq!(entry.id, fixture.escrow_id);
    // Fully funded (status 2 == Funded).
    assert_eq!(entry.status, 2);
    assert_eq!(entry.funded_amount, fixture.total_amount());
    assert_eq!(entry.released_amount, 0);
}

#[test]
fn released_milestone_updates_page_entry() {
    let fixture = super::EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let cid = fixture.escrow_id;

    escrow.approve_milestone_release(&cid, &fixture.client, &0u32);
    escrow.release_milestone(&cid, &fixture.client, &0u32);

    let page = escrow.get_contracts_page(&0u32, &10u32);
    assert_eq!(page.len(), 1);
    let entry = page.get(0).unwrap();
    assert!(entry.released_amount > 0);
}

#[test]
fn single_contract_pagination() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, id) = create_contract(&env, &client);

    let page = client.get_contracts_page(&0u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().id, id);
}
