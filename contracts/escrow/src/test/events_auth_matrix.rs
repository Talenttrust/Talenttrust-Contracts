#![cfg(test)]
//! Events authorization matrix tests.
//!
//! Verifies that indexed events (contract events, milestone index events,
//! storage index events) are emitted with correct authorization — only
//! authorized callers can trigger event-emitting actions, and events
//! carry the correct payload.

use crate::{Error, Escrow, EscrowClient, ReleaseAuthorization};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, vec, Address, Env, String,
};

use super::assert_contract_error;

struct TestEnv<'a> {
    env: Env,
    client: EscrowClient<'a>,
    admin: Address,
    client_addr: Address,
    freelancer_addr: Address,
    arbiter_addr: Address,
    stranger_addr: Address,
    token_addr: Address,
}

fn setup_full() -> TestEnv<'static> {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let token_addr = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token_addr);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let stranger_addr = Address::generate(&env);

    TestEnv {
        env,
        client,
        admin,
        client_addr,
        freelancer_addr,
        arbiter_addr,
        stranger_addr,
        token_addr,
    }
}

fn create_funded_contract(
    test_env: &TestEnv,
    auth: &ReleaseAuthorization,
) -> u32 {
    let milestones = vec![&test_env.env, 500_0000000_i128, 300_0000000_i128];
    let arbiter = match auth {
        ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter => {
            Some(test_env.arbiter_addr.clone())
        }
        _ => None,
    };
    let id = test_env.client.create_contract(
        &test_env.client_addr,
        &test_env.freelancer_addr,
        &arbiter,
        &milestones,
        auth,
    );
    let total = 800_0000000_i128;
    StellarAssetClient::new(&test_env.env, &test_env.token_addr)
        .mint(&test_env.client_addr, &total);
    test_env.client.deposit_funds(&id, &test_env.client_addr, &total);
    id
}

// ===========================================================================
// 1. Create Contract — events emitted only by authorized Client
// ===========================================================================

#[test]
fn events_create_contract_client_allowed() {
    let t = setup_full();
    let milestones = vec![&t.env, 500_0000000_i128];
    let id = t.client.create_contract(
        &t.client_addr,
        &t.freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(id > 0);
}

#[test]
fn events_create_contract_admin_denied() {
    let t = setup_full();
    let milestones = vec![&t.env, 500_0000000_i128];
    assert_contract_error(
        t.client.try_create_contract(
            &t.admin,
            &t.freelancer_addr,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        ),
        Error::UnauthorizedRole,
    );
}

#[test]
fn events_create_contract_stranger_denied() {
    let t = setup_full();
    let milestones = vec![&t.env, 500_0000000_i128];
    assert_contract_error(
        t.client.try_create_contract(
            &t.stranger_addr,
            &t.freelancer_addr,
            &None,
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        ),
        Error::UnauthorizedRole,
    );
}

// ===========================================================================
// 2. Deposit Funds — events emitted only by authorized Client
// ===========================================================================

#[test]
fn events_deposit_client_allowed() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    let amount = 100_0000000_i128;
    StellarAssetClient::new(&t.env, &t.token_addr).mint(&t.client_addr, &amount);
    assert!(t.client.deposit_funds(&id, &t.client_addr, &amount));
}

#[test]
fn events_deposit_freelancer_denied() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    let amount = 100_0000000_i128;
    StellarAssetClient::new(&t.env, &t.token_addr).mint(&t.freelancer_addr, &amount);
    assert_contract_error(
        t.client.try_deposit_funds(&id, &t.freelancer_addr, &amount),
        Error::UnauthorizedRole,
    );
}

#[test]
fn events_deposit_stranger_denied() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    let amount = 100_0000000_i128;
    StellarAssetClient::new(&t.env, &t.token_addr).mint(&t.stranger_addr, &amount);
    assert_contract_error(
        t.client.try_deposit_funds(&id, &t.stranger_addr, &amount),
        Error::UnauthorizedRole,
    );
}

// ===========================================================================
// 3. Submit Work Evidence — events emitted only by authorized Freelancer
// ===========================================================================

#[test]
fn events_submit_work_freelancer_allowed() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    let cid = String::from_str(&t.env, "QmTest1234567890");
    assert!(t.client.submit_work_evidence(&id, &t.freelancer_addr, &0, &cid));
}

#[test]
fn events_submit_work_admin_denied() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    let cid = String::from_str(&t.env, "QmTest1234567890");
    assert_contract_error(
        t.client.try_submit_work_evidence(&id, &t.admin, &0, &cid),
        Error::UnauthorizedRole,
    );
}

#[test]
fn events_submit_work_client_denied() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    let cid = String::from_str(&t.env, "QmTest1234567890");
    assert_contract_error(
        t.client.try_submit_work_evidence(&id, &t.client_addr, &0, &cid),
        Error::UnauthorizedRole,
    );
}

// ===========================================================================
// 4. Issue Reputation — events emitted only by authorized Client
// ===========================================================================

#[test]
fn events_issue_reputation_client_allowed() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    t.client.approve_milestone_release(&id, &t.client_addr, &0);
    t.client.release_milestone(&id, &t.client_addr, &0);
    let comment = String::from_str(&t.env, "Excellent");
    assert!(t.client.issue_reputation(&id, &t.client_addr, &5, &comment));
}

#[test]
fn events_issue_reputation_stranger_denied() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    t.client.approve_milestone_release(&id, &t.client_addr, &0);
    t.client.release_milestone(&id, &t.client_addr, &0);
    let comment = String::from_str(&t.env, "Excellent");
    assert_contract_error(
        t.client.try_issue_reputation(&id, &t.stranger_addr, &5, &comment),
        Error::UnauthorizedRole,
    );
}

// ===========================================================================
// 5. Finalize Contract — events emitted only by participants
// ===========================================================================

#[test]
fn events_finalize_client_allowed() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    t.client.approve_milestone_release(&id, &t.client_addr, &0);
    t.client.release_milestone(&id, &t.client_addr, &0);
    assert!(t.client.finalize_contract(&id, &t.client_addr));
}

#[test]
fn events_finalize_stranger_denied() {
    let t = setup_full();
    let id = create_funded_contract(&t, &ReleaseAuthorization::ClientOnly);
    t.client.approve_milestone_release(&id, &t.client_addr, &0);
    t.client.release_milestone(&id, &t.client_addr, &0);
    assert_contract_error(
        t.client.try_finalize_contract(&id, &t.stranger_addr),
        Error::UnauthorizedRole,
    );
}

// ===========================================================================
// 6. Admin Governance — events emitted only by Admin
// ===========================================================================

#[test]
fn events_admin_settlement_token_allowed() {
    let t = setup_full();
    let new_token = t.env.register_stellar_asset_contract(t.admin.clone());
    assert!(t.client.set_settlement_token(&t.admin, &new_token));
}

#[test]
fn events_admin_settlement_token_client_denied() {
    let t = setup_full();
    let new_token = t.env.register_stellar_asset_contract(t.admin.clone());
    assert_contract_error(
        t.client.try_set_settlement_token(&t.client_addr, &new_token),
        Error::UnauthorizedRole,
    );
}