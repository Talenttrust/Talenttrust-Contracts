use soroban_sdk::{testutils::Address as _, Address, Env};

use super::register_client;
use crate::EscrowClient;

/// Create a funded contract with an arbiter, ready for dispute.
/// Returns (client_addr, freelancer_addr, arbiter_addr, contract_id).
fn funded_contract_with_arbiter(
    env: &Env,
    client: &EscrowClient<'_>,
) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    let milestones = soroban_sdk::vec![env, 100_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

#[test]
fn empty_disputes_page_is_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let page = client.get_disputes_page(&0u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn zero_limit_returns_empty_page() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);
    client.raise_dispute(&contract_id, &client_addr);

    let page = client.get_disputes_page(&0u32, &0u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn start_beyond_end_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let page = client.get_disputes_page(&100u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn single_dispute_appears_in_page() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);
    client.raise_dispute(&contract_id, &client_addr);

    let page = client.get_disputes_page(&0u32, &10u32);
    assert_eq!(page.len(), 1);
    let meta = page.get(0).unwrap();
    assert_eq!(meta.raised_by, client_addr);
    assert_eq!(meta.schema_version, crate::DISPUTE_STORAGE_VERSION);
}

#[test]
fn non_disputed_contracts_are_skipped() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, _, id1) = funded_contract_with_arbiter(&env, &client);
    let (_, _, _, _id2) = funded_contract_with_arbiter(&env, &client);

    client.raise_dispute(&id1, &client_addr);

    let page = client.get_disputes_page(&0u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().raised_by, client_addr);
}

#[test]
fn continuation_page_fetches_remaining() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr1, _, _, id1) = funded_contract_with_arbiter(&env, &client);
    let (client_addr2, _, _, id2) = funded_contract_with_arbiter(&env, &client);
    let (client_addr3, _, _, id3) = funded_contract_with_arbiter(&env, &client);

    client.raise_dispute(&id1, &client_addr1);
    client.raise_dispute(&id2, &client_addr2);
    client.raise_dispute(&id3, &client_addr3);

    let page1 = client.get_disputes_page(&0u32, &1u32);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap().raised_by, client_addr1);

    let page2 = client.get_disputes_page(&1u32, &1u32);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().raised_by, client_addr2);

    let page3 = client.get_disputes_page(&2u32, &1u32);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().raised_by, client_addr3);

    let page4 = client.get_disputes_page(&3u32, &1u32);
    assert_eq!(page4.len(), 0);
}

#[test]
fn limit_clamped_to_page_ceiling() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    for _ in 0..3 {
        let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);
        client.raise_dispute(&contract_id, &client_addr);
    }

    let page = client.get_disputes_page(&0u32, &(crate::PAGE_CEILING * 10));
    assert_eq!(page.len(), 3);
}

#[test]
fn resolved_dispute_clears_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, arbiter_addr, contract_id) = funded_contract_with_arbiter(&env, &client);

    client.raise_dispute(&contract_id, &client_addr);
    assert_eq!(client.get_disputes_page(&0u32, &10u32).len(), 1);

    client.resolve_dispute(&contract_id, &arbiter_addr, &crate::DisputeResolution::FullRefund);
    assert_eq!(client.get_disputes_page(&0u32, &10u32).len(), 0);
}

#[test]
fn get_dispute_returns_metadata_for_active_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);

    client.raise_dispute(&contract_id, &client_addr);

    let meta = client.get_dispute(&contract_id);
    assert!(meta.is_some());
    let meta = meta.unwrap();
    assert_eq!(meta.raised_by, client_addr);
    assert_eq!(meta.schema_version, crate::DISPUTE_STORAGE_VERSION);
}

#[test]
fn get_dispute_returns_none_without_active_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);

    let meta = client.get_dispute(&contract_id);
    assert!(meta.is_none());
}

#[test]
fn get_dispute_returns_none_for_unknown_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let meta = client.get_dispute(&999u32);
    assert!(meta.is_none());
}
