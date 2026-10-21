extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    token::TokenClient,
    TryFromVal, TryIntoVal,
    vec,
};

struct Setup<'a> {
    env: Env,
    escrow: EscrowClient<'a>,
    token: TokenClient<'a>,
    client: Address,
    freelancer: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let token = TokenClient::new(&env, &sac.address());
    Setup {
        env,
        escrow,
        token,
        client,
        freelancer,
    }
}

#[test]
fn initialize_only_once() {
    let s = setup();
    let other = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_initialize(&other),
        Err(Ok(EscrowError::AlreadyInitialized))
    );
}

#[test]
fn full_happy_path() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128],
    );
    assert_eq!(id, 1);
    s.escrow.deposit_funds(&id, &300);
    assert_eq!(s.token.balance(&s.client), 700);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.token.balance(&s.freelancer), 300);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    s.escrow.issue_reputation(&id, &5);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
}

#[test]
fn rejects_overfunding_and_double_release() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &101),
        Err(Ok(EscrowError::OverFunded))
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    assert!(s.escrow.try_release_milestone(&id, &0).is_err());
}

#[test]
fn dispute_split_and_cancel() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&id, &2_500);
    assert_eq!(s.token.balance(&s.freelancer), 100);
    assert_eq!(s.token.balance(&s.client), 900);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.token.balance(&s.client), 900);
    assert_eq!(
        s.escrow.get_contract(&id2).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn pause_blocks_mutations() {
    let s = setup();
    s.escrow.set_paused(&true);
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 1i128]
        ),
        Err(Ok(EscrowError::Paused))
    );
}

// ---------------------------------------------------------------------------
// Payout address feature
// ---------------------------------------------------------------------------

/// Helper: create + fully fund an escrow, returning its id.
fn funded_escrow(s: &Setup) -> u64 {
    let id = s
        .escrow
        .create_contract(&s.client, &s.freelancer, &s.token.address, &vec![&s.env, 100i128]);
    s.escrow.deposit_funds(&id, &100);
    id
}

#[test]
fn get_payout_address_defaults_to_freelancer() {
    let s = setup();
    let id = funded_escrow(&s);
    assert_eq!(s.escrow.get_payout_address(&id), s.freelancer);
}

#[test]
fn release_credits_payout_address_not_freelancer() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.set_payout_address(&id, &payout_wallet);
    assert_eq!(s.escrow.get_payout_address(&id), payout_wallet);

    s.escrow.release_milestone(&id, &0);

    assert_eq!(s.token.balance(&payout_wallet), 100);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn resolve_dispute_credits_payout_address_not_freelancer() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.set_payout_address(&id, &payout_wallet);
    s.escrow.open_dispute(&id, &s.freelancer);
    // set_payout_address must be rejected while disputed
    let other = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_set_payout_address(&id, &other),
        Err(Ok(EscrowError::InvalidStatus))
    );
    s.escrow.resolve_dispute(&id, &10_000); // full share to freelancer side

    assert_eq!(s.token.balance(&payout_wallet), 100);
    assert_eq!(s.token.balance(&s.freelancer), 0);
}

#[test]
fn reputation_still_keyed_by_freelancer() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.set_payout_address(&id, &payout_wallet);
    s.escrow.release_milestone(&id, &0);
    s.escrow.issue_reputation(&id, &5);

    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
    // payout wallet gets no reputation
    let rep_payout = s.escrow.get_reputation(&payout_wallet);
    assert_eq!(rep_payout.completed_contracts, 0);
}

#[test]
fn client_cannot_set_payout_address() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    // Authorize only the client (not the freelancer). Since the contract
    // calls `c.freelancer.require_auth()`, this must fail.
    s.env.mock_auths(&[MockAuth {
        address: &s.client,
        invoke: &MockAuthInvoke {
            contract: &s.escrow.address,
            fn_name: "set_payout_address",
            args: (id, payout_wallet.clone()).try_into_val(&s.env).unwrap(),
            sub_invokes: &[],
        },
    }]);
    assert!(s.escrow.try_set_payout_address(&id, &payout_wallet).is_err());
    s.env.mock_all_auths();
}

#[test]
fn setting_payout_to_client_rejected() {
    let s = setup();
    let id = funded_escrow(&s);

    assert_eq!(
        s.escrow.try_set_payout_address(&id, &s.client),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn setting_payout_to_contract_address_rejected() {
    let s = setup();
    let id = funded_escrow(&s);
    let contract_addr = s.escrow.address.clone();

    assert_eq!(
        s.escrow.try_set_payout_address(&id, &contract_addr),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn setting_payout_during_dispute_rejected() {
    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.open_dispute(&id, &s.freelancer);
    assert_eq!(
        s.escrow.try_set_payout_address(&id, &payout_wallet),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn set_payout_address_emits_event() {
    use soroban_sdk::Symbol;

    let s = setup();
    let id = funded_escrow(&s);
    let payout_wallet = Address::generate(&s.env);

    s.escrow.set_payout_address(&id, &payout_wallet);

    // Events are (contract_address, Vec<Val> topics, Val data).
    // Our event publishes topics = (symbol_short!("payout"), id) and
    // data = (old_addr, new_addr).
    let events = s.env.events().all();
    let payout_sym: Symbol = symbol_short!("payout");
    let matching: std::vec::Vec<_> = events
        .iter()
        .filter(|(_contract, topics, _data)| {
            topics.len() >= 2
                && Symbol::try_from_val(&s.env, &topics.get(0).unwrap()).ok().as_ref()
                    == Some(&payout_sym)
        })
        .collect();
    assert!(!matching.is_empty(), "no 'payout' event found in {:?}", events);
    let (_contract, _topics, data) = &matching[0];
    let (old, new): (Address, Address) =
        <(Address, Address)>::try_from_val(&s.env, data).expect("data decode failed");
    assert_eq!(old, s.freelancer);
    assert_eq!(new, payout_wallet);
}
