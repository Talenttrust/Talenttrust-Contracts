#![cfg(test)]

use super::total_milestone_amount;
use crate::{Escrow, ReleaseAuthorization};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Events;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{symbol_short, Address, Env, String, Symbol, TryFromVal};

fn valid_comment(env: &Env) -> String {
    String::from_str(env, "Great work!")
}

fn mint_to(env: &Env, sac: &Address, holder: &Address, amount: i128) {
    StellarAssetClient::new(env, sac).mint(holder, &amount);
}

fn setup_bound(env: &Env) -> (super::EscrowClient<'_>, Address, Address) {
    env.ledger().set_timestamp(1000);
    let id = env.register(Escrow, ());
    let escrow = super::EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract(admin.clone());
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    escrow.bind_settlement_token(&admin, &sac);
    (escrow, sac, admin)
}

fn setup_funded_contract(env: &Env) -> (Address, Address, u32) {
    let (escrow, sac, _) = setup_bound(env);
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = super::default_milestones(env);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = total_milestone_amount();
    mint_to(env, &sac, &client_addr, total);
    escrow.deposit_funds(&contract_id, &client_addr, &total);
    (client_addr, freelancer_addr, contract_id)
}

fn setup_completed_contract(env: &Env) -> (super::EscrowClient<'_>, Address, Address, u32) {
    let (escrow, sac, _) = setup_bound(env);
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = super::default_milestones(env);
    let contract_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = total_milestone_amount();
    mint_to(env, &sac, &client_addr, total);
    escrow.deposit_funds(&contract_id, &client_addr, &total);
    for idx in 0..3u32 {
        escrow.approve_milestone_release(&contract_id, &client_addr, &idx);
        escrow.release_milestone(&contract_id, &client_addr, &idx);
    }
    (escrow, client_addr, freelancer_addr, contract_id)
}

