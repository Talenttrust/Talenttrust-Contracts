extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, token::TokenClient, vec};

struct Setup<'a> {
    env: Env,
    escrow: EscrowClient<'a>,
    token: TokenClient<'a>,
    client: Address,
    freelancer: Address,
    admin: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    // Mint enough to cover MAX_ESCROW_TOTAL boundary tests (10^18 + buffer).
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &MAX_ESCROW_TOTAL);
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
        admin,
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
        &vec![&s.env, 1_000_000i128, 2_000_000i128],
    );
    assert_eq!(id, 1);
    s.escrow.deposit_funds(&id, &3_000_000);
    assert_eq!(s.token.balance(&s.client), MAX_ESCROW_TOTAL - 3_000_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.token.balance(&s.freelancer), 3_000_000);
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
        &vec![&s.env, 1_000_000i128],
    );
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &1_000_001),
        Err(Ok(EscrowError::OverFunded))
    );
    s.escrow.deposit_funds(&id, &1_000_000);
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
        &vec![&s.env, 4_000_000i128],
    );
    s.escrow.deposit_funds(&id, &4_000_000);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&id, &2_500);
    assert_eq!(s.token.balance(&s.freelancer), 1_000_000);
    assert_eq!(s.token.balance(&s.client), MAX_ESCROW_TOTAL - 4_000_000 + 3_000_000);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 1_000_000i128],
    );
    s.escrow.deposit_funds(&id2, &1_000_000);
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.token.balance(&s.client), MAX_ESCROW_TOTAL - 4_000_000 + 3_000_000);
    assert_eq!(
        s.escrow.get_contract(&id2).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn pause_blocks_mutations() {
    let s = setup();
    s.escrow.set_paused(&true);
    // Paused check fires before any party or amount validation
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 1_000_000i128]
        ),
        Err(Ok(EscrowError::Paused))
    );
}

// ---------------------------------------------------------------------------
// create_contract input validation
// ---------------------------------------------------------------------------

/// Helper that builds a valid milestone vector for a given amount.
fn milestones(s: &Setup, amount: i128) -> Vec<i128> {
    vec![&s.env, amount]
}

#[test]
fn rejects_client_equal_to_contract_address() {
    let s = setup();
    let contract_addr = s.escrow.address.clone();
    assert_eq!(
        s.escrow.try_create_contract(
            &contract_addr,
            &s.freelancer,
            &s.token.address,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn rejects_freelancer_equal_to_contract_address() {
    let s = setup();
    let contract_addr = s.escrow.address.clone();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &contract_addr,
            &s.token.address,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn rejects_token_equal_to_contract_address() {
    let s = setup();
    let contract_addr = s.escrow.address.clone();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &contract_addr,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::InvalidParties))
    );
}

#[test]
fn rejects_client_equal_to_admin() {
    let s = setup();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.admin,
            &s.freelancer,
            &s.token.address,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::AdminCannotBeParty))
    );
}

#[test]
fn rejects_freelancer_equal_to_admin() {
    let s = setup();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.admin,
            &s.token.address,
            &milestones(&s, 1_000_000),
        ),
        Err(Ok(EscrowError::AdminCannotBeParty))
    );
}

#[test]
fn rejects_milestone_below_minimum() {
    let s = setup();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &milestones(&s, MIN_MILESTONE_AMOUNT - 1),
        ),
        Err(Ok(EscrowError::MilestoneTooSmall))
    );
}

#[test]
fn accepts_milestone_at_minimum() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &milestones(&s, MIN_MILESTONE_AMOUNT),
    );
    assert_eq!(id, 1);
}

#[test]
fn rejects_total_above_maximum() {
    let s = setup();
    // Two milestones each at MAX_ESCROW_TOTAL / 2 + 1 — total exceeds the cap
    // while each individual milestone is valid.
    let half = MAX_ESCROW_TOTAL / 2;
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, half + 1, half + 1],
        ),
        Err(Ok(EscrowError::InvalidAmount))
    );
}

#[test]
fn accepts_total_at_maximum() {
    let s = setup();
    // Two milestones summing exactly to MAX_ESCROW_TOTAL
    let half = MAX_ESCROW_TOTAL / 2;
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, half, MAX_ESCROW_TOTAL - half],
    );
    assert_eq!(id, 1);
}

#[test]
fn validation_order_parties_before_amounts() {
    // An input that violates BOTH the party rule (client == admin) and the
    // amount rule (below minimum) must return the party error first.
    let s = setup();
    assert_eq!(
        s.escrow.try_create_contract(
            &s.admin,
            &s.freelancer,
            &s.token.address,
            &milestones(&s, 1), // below MIN_MILESTONE_AMOUNT
        ),
        Err(Ok(EscrowError::AdminCannotBeParty))
    );
}
