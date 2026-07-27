use super::{create_contract, default_milestones, generated_participants, register_client};
use crate::ReleaseAuthorization;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn empty_contract_page_is_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let page = client.get_contracts_page(&0u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn contract_page_returns_in_order_for_single_page() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let _ = create_contract(&env, &client);
    let _ = create_contract(&env, &client);

    let page = client.get_contracts_page(&0u32, &10u32);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), 1);
    assert_eq!(page.get(1).unwrap(), 2);

    let page = client.get_contracts_page(&1u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), 2);

    let page = client.get_contracts_page(&2u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn continuation_page_uses_start_offset_and_clamps_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let milestones = default_milestones(&env);
    for _ in 0..3 {
        let (client_addr, freelancer_addr, _) = generated_participants(&env);
        client.create_contract(
            &client_addr,
            &freelancer_addr,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );
    }

    let page = client.get_contracts_page(&0u32, &2u32);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), 1);
    assert_eq!(page.get(1).unwrap(), 2);

    let page = client.get_contracts_page(&2u32, &2u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), 3);

    let page = client.get_contracts_page(&0u32, &1000u32);
    assert_eq!(page.len(), 3);
}

#[test]
fn zero_limit_returns_empty_page() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let page = client.get_contracts_page(&0u32, &0u32);
    assert_eq!(page.len(), 0);
}
