extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    token::TokenClient,
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
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &lo_funds());
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

fn lo_funds() -> i128 {
    1_000_000
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
    assert_eq!(s.token.balance(&s.client), lo_funds() - 300);
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
    assert_eq!(s.token.balance(&s.client), lo_funds() - 300);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.token.balance(&s.client), lo_funds() - 300);
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
fn dispute_outcomes_freelancer_client_and_exact_threshold() {
    let s = setup();

    // 1. Freelancer-favoured resolution (> 5_000 bps): 7_500 bps
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id1, &200);
    s.escrow.open_dispute(&id1, &s.client);
    s.escrow.resolve_dispute(&id1, &7_500);

    let client_outcomes1 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes1 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes1.disputes_opened, 1);
    assert_eq!(client_outcomes1.disputes_won, 0);
    assert_eq!(client_outcomes1.disputes_lost, 1);
    assert_eq!(freelancer_outcomes1.disputes_opened, 0);
    assert_eq!(freelancer_outcomes1.disputes_won, 1);
    assert_eq!(freelancer_outcomes1.disputes_lost, 0);

    // 2. Client-favoured resolution (< 5_000 bps): 2_000 bps
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id2, &200);
    s.escrow.open_dispute(&id2, &s.freelancer);
    s.escrow.resolve_dispute(&id2, &2_000);

    let client_outcomes2 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes2 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes2.disputes_opened, 1);
    assert_eq!(client_outcomes2.disputes_won, 1);
    assert_eq!(client_outcomes2.disputes_lost, 1);
    assert_eq!(freelancer_outcomes2.disputes_opened, 1);
    assert_eq!(freelancer_outcomes2.disputes_won, 1);
    assert_eq!(freelancer_outcomes2.disputes_lost, 1);

    // 3. Exactly 5_000 bps threshold: counts as a win for freelancer, loss for client
    let id3 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id3, &200);
    s.escrow.open_dispute(&id3, &s.client);
    s.escrow.resolve_dispute(&id3, &5_000);

    let client_outcomes3 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes3 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes3.disputes_opened, 2);
    assert_eq!(client_outcomes3.disputes_won, 1);
    assert_eq!(client_outcomes3.disputes_lost, 2);
    assert_eq!(freelancer_outcomes3.disputes_opened, 1);
    assert_eq!(freelancer_outcomes3.disputes_won, 2);
    assert_eq!(freelancer_outcomes3.disputes_lost, 1);
}

#[test]
fn rate_resolved_escrow_succeeds_once_and_cancelled_rejected() {
    let s = setup();

    // Create and resolve dispute
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.open_dispute(&id, &s.client);
    s.escrow.resolve_dispute(&id, &5_000);

    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);

    // Rating a Resolved escrow succeeds once
    s.escrow.issue_reputation(&id, &4);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 4);

    // Second rating attempt is rejected
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &4),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );

    // Cancelled escrow rating is rejected
    let id_cancel = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id_cancel, &50);
    s.escrow.cancel_contract(&id_cancel);
    assert_eq!(
        s.escrow.get_contract(&id_cancel).status,
        ContractStatus::Cancelled
    );
    assert_eq!(
        s.escrow.try_issue_reputation(&id_cancel, &5),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn cancel_unfunded_escrow_does_not_count_cancellation() {
    let s = setup();

    // 1. Unfunded cancellation -> cancellations remains 0
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.cancel_contract(&id1);
    assert_eq!(s.escrow.get_outcomes(&s.client).cancellations, 0);

    // 2. Funded cancellation -> cancellations increments to 1
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id2, &100);
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.escrow.get_outcomes(&s.client).cancellations, 1);
}

// --- Deadline / reclaim tests ---

fn now(s: &Setup) -> u64 {
    s.env.ledger().timestamp()
}

fn advance(s: &Setup, secs: u64) {
    s.env.ledger().set_timestamp(s.env.ledger().timestamp() + secs);
}

#[test]
fn deadline_boundary_and_refund() {
    let s = setup();
    let deadline = now(&s) + 1000;
    let id = s.escrow.create_contract_with_deadlines(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
        &vec![&s.env, deadline],
    );
    s.escrow.deposit_funds(&id, &100);
    assert_eq!(s.token.balance(&s.client), lo_funds() - 100);
    // One second before deadline + grace.
    let target = deadline + DEADLINE_GRACE_SECS;
    s.env.ledger().set_timestamp(target - 1);
    assert_eq!(
        s.escrow.try_reclaim_overdue(&id, &0),
        Err(Ok(EscrowError::NotOverdue))
    );
    // At deadline + grace.
    s.env.ledger().set_timestamp(target);
    s.escrow.reclaim_overdue(&id, &0);
    assert_eq!(s.token.balance(&s.client), lo_funds());
    assert_eq!(s.token.balance(&s.freelancer), 0);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.funded_amount - c.released_amount, 0);
    assert_eq!(c.status, ContractStatus::Cancelled);
    // Reclaim twice rejected.
    assert_eq!(
        s.escrow.try_reclaim_overdue(&id, &0),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );
}

#[test]
fn reclaim_milestone_zero_of_three() {
    let s = setup();
    let d = now(&s) + 1000;
    let id = s.escrow.create_contract_with_deadlines(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128, 300i128],
        &vec![&s.env, d, d + 1000, d + 2000],
    );
    s.escrow.deposit_funds(&id, &600);
    assert_eq!(s.token.balance(&s.client), lo_funds() - 600);
    s.env.ledger().set_timestamp(d + DEADLINE_GRACE_SECS);
    s.escrow.reclaim_overdue(&id, &0);
    assert_eq!(s.token.balance(&s.client), lo_funds() - 500);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Funded);
    assert_eq!(c.funded_amount - c.released_amount, 500);
    // Milestones 1 and 2 remain releasable.
    s.escrow.release_milestone(&id, &1);
    s.escrow.release_milestone(&id, &2);
    assert_eq!(s.token.balance(&s.freelancer), 500);
    let c = s.escrow.get_contract(&id);
    assert_eq!(c.status, ContractStatus::Completed);
    assert_eq!(c.funded_amount - c.released_amount, 0);
    // Releasing a reclaimed milestone is rejected.
    assert_eq!(
        s.escrow.try_release_milestone(&id, &0),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );
}

#[test]
fn invalid_deadline_vectors_rejected() {
    let s = setup();
    let d = now(&s) + 1000;
    // Length mismatch.
    assert_eq!(
        s.escrow.try_create_contract_with_deadlines(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 100i128, 200i128],
            &vec![&s.env, d],
        ),
        Err(Ok(EscrowError::InvalidDeadline))
    );
    // Past timestamp.
    assert_eq!(
        s.escrow.try_create_contract_with_deadlines(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 100i128],
            &vec![&s.env, now(&s)],
        ),
        Err(Ok(EscrowError::InvalidDeadline))
    );
    // Decreasing.
    assert_eq!(
        s.escrow.try_create_contract_with_deadlines(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 100i128, 200i128],
            &vec![&s.env, d + 1000, d],
        ),
        Err(Ok(EscrowError::InvalidDeadline))
    );
}

#[test]
fn reclaim_without_deadline_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    advance(&s, DEADLINE_GRACE_SECS + 10_000);
    assert_eq!(
        s.escrow.try_reclaim_overdue(&id, &0),
        Err(Ok(EscrowError::InvalidDeadline))
    );
}
