use super::{register_client_with_token, complete_contract_funded};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn valid_comment(env: &Env) -> String {
    String::from_str(env, "Great job!")
}

// Tests for the paginated reputations view: empty, single page, continuation, ceiling clamp.

#[test]
fn reputations_empty_returns_empty_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _token) = register_client_with_token(&env);

    let page = client.get_reputations_page(&0u32, &10u32);
    assert_eq!(page.len(), 0);
}

#[test]
fn reputations_single_page_and_contents() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token) = register_client_with_token(&env);

    // Create and issue reputations for three different freelancers.
    let mut freelancers = Vec::new();
    for _ in 0..3 {
        let (client_addr, freelancer_addr, contract_id) =
            complete_contract_funded(&env, &client, &token);
        assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));
        freelancers.push(freelancer_addr);
    }

    let page = client.get_reputations_page(&0u32, &10u32);
    assert_eq!(page.len(), 3);
    // Ensure returned accounts match stored entries in index order.
    for i in 0..3u32 {
        let entry = page.get(i).unwrap();
        assert_eq!(entry.account, freelancers.get(i as usize));
        assert_eq!(entry.completed_contracts, 1);
        assert_eq!(entry.total_rating, 5);
        assert_eq!(entry.last_rating, 5);
    }
}

#[test]
fn reputations_pagination_continuation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token) = register_client_with_token(&env);

    // Create 5 reputations
    let mut freelancers = Vec::new();
    for _ in 0..5 {
        let (client_addr, freelancer_addr, contract_id) =
            complete_contract_funded(&env, &client, &token);
        assert!(client.issue_reputation(&contract_id, &client_addr, &4, &valid_comment(&env)));
        freelancers.push(freelancer_addr);
    }

    // Page 1: start 0, limit 2
    let page1 = client.get_reputations_page(&0u32, &2u32);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().account, freelancers.get(0));
    assert_eq!(page1.get(1).unwrap().account, freelancers.get(1));

    // Page 2: start 2, limit 2
    let page2 = client.get_reputations_page(&2u32, &2u32);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap().account, freelancers.get(2));
    assert_eq!(page2.get(1).unwrap().account, freelancers.get(3));

    // Page 3: start 4, limit 2 -> last item only
    let page3 = client.get_reputations_page(&4u32, &2u32);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().account, freelancers.get(4));
}

#[test]
fn reputations_ceiling_clamp_behaviour() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, token) = register_client_with_token(&env);

    // Create 3 reputations
    for _ in 0..3 {
        let (client_addr, _freelancer_addr, contract_id) =
            complete_contract_funded(&env, &client, &token);
        assert!(client.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env)));
    }

    // Request a huge limit; result should just include available entries without error.
    let page = client.get_reputations_page(&0u32, &1000u32);
    assert_eq!(page.len(), 3);
}
