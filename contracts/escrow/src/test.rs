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

#[test]
fn summary_tracks_lifecycle_and_milestones() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128, 300i128],
    );

    s.escrow.deposit_funds(&id, &100);
    assert_eq!(
        s.escrow.get_escrow_summary(&id),
        EscrowSummary {
            id,
            status: ContractStatus::Created,
            total_amount: 600,
            funded_amount: 100,
            released_amount: 0,
            held: 100,
            remaining_to_fund: 500,
            milestones_total: 3,
            milestones_released: 0,
            next_unreleased: Some(0),
            next_release_affordable: true,
        }
    );

    s.escrow.deposit_funds(&id, &500);
    assert_eq!(
        s.escrow.get_escrow_summary(&id),
        EscrowSummary {
            id,
            status: ContractStatus::Funded,
            total_amount: 600,
            funded_amount: 600,
            released_amount: 0,
            held: 600,
            remaining_to_fund: 0,
            milestones_total: 3,
            milestones_released: 0,
            next_unreleased: Some(0),
            next_release_affordable: true,
        }
    );

    s.escrow.release_milestone(&id, &0);
    assert_eq!(
        s.escrow.get_escrow_summary(&id),
        EscrowSummary {
            id,
            status: ContractStatus::Funded,
            total_amount: 600,
            funded_amount: 600,
            released_amount: 100,
            held: 500,
            remaining_to_fund: 0,
            milestones_total: 3,
            milestones_released: 1,
            next_unreleased: Some(1),
            next_release_affordable: true,
        }
    );
    assert_eq!(
        s.escrow.get_milestone(&id, &0),
        Milestone {
            amount: 100,
            released: true,
        }
    );
    assert_eq!(
        s.escrow.get_milestone(&id, &1),
        Milestone {
            amount: 200,
            released: false,
        }
    );
    assert_eq!(
        s.escrow.try_get_milestone(&id, &3),
        Err(Ok(EscrowError::MilestoneNotFound))
    );

    s.escrow.release_milestone(&id, &1);
    s.escrow.release_milestone(&id, &2);
    assert_eq!(
        s.escrow.get_escrow_summary(&id),
        EscrowSummary {
            id,
            status: ContractStatus::Completed,
            total_amount: 600,
            funded_amount: 600,
            released_amount: 600,
            held: 0,
            remaining_to_fund: 0,
            milestones_total: 3,
            milestones_released: 3,
            next_unreleased: None,
            next_release_affordable: false,
        }
    );
}

#[test]
fn next_release_affordable_requires_full_milestone_coverage() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128],
    );

    s.escrow.deposit_funds(&id, &99);
    let summary = s.escrow.get_escrow_summary(&id);
    assert_eq!(summary.status, ContractStatus::Created);
    assert_eq!(summary.next_unreleased, Some(0));
    assert!(!summary.next_release_affordable);

    s.escrow.deposit_funds(&id, &1);
    let summary = s.escrow.get_escrow_summary(&id);
    assert_eq!(summary.next_unreleased, Some(0));
    assert!(summary.next_release_affordable);

    s.escrow.release_milestone(&id, &0);
    s.escrow.deposit_funds(&id, &100);
    let summary = s.escrow.get_escrow_summary(&id);
    assert_eq!(summary.next_unreleased, Some(1));
    assert!(!summary.next_release_affordable);
}

#[test]
fn summary_after_cancel_and_resolve() {
    let s = setup();
    let cancelled = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 100i128],
    );
    s.escrow.deposit_funds(&cancelled, &200);
    s.escrow.release_milestone(&cancelled, &0);
    s.escrow.cancel_contract(&cancelled);
    assert_eq!(
        s.escrow.get_escrow_summary(&cancelled),
        EscrowSummary {
            id: cancelled,
            status: ContractStatus::Cancelled,
            total_amount: 200,
            funded_amount: 100,
            released_amount: 100,
            held: 0,
            remaining_to_fund: 100,
            milestones_total: 2,
            milestones_released: 1,
            next_unreleased: Some(1),
            next_release_affordable: false,
        }
    );

    let resolved = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
    );
    s.escrow.deposit_funds(&resolved, &400);
    s.escrow.open_dispute(&resolved, &s.freelancer);
    s.escrow.resolve_dispute(&resolved, &2_500);
    assert_eq!(
        s.escrow.get_escrow_summary(&resolved),
        EscrowSummary {
            id: resolved,
            status: ContractStatus::Resolved,
            total_amount: 400,
            funded_amount: 100,
            released_amount: 100,
            held: 0,
            remaining_to_fund: 300,
            milestones_total: 1,
            milestones_released: 0,
            next_unreleased: Some(0),
            next_release_affordable: false,
        }
    );
}

#[test]
fn batch_summaries_skip_unknown_and_enforce_cap() {
    let s = setup();
    let first = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 10i128],
    );
    let second = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 20i128],
    );
    let third = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 30i128],
    );

    let summaries = s
        .escrow
        .get_escrow_summaries(&vec![&s.env, first, 99, second, third]);
    assert_eq!(summaries.len(), 3);
    assert_eq!(summaries.get(0).unwrap().id, first);
    assert_eq!(summaries.get(1).unwrap().id, second);
    assert_eq!(summaries.get(2).unwrap().id, third);

    let mut max_ids = Vec::new(&s.env);
    for i in 0..MAX_SUMMARY_BATCH {
        max_ids.push_back(1_000 + i as u64);
    }
    assert_eq!(s.escrow.get_escrow_summaries(&max_ids).len(), 0);

    max_ids.push_back(1_000 + MAX_SUMMARY_BATCH as u64);
    assert_eq!(
        s.escrow.try_get_escrow_summaries(&max_ids),
        Err(Ok(EscrowError::InvalidAmount))
    );
}
