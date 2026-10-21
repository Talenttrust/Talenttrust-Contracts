extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{storage::Persistent as _, Address as _},
    token::{StellarAssetClient, TokenClient},
    vec, BytesN, Symbol, Vec,
};

struct Setup {
    env: Env,
    escrow_addr: soroban_sdk::Address,
    token_addr: soroban_sdk::Address,
    client: Address,
    freelancer: Address,
    id: u64,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    let eid = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &eid);
    escrow.initialize(&admin);
    let id = escrow.create_contract(
        &client, &freelancer, &sac.address(), &vec![&env, 100_i128, 200_i128],
    );
    Setup { env, escrow_addr: eid, token_addr: sac.address(), client, freelancer, id }
}

fn escrow(s: &Setup) -> EscrowClient<'_> {
    EscrowClient::new(&s.env, &s.escrow_addr)
}
fn token(s: &Setup) -> TokenClient<'_> {
    TokenClient::new(&s.env, &s.token_addr)
}

fn hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

// ── existing tests ────────────────────────────────────────────────────────────

#[test]
fn initialize_only_once() {
    let s = setup();
    let other = Address::generate(&s.env);
    assert_eq!(
        escrow(&s).try_initialize(&other),
        Err(Ok(EscrowError::AlreadyInitialized))
    );
}

#[test]
fn full_happy_path() {
    let s = setup();
    let e = escrow(&s);
    e.deposit_funds(&s.id, &300);
    assert_eq!(token(&s).balance(&s.client), 700);
    assert_eq!(e.get_contract(&s.id).status, ContractStatus::Funded);
    e.release_milestone(&s.id, &0);
    e.release_milestone(&s.id, &1);
    assert_eq!(token(&s).balance(&s.freelancer), 300);
    assert_eq!(e.get_contract(&s.id).status, ContractStatus::Completed);
    e.issue_reputation(&s.id, &5);
    let rep = e.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
}

#[test]
fn rejects_overfunding_and_double_release() {
    let s = setup();
    let e = escrow(&s);
    assert_eq!(e.try_deposit_funds(&s.id, &301), Err(Ok(EscrowError::OverFunded)));
    e.deposit_funds(&s.id, &300);
    e.release_milestone(&s.id, &0);
    assert!(e.try_release_milestone(&s.id, &0).is_err());
}

#[test]
fn dispute_split_and_cancel() {
    let s = setup();
    let e = escrow(&s);
    // Fund a single-milestone contract for dispute test
    let env = &s.env;
    let admin = Address::generate(env);
    let c2 = Address::generate(env);
    let f2 = Address::generate(env);
    let sac2 = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(env, &sac2.address()).mint(&c2, &1_000);
    let eid2 = env.register(Escrow, ());
    let e2 = EscrowClient::new(env, &eid2);
    e2.initialize(&admin);
    let id = e2.create_contract(&c2, &f2, &sac2.address(), &vec![env, 400_i128]);
    e2.deposit_funds(&id, &400);
    e2.open_dispute(&id, &f2);
    e2.resolve_dispute(&id, &2_500);
    let tok2 = TokenClient::new(env, &sac2.address());
    assert_eq!(tok2.balance(&f2), 100);
    assert_eq!(tok2.balance(&c2), 900);

    let id2 = e2.create_contract(&c2, &f2, &sac2.address(), &vec![env, 50_i128]);
    e2.deposit_funds(&id2, &50);
    e2.cancel_contract(&id2);
    assert_eq!(tok2.balance(&c2), 900);
    assert_eq!(e2.get_contract(&id2).status, ContractStatus::Cancelled);
}

#[test]
fn pause_blocks_mutations() {
    let s = setup();
    let e = escrow(&s);
    e.set_paused(&true);
    let other_c = Address::generate(&s.env);
    let other_f = Address::generate(&s.env);
    assert_eq!(
        e.try_create_contract(&other_c, &other_f, &s.token_addr, &vec![&s.env, 1_i128]),
        Err(Ok(EscrowError::Paused))
    );
}

// ── set_terms tests ───────────────────────────────────────────────────────────

#[test]
fn set_terms_then_fund_succeeds() {
    let s = setup();
    let e = escrow(&s);
    let terms = hash(&s.env, 0x01);
    let specs: Vec<BytesN<32>> = Vec::new(&s.env);
    e.set_terms(&s.id, &terms, &specs);

    let (stored_terms, stored_specs) = e.get_terms(&s.id).unwrap();
    assert_eq!(stored_terms, terms);
    assert_eq!(stored_specs.len(), 0);

    e.deposit_funds(&s.id, &300);
    assert_eq!(e.get_contract(&s.id).status, ContractStatus::Funded);
}

