#![cfg(test)]

use crate::{ContractStatus, DisputeResolution, Escrow, EscrowClient, ReleaseAuthorization};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    token::StellarAssetClient,
    vec, Address, Env, Symbol, TryFromVal,
};

// ---------------------------------------------------------------------------
// Test helpers (duplicated from dispute.rs to keep the module self-contained)
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env
}

fn make_client(env: &Env) -> (EscrowClient<'_>, Address) {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Create a funded contract with an arbiter, ready for dispute.
/// Binds a settlement token (as admin), mints tokens to the client, and deposits.
/// Returns (client_addr, freelancer_addr, arbiter_addr, contract_id).
fn funded_contract_with_arbiter(
    env: &Env,
    client: &EscrowClient<'_>,
    admin: &Address,
) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);

    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(admin, &token);

    let milestones = vec![env, 100_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    StellarAssetClient::new(env, &token).mint(&client_addr, &100_i128);
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

// ---------------------------------------------------------------------------
// Tests: opened event
// ---------------------------------------------------------------------------

#[test]
fn raise_dispute_emits_opened_event_with_correct_topics() {
    let env = make_env();
    let (client, admin) = make_client(&env);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client, &admin);

    client.raise_dispute(&contract_id, &client_addr);

    let events = env.events().all();
    let (_, topics, _) = events
        .iter()
        .rev()
        .find(|(contract, _, _)| *contract == client.address)
        .expect("must emit a dispute event");

    assert_eq!(topics.len(), 2, "dispute events have two topics");
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get_unchecked(0)).unwrap(),
        symbol_short!("dispute"),
    );
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get_unchecked(1)).unwrap(),
        symbol_short!("opened"),
    );
}

#[test]
fn raise_dispute_emits_opened_event_with_correct_payload() {
    let env = make_env();
    let (client, admin) = make_client(&env);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client, &admin);

    client.raise_dispute(&contract_id, &client_addr);

    let events = env.events().all();
    let (_, _, payload) = events
        .iter()
        .rev()
        .find(|(contract, _, _)| *contract == client.address)
        .expect("must emit a dispute event");

    let decoded: (u32, Address, i128, i128, i128) =
        TryFromVal::try_from_val(&env, &payload).unwrap();
    assert_eq!(decoded.0, contract_id);
    assert_eq!(decoded.1, client_addr);
    // Contract was fully deposited (100) with no releases or refunds
    assert_eq!(decoded.2, 100); // funded_amount
    assert_eq!(decoded.3, 0); // released_amount
    assert_eq!(decoded.4, 0); // refunded_amount
}

// ---------------------------------------------------------------------------
// Tests: resolved event
// ---------------------------------------------------------------------------

#[test]
fn resolve_dispute_emits_resolved_event_with_correct_topics() {
    let env = make_env();
    let (client, admin) = make_client(&env);
    let (client_addr, _, arbiter_addr, contract_id) =
        funded_contract_with_arbiter(&env, &client, &admin);

    client.raise_dispute(&contract_id, &client_addr);
    client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund);

    let events = env.events().all();
    let (_, topics, _) = events
        .iter()
        .rev()
        .find(|(contract, _, _)| *contract == client.address)
        .expect("must emit a dispute resolved event");

    assert_eq!(topics.len(), 2, "dispute events have two topics");
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get_unchecked(0)).unwrap(),
        symbol_short!("dispute"),
    );
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get_unchecked(1)).unwrap(),
        symbol_short!("resolved"),
    );
}

#[test]
fn resolve_full_refund_emits_resolved_event_with_correct_payload() {
    let env = make_env();
    let (client, admin) = make_client(&env);
    let (client_addr, _, arbiter_addr, contract_id) =
        funded_contract_with_arbiter(&env, &client, &admin);

    client.raise_dispute(&contract_id, &client_addr);
    client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund);

    let events = env.events().all();
    let (_, _, payload) = events
        .iter()
        .rev()
        .find(|(contract, _, _)| *contract == client.address)
        .expect("must emit a dispute resolved event");

    // (contract_id, client_payout, freelancer_payout, resolution_code, final_status)
    let decoded: (u32, i128, i128, u32, u32) = TryFromVal::try_from_val(&env, &payload).unwrap();
    assert_eq!(decoded.0, contract_id);
    assert_eq!(decoded.1, 100); // client_payout
    assert_eq!(decoded.2, 0); // freelancer_payout
    assert_eq!(decoded.3, 0); // DisputeResolution::FullRefund.code()
    assert_eq!(decoded.4, ContractStatus::Refunded as u32);
}

#[test]
fn resolve_full_payout_emits_resolved_event_with_correct_payload() {
    let env = make_env();
    let (client, admin) = make_client(&env);
    let (client_addr, _, arbiter_addr, contract_id) =
        funded_contract_with_arbiter(&env, &client, &admin);

    client.raise_dispute(&contract_id, &client_addr);
    client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullPayout);

    let events = env.events().all();
    let (_, _, payload) = events
        .iter()
        .rev()
        .find(|(contract, _, _)| *contract == client.address)
        .expect("must emit a dispute resolved event");

    let decoded: (u32, i128, i128, u32, u32) = TryFromVal::try_from_val(&env, &payload).unwrap();
    assert_eq!(decoded.0, contract_id);
    assert_eq!(decoded.1, 0); // client_payout
    assert_eq!(decoded.2, 100); // freelancer_payout
    assert_eq!(decoded.3, 2); // DisputeResolution::FullPayout.code()
    assert_eq!(decoded.4, ContractStatus::Completed as u32);
}

// ---------------------------------------------------------------------------
// Tests: no topic collision
// ---------------------------------------------------------------------------

#[test]
fn dispute_event_topics_do_not_collide_with_existing_topics() {
    let dispute_topics = [
        symbol_short!("dispute"),
        symbol_short!("opened"),
        symbol_short!("resolved"),
    ];
    for (index, topic) in dispute_topics.iter().enumerate() {
        assert!(
            dispute_topics[index + 1..]
                .iter()
                .all(|other| topic != other),
            "dispute event topics must be unique"
        );
    }

    let other_primary_topics = [
        symbol_short!("init"),
        symbol_short!("admin"),
        symbol_short!("created"),
        symbol_short!("contract"),
        symbol_short!("deposit"),
        symbol_short!("ctrct_st"),
        symbol_short!("ctrct_cmp"),
        symbol_short!("pause"),
        symbol_short!("unpaused"),
        symbol_short!("cancelled"),
        symbol_short!("fee"),
        symbol_short!("withdraw"),
        symbol_short!("finalized"),
        symbol_short!("mlstn_idx"),
        symbol_short!("mlstn_rls"),
        symbol_short!("refunded"),
        symbol_short!("evidence"),
        symbol_short!("repr_put"),
        symbol_short!("sttl_bind"),
        symbol_short!("proto_fee"),
        symbol_short!("limits"),
        symbol_short!("rollback"),
        symbol_short!("auth_chg"),
    ];
    for other_topic in other_primary_topics {
        assert!(
            dispute_topics.iter().all(|d| d != &other_topic),
            "dispute topic {other_topic:?} must not duplicate a primary topic from another event family"
        );
    }
}