fn has_event_with_topic(env: &Env, topic: &Symbol) -> bool {
    env.events().all().iter().any(|event| {
        !event.1.is_empty()
            && Symbol::try_from_val(env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(topic)
    })
}

fn find_event_with_topic(
    env: &Env,
    topic: &Symbol,
) -> Option<(
    Address,
    soroban_sdk::Vec<soroban_sdk::Val>,
    soroban_sdk::Val,
)> {
    env.events().all().into_iter().find(|event| {
        !event.1.is_empty()
            && Symbol::try_from_val(env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(topic)
    })
}

// ── Deposit event ─────────────────────────────────────────────────────────

#[test]
fn deposit_emits_deposit_event_with_correct_topic() {
    let env = Env::default();
    let (escrow, sac, _) = setup_bound(&env);
    let client_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let contract_id = escrow.create_contract(
        &client_addr,
        &Address::generate(&env),
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = total_milestone_amount();
    mint_to(&env, &sac, &client_addr, total);
    escrow.deposit_funds(&contract_id, &client_addr, &total);

    let topic = symbol_short!("deposit");
    assert!(
        has_event_with_topic(&env, &topic),
        "deposit event must be emitted"
    );
}

#[test]
fn deposit_event_contains_contract_id_in_topics() {
    let env = Env::default();
    let (escrow, sac, _) = setup_bound(&env);
    let client_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let contract_id = escrow.create_contract(
        &client_addr,
        &Address::generate(&env),
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = total_milestone_amount();
    mint_to(&env, &sac, &client_addr, total);
    escrow.deposit_funds(&contract_id, &client_addr, &total);

    let topic = symbol_short!("deposit");
    let (_, topics, _) = find_event_with_topic(&env, &topic).expect("deposit event missing");

    assert_eq!(topics.len(), 2, "topics must have 2 elements");
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        topic
    );

    let topic_contract_id: u32 = TryFromVal::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(
        topic_contract_id, contract_id,
        "second topic must be contract_id"
    );
}

#[test]
fn deposit_event_payload_contains_amount_caller_timestamp() {
    let env = Env::default();
    let (escrow, sac, _) = setup_bound(&env);
    let client_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let contract_id = escrow.create_contract(
        &client_addr,
        &Address::generate(&env),
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = total_milestone_amount();
    mint_to(&env, &sac, &client_addr, total);
    escrow.deposit_funds(&contract_id, &client_addr, &total);

    let topic = symbol_short!("deposit");
    let (_, _, data) = find_event_with_topic(&env, &topic).expect("deposit event missing");

    let data_vec: soroban_sdk::Vec<soroban_sdk::Val> =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(data_vec.len(), 3, "data must have 3 elements");

    let amount: i128 = TryFromVal::try_from_val(&env, &data_vec.get(0).unwrap()).unwrap();
    assert_eq!(amount, total, "data[0] must be deposit amount");

    let data_caller: Address = TryFromVal::try_from_val(&env, &data_vec.get(1).unwrap()).unwrap();
    assert_eq!(data_caller, client_addr, "data[1] must be caller address");

    let ts: u64 = TryFromVal::try_from_val(&env, &data_vec.get(2).unwrap()).unwrap();
    assert!(ts > 0, "data[2] must be a non-zero timestamp");
}

#[test]
fn deposit_event_not_emitted_for_zero_deposit() {
    let env = Env::default();
    let (escrow, _, _) = setup_bound(&env);
    let client_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let contract_id = escrow.create_contract(
        &client_addr,
        &Address::generate(&env),
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let result = escrow.try_deposit_funds(&contract_id, &client_addr, &0_i128);
    assert!(result.is_err(), "zero deposit must fail");

    let topic = symbol_short!("deposit");
    assert!(
        !has_event_with_topic(&env, &topic),
        "deposit event must NOT be emitted for failed deposits"
    );
}

// ── Reputation event ──────────────────────────────────────────────────────

#[test]
fn reputation_emits_repr_put_event_with_correct_topic() {
    let env = Env::default();
    let (escrow, client_addr, _freelancer_addr, contract_id) = setup_completed_contract(&env);

    escrow.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));

    let topic = symbol_short!("repr_put");
    assert!(
        has_event_with_topic(&env, &topic),
        "repr_put event must be emitted"
    );
}

#[test]
fn reputation_event_contains_contract_id_in_topics() {
    let env = Env::default();
    let (escrow, client_addr, _freelancer_addr, contract_id) = setup_completed_contract(&env);

    escrow.issue_reputation(&contract_id, &client_addr, &4, &valid_comment(&env));

    let topic = symbol_short!("repr_put");
    let (_, topics, _) = find_event_with_topic(&env, &topic).expect("repr_put event missing");

    assert_eq!(topics.len(), 2, "topics must have 2 elements");
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        topic
    );

    let topic_contract_id: u32 = TryFromVal::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(
        topic_contract_id, contract_id,
        "second topic must be contract_id"
    );
}

#[test]
fn reputation_event_payload_contains_freelancer_rating_timestamp() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, contract_id) = setup_completed_contract(&env);

    let rating: u32 = 3;
    escrow.issue_reputation(&contract_id, &client_addr, &rating, &valid_comment(&env));

    let topic = symbol_short!("repr_put");
    let (_, _, data) = find_event_with_topic(&env, &topic).expect("repr_put event missing");

    let data_vec: soroban_sdk::Vec<soroban_sdk::Val> =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(data_vec.len(), 3, "data must have 3 elements");

    let data_freelancer: Address =
        TryFromVal::try_from_val(&env, &data_vec.get(0).unwrap()).unwrap();
    assert_eq!(
        data_freelancer, freelancer_addr,
        "data[0] must be freelancer address"
    );

    let data_rating: u32 = TryFromVal::try_from_val(&env, &data_vec.get(1).unwrap()).unwrap();
    assert_eq!(data_rating, rating, "data[1] must be rating");

    let ts: u64 = TryFromVal::try_from_val(&env, &data_vec.get(2).unwrap()).unwrap();
    assert!(ts > 0, "data[2] must be a non-zero timestamp");
}

#[test]
fn reputation_event_emitted_exactly_once() {
    let env = Env::default();
    let (escrow, client_addr, _freelancer_addr, contract_id) = setup_completed_contract(&env);

    escrow.issue_reputation(&contract_id, &client_addr, &5, &valid_comment(&env));

    let topic = symbol_short!("repr_put");
    let count = env
        .events()
        .all()
        .iter()
        .filter(|event| {
            !event.1.is_empty()
                && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                    .ok()
                    .as_ref()
                    == Some(&topic)
        })
        .count();
    assert_eq!(count, 1, "repr_put event must be emitted exactly once");
}