#[test]
fn set_terms_with_milestone_hashes_succeeds() {
    let s = setup();
    let e = escrow(&s);
    let terms = hash(&s.env, 0x01);
    let specs = vec![&s.env, hash(&s.env, 0x10), hash(&s.env, 0x20)];
    e.set_terms(&s.id, &terms, &specs);

    let (t, sp) = e.get_terms(&s.id).unwrap();
    assert_eq!(t, terms);
    assert_eq!(sp.len(), 2);
    assert_eq!(sp.get(0).unwrap(), hash(&s.env, 0x10));
    assert_eq!(sp.get(1).unwrap(), hash(&s.env, 0x20));
}

#[test]
fn set_terms_after_deposit_rejected() {
    let s = setup();
    let e = escrow(&s);
    e.deposit_funds(&s.id, &300);
    let result = e.try_set_terms(&s.id, &hash(&s.env, 0x01), &Vec::new(&s.env));
    assert_eq!(result, Err(Ok(EscrowError::InvalidStatus)));
}

#[test]
fn set_terms_length_mismatch_rejected() {
    let s = setup();
    let e = escrow(&s);
    // 2 milestones, 1 hash → InvalidMetadata
    let result = e.try_set_terms(
        &s.id,
        &hash(&s.env, 0x01),
        &vec![&s.env, hash(&s.env, 0x10)],
    );
    assert_eq!(result, Err(Ok(EscrowError::InvalidMetadata)));
}

#[test]
fn get_terms_returns_none_when_unset() {
    let s = setup();
    assert!(escrow(&s).get_terms(&s.id).is_none());
}

// ── TTL assertions ────────────────────────────────────────────────────────────

#[test]
fn set_terms_bumps_ttl_for_terms_key() {
    let s = setup();
    escrow(&s).set_terms(&s.id, &hash(&s.env, 0x01), &Vec::new(&s.env));
    let ttl: u32 = s.env.as_contract(&s.escrow_addr, || {
        s.env.storage().persistent().get_ttl(&DataKey::Terms(s.id))
    });
    assert!(ttl >= PERSISTENT_TTL_THRESHOLD, "TTL must be >= threshold after set_terms, got {}", ttl);
}

#[test]
fn set_terms_bumps_ttl_for_milestone_specs_key() {
    let s = setup();
    let specs = vec![&s.env, hash(&s.env, 0x10), hash(&s.env, 0x20)];
    escrow(&s).set_terms(&s.id, &hash(&s.env, 0x01), &specs);
    let ttl: u32 = s.env.as_contract(&s.escrow_addr, || {
        s.env.storage().persistent().get_ttl(&DataKey::MilestoneSpecs(s.id))
    });
    assert!(ttl >= PERSISTENT_TTL_THRESHOLD, "MilestoneSpecs TTL must be >= threshold, got {}", ttl);
}

#[test]
fn deliver_bumps_ttl_for_delivery_key() {
    let s = setup();
    escrow(&s).deliver(&s.id, &0, &hash(&s.env, 0xA0));
    let ttl: u32 = s.env.as_contract(&s.escrow_addr, || {
        s.env.storage().persistent().get_ttl(&DataKey::Delivery(s.id, 0))
    });
    assert!(ttl >= PERSISTENT_TTL_THRESHOLD, "Delivery TTL must be >= threshold, got {}", ttl);
}

// ── deliver tests ─────────────────────────────────────────────────────────────

#[test]
fn deliver_recorded_and_overwritable() {
    let s = setup();
    let e = escrow(&s);
    assert!(e.get_delivery(&s.id, &0).is_none());

    let h1 = hash(&s.env, 0xA0);
    e.deliver(&s.id, &0, &h1);
    assert_eq!(e.get_delivery(&s.id, &0).unwrap(), h1);

    let h2 = hash(&s.env, 0xB0);
    e.deliver(&s.id, &0, &h2);
    assert_eq!(e.get_delivery(&s.id, &0).unwrap(), h2);
}

#[test]
fn deliver_after_release_rejected() {
    let s = setup();
    let e = escrow(&s);
    e.deposit_funds(&s.id, &300);
    e.release_milestone(&s.id, &0);
    let result = e.try_deliver(&s.id, &0, &hash(&s.env, 0xA0));
    assert_eq!(result, Err(Ok(EscrowError::MilestoneAlreadyReleased)));
}

#[test]
fn deliver_index_out_of_bounds_rejected() {
    let s = setup();
    let result = escrow(&s).try_deliver(&s.id, &99, &hash(&s.env, 0xA0));
    assert_eq!(result, Err(Ok(EscrowError::MilestoneNotFound)));
}

#[test]
fn get_delivery_returns_none_when_unset() {
    let s = setup();
    assert!(escrow(&s).get_delivery(&s.id, &0).is_none());
}
