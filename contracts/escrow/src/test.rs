extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, token::TokenClient, vec};

struct Setup<'a> {
    env: Env,
    escrow: EscrowClient<'a>,
    token: TokenClient<'a>,
    token_admin: StellarAssetClient<'a>,
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
    let token_admin = StellarAssetClient::new(&env, &sac.address());
    Setup {
        env,
        escrow,
        token,
        token_admin,
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

fn disputed_high_value_escrow(s: &Setup, amount: i128) -> u64 {
    s.token_admin.mint(&s.client, &amount);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, amount],
    );
    s.escrow.deposit_funds(&id, &amount);
    s.escrow.open_dispute(&id, &s.client);
    id
}

#[test]
fn panel_two_of_three_different_votes_do_not_settle_until_matching() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    let m3 = Address::generate(&s.env);
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone(), m3.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);
    // A high-value escrow cannot be settled by a single admin call.
    assert_eq!(
        s.escrow.try_resolve_dispute(&id, &5_000),
        Err(Ok(EscrowError::PanelRequired))
    );
    // Two different splits do not settle.
    s.escrow.vote_resolution(&m1, &id, &4_000);
    s.escrow.vote_resolution(&m2, &id, &6_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    assert_eq!(s.token.balance(&s.client), 1_000);
    // The second matching vote (m3 joins m1 at 4_000) settles the escrow.
    s.escrow.vote_resolution(&m3, &id, &4_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    let to_freelancer = PANEL_THRESHOLD_AMOUNT * 4_000 / BPS_DENOMINATOR;
    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    assert_eq!(
        s.token.balance(&s.client),
        1_000 + PANEL_THRESHOLD_AMOUNT - to_freelancer
    );
    // Votes are cleared once the escrow settles.
    assert_eq!(s.escrow.get_votes(&id).len(), 0);
}

#[test]
fn vote_change_replaces_vote_and_requires_membership() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    let m3 = Address::generate(&s.env);
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone(), m3.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);

    // A non-member cannot vote.
    let outsider = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_vote_resolution(&outsider, &id, &5_000),
        Err(Ok(EscrowError::Unauthorized))
    );
    // A split above 100% is rejected.
    assert_eq!(
        s.escrow.try_vote_resolution(&m1, &id, &10_001),
        Err(Ok(EscrowError::InvalidSplit))
    );
    // Parties to the escrow cannot vote, even if on the panel.
    s.escrow
        .set_panel(&vec![&s.env, s.client.clone(), m1.clone(), m2.clone()], &2);
    assert_eq!(
        s.escrow.try_vote_resolution(&s.client, &id, &5_000),
        Err(Ok(EscrowError::Unauthorized))
    );
    assert_eq!(
        s.escrow.try_vote_resolution(&s.freelancer, &id, &5_000),
        Err(Ok(EscrowError::Unauthorized))
    );

    // Changing a vote replaces it: the first vote no longer counts.
    s.escrow.vote_resolution(&m1, &id, &5_000);
    s.escrow.vote_resolution(&m1, &id, &7_000);
    let votes = s.escrow.get_votes(&id);
    assert_eq!(votes.len(), 1);
    assert_eq!(votes.get(m1.clone()), Some(7_000));
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);

    // m2 matching m1's new vote settles at 7_000.
    s.escrow.vote_resolution(&m2, &id, &7_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    let to_freelancer = PANEL_THRESHOLD_AMOUNT * 7_000 / BPS_DENOMINATOR;
    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    assert_eq!(
        s.token.balance(&s.client),
        1_000 + PANEL_THRESHOLD_AMOUNT - to_freelancer
    );
}

#[test]
fn panel_change_mid_dispute_ignores_removed_members_votes() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    let m3 = Address::generate(&s.env);
    let m4 = Address::generate(&s.env);
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone(), m3.clone()], &2);
    let id = disputed_high_value_escrow(&s, PANEL_THRESHOLD_AMOUNT);
    s.escrow.vote_resolution(&m1, &id, &3_000);
    // Rule: when the panel changes, votes from removed members stop counting.
    s.escrow
        .set_panel(&vec![&s.env, m2.clone(), m3.clone(), m4.clone()], &2);
    s.escrow.vote_resolution(&m2, &id, &3_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Disputed);
    assert_eq!(s.token.balance(&s.freelancer), 0);
    // A second vote from a current member reaches the threshold.
    s.escrow.vote_resolution(&m3, &id, &3_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    let to_freelancer = PANEL_THRESHOLD_AMOUNT * 3_000 / BPS_DENOMINATOR;
    assert_eq!(s.token.balance(&s.freelancer), to_freelancer);
    assert_eq!(
        s.token.balance(&s.client),
        1_000 + PANEL_THRESHOLD_AMOUNT - to_freelancer
    );
}

#[test]
fn below_threshold_escrow_still_settles_via_resolve_dispute() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    s.escrow.set_panel(&vec![&s.env, m1, m2], &2);
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&id, &2_500);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    assert_eq!(s.token.balance(&s.freelancer), 100);
    assert_eq!(s.token.balance(&s.client), 900);
}

#[test]
fn set_panel_validates_threshold_and_members() {
    let s = setup();
    let m1 = Address::generate(&s.env);
    let m2 = Address::generate(&s.env);
    assert_eq!(
        s.escrow
            .try_set_panel(&vec![&s.env, m1.clone(), m2.clone()], &0),
        Err(Ok(EscrowError::InvalidPanel))
    );
    assert_eq!(
        s.escrow.try_set_panel(&vec![&s.env, m1.clone()], &2),
        Err(Ok(EscrowError::InvalidPanel))
    );
    let too_many = vec![
        &s.env,
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
        Address::generate(&s.env),
    ];
    assert_eq!(
        s.escrow.try_set_panel(&too_many, &2),
        Err(Ok(EscrowError::InvalidPanel))
    );
    assert_eq!(
        s.escrow
            .try_set_panel(&vec![&s.env, m1.clone(), m2.clone(), m1.clone()], &2),
        Err(Ok(EscrowError::InvalidPanel))
    );
    s.escrow
        .set_panel(&vec![&s.env, m1.clone(), m2.clone()], &2);
    let panel = s.escrow.get_panel().unwrap();
    assert_eq!(panel.threshold, 2);
    assert_eq!(panel.members.len(), 2);
}
