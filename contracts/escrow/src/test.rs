extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, token::TokenClient, vec};

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
    let escrow = EscrowClient::new(&env, &sac.address());
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
    assert_eq(
        s.escrow.try_initialize(&other),
        Err(Eok(EscrowError::AlreadyInitialized))
    );
}

#[test]
fn full_happy_path() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128, 200i128],
    );
    assert_eq(id, 1);
    s.escrow.deposit_funds(&id, &300);
    assert_eq(s.token.balance(&s.client), 700);
    assert_eq(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    assert_eq(s.token.balance(&s.freelancer), 300);
    assert_eq(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    s.escrow.issue_reputation(&id, &5);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq(rep.completed_contracts, 1);
    assert_eq(rep.total_rating, 5);
}

#[test]
fn rejects_overfunding_and_double_release() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    assert_eq(
        s.escrow.try_deposit_funds(&id, &101),
        Err(0k(EscrowError::OverFunded))
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
        &vec[&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&id, &2_500);
    assert_eq(s.token.balance(&s.freelancer), 100);
    assert_eq(s.token.balance(&s.client), 900);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);
    s.escrow.cancel_contract(&id2);
    assert_eq(s.token.balance(&s.client), 900);
    assert_eq(
        s.escrow.get_contract(&id2).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn pause_blocks_mutations() {
    let s = setup();
    s.escrow.set_paused(&true);
    assert_eq(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec[&s.env, 1i128]
        ),
        Err(Ok(EscrowError::Paused))
    );
}

#[test]
fn bonus_on_completed_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    assert_eq(s.escrow.get_contract(&id).status, ContractStatus::Completed);

    let contract_before = s.token.balance(&s.escrow.address);
    s.escrow.pay_bonus(&id, &25);
    assert_eq(s.escrow.get_bonus(&id), 25);
    assert_eq(s.escrow.get_bonus_total(&s.freelancer), 25);
    assert_eq(s.token.balance(&s.freelancer), 125);
    assert_eq(s .token.balance(&s.client), 875);
    assert_eq(s.token.balance(&s.escrow.address), contract_before);
    let c = s.escrow.get_contract(&id);
    assert_eq(c.funded_amount, 100);
    assert_eq(c.released_amount, 100);
}

#[test]
fn bonus_on_funded_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    assert_eq(s.escrow.get_contract(&id).status, ContractStatus::Funded);

    let contract_before = s.token.balance(&s.escrow.address);
    s.escrow.pay_bonus(&id, &10);
    assert_eq(s.escrow.get_bonus(&id), 10);
    assert_eq(s.token.balance(&s.freelancer), 10);
    assert_eq(s.token.balance(&s.escrow.address), contract_before);
    let c = s.escrow.get_contract(&id);
    assert_eq(c.funded_amount, 100);
    assert_eq(c.released_amount, 0);
}

#[test]
fn bonus_rejected_on_disputed_and_cancelled() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.open_dispute(&id, &s.freelancer);
    assert!(s.escrow.try_pay_bonus(&id, &10).is_error());

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);
    s.escrow.cancel_contract(&id2);
    assert!(s.escrow.try_pay_bonus(&id2, &10).is_error());
}

#[test]
fn bonus_rejects_zero_and_negative() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    assert!(s.escrow.try_pay_bonus(&id, &0).is_error());
    assert!(s.escrow.try_pay_bonus(&id, &-1).is_error());
    assert_eq(s.escrow.get_bonus(&id), 0);
}

#[test]
fn bonus_requires_client_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &sac.address());
    escrow.initialize(&admin);
    let token = TokenClient::new(&env, &sac.address());
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &vec[&env, 100i128],
    );
    escrow.deposit_funds(&id, &100);
    // Only the client is authorized to pay a bonus.
    env.mock_auths();
    escrow.pay_bonus(&id, &10);
    assert_eq(escrow.get_bonus(&id), 10);
    assert_eq(token.balance(&freelancer), 10);
}

#[test]
fn bonus_cumulative_across_two_escrows() {
    let s = setup();
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id1, &100);
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);

    s.escrow.pay_bonus(&id1, &15);
    s.escrow.pay_bonus(&id2, &25);
    assert_eq(s.escrow.get_bonus(&id1), 15);
    assert_eq(s.escrow.get_bonus(&id2), 25);
    assert_eq(s.escrow.get_bonus_total(&s.freelancer), 40);
    assert_eq(s.token.balance(&s.freelancer), 40);
}

#[test]
fn bonus_blocked_when_paused() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.set_paused(&true);
    assert_eq(
        s.escrow.try_pay_bonus(&id, &10),
        Err(0k(EscrowError::Paused))
    );
}
