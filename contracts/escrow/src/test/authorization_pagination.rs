//! Tests for paginated authorization records enumeration.

use super::{default_milestones, register_client};
use crate::types::ReleaseAuthorization;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env};

fn make_participants(env: &Env) -> (Address, Address, Address) {
    (
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
    )
}

#[test]
fn authorization_records_empty_and_unknown_contract_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    // Unknown contract ID should return an empty vector without panicking.
    let records = escrow.get_authorization_records(&9999u32, &0u32, &10u32);
    assert_eq!(records.len(), 0);

    // Also test aliases
    let records_page = escrow.get_authorization_records_page(&9999u32, &0u32, &10u32);
    assert_eq!(records_page.len(), 0);

    let list_records = escrow.list_authorization_records(&9999u32, &0u32, &10u32);
    assert_eq!(list_records.len(), 0);
}

#[test]
fn authorization_records_limit_zero_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client, freelancer, _) = make_participants(&env);
    let milestones = default_milestones(&env);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let records = escrow.get_authorization_records(&id, &0u32, &0u32);
    assert_eq!(records.len(), 0);
}

#[test]
fn authorization_records_start_out_of_range_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client, freelancer, _) = make_participants(&env);
    let milestones = default_milestones(&env);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Milestone count is 3, start at index 5 should return empty
    let records = escrow.get_authorization_records(&id, &5u32, &10u32);
    assert_eq!(records.len(), 0);
}

#[test]
fn authorization_records_single_page_and_continuation() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);

    let milestones = vec![&env, 100_i128, 200_i128, 300_i128];
    let escrow_address = env.register(crate::Escrow, ());
    let escrow = crate::EscrowClient::new(&env, &escrow_address);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::MultiSig,
    );

    StellarAssetClient::new(&env, &token).mint(&client_addr, &600_i128);
    escrow.deposit_funds(&id, &client_addr, &600_i128);

    // Record an approval on milestone index 1
    escrow.approve_milestone_release(&id, &client_addr, &1u32);

    // Query Page 1: start=0, limit=2
    let page1 = escrow.get_authorization_records(&id, &0u32, &2u32);
    assert_eq!(page1.len(), 2);

    let rec0 = page1.get(0).unwrap();
    assert_eq!(rec0.milestone_index, 0);
    assert_eq!(rec0.has_approvals, false);
    assert_eq!(rec0.client_approved, false);
    assert_eq!(rec0.freelancer_approved, false);
    assert_eq!(rec0.arbiter_approved, false);

    let rec1 = page1.get(1).unwrap();
    assert_eq!(rec1.milestone_index, 1);
    assert_eq!(rec1.has_approvals, true);
    assert_eq!(rec1.client_approved, true);
    assert_eq!(rec1.freelancer_approved, false);
    assert_eq!(rec1.arbiter_approved, false);

    // Query Page 2 (continuation): start=2, limit=2
    let page2 = escrow.get_authorization_records(&id, &2u32, &2u32);
    assert_eq!(page2.len(), 1);

    let rec2 = page2.get(0).unwrap();
    assert_eq!(rec2.milestone_index, 2);
    assert_eq!(rec2.has_approvals, false);
    assert_eq!(rec2.client_approved, false);
}

#[test]
fn authorization_records_ceiling_clamp() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client, freelancer, _) = make_participants(&env);
    let milestones = default_milestones(&env);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Request limit 1000, should be clamped by pagination ceiling (MAX_PAGINATION_LIMIT = 50)
    // returning all 3 available milestones without error
    let records = escrow.get_authorization_records(&id, &0u32, &1000u32);
    assert_eq!(records.len(), 3);
}
