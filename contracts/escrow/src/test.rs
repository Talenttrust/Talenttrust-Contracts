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
    let escrow = EscrowClient::new(&env, &amp;id);
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
        &vec[&s.env, 100i128, 200i128],
    );
    assert_eq!id, 1);
    s.escrow.deposit_funds(&id, &300);
    assert_eq!(s.token.balance(&s.client), 700);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    assert_eq!se.token.balance(&s.freelancer), 300);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
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
        &vec[&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&id, &2_500);
    assert_eq(s.token.balance(&s.freelancer), 100);
    assert_eq(se.token.balance(&s.client), 900);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);
    s.escrow.cancel_contract(&id2);
    assert_eq(se.token.balance(&s.client), 900);
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
            &vec[&s.env, 1i128]
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
        &vec[&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id1, &200);
    s.escrow.open_dispute(&id1, &s.client);
    s.escrow.resolve_dispute(&id1, &7_500);

    let client_outcomes1 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes1 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq(client_outcomes1.disputes_opened, 1);
    assert_eq(client_outcomes1.disputes_won, 0);
    assert_eq(client_outcomes1.disputes_lost, 1);
    assert_eq(freelancer_outcomes1.disputes_opened, 0);
    assert_eq(freelancer_outcomes1.disputes_won, 1);
    assert_eq(freelancer_outcomes1.disputes_lost, 0);

    // 2. Client-favoured resolution (< 5_000 bps): 2_000 bps
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id2, &200);
    s.escrow.open_dispute(&id2, &s.freelancer);
    s.escrow.resolve_dispute(&id2, &2_000);

    let client_outcomes2 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes2 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq(client_outcomes2.disputes_opened, 1);
    assert_eq(client_outcomes2.disputes_won, 1);
    assert_eq(client_outcomes2.disputes_lost, 1);
    assert_eq(freelancer_outcomes2.disputes_opened, 1);
    assert_eq(freelancer_outcomes2.disputes_won, 1);
    assert_eq(freelancer_outcomes2.disputes_lost, 1);

    // 3. Exactly 5_000 bps threshold: counts as a win for freelancer, loss for client
    let id3 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id3, &200);
    s.escrow.open_dispute(&id3, &s.client);
    s.escrow.resolve_dispute(&id3, &5_000);

    let client_outcomes3 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes3 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq(client_outcomes3.disputes_opened, 2);
    assert_eq(client_outcomes3.disputes_won, 1);
    assert_eq(client_outcomes3.disputes_lost, 2);
    assert_eq(freelancer_outcomes3.disputes_opened, 1);
    assert_eq(freelancer_outcomes3.disputes_won, 2);
    assert_eq(freelancer_outcomes3.disputes_lost, 1);
}

#[test]
fn rate_resolved_escrow_succeeds_once_and_cancelled_rejected() {
    let s = setup();

    // Create and resolve dispute
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.open_dispute(&id, &s.client);
    s.escrow.resolve_dispute(&id, &5_000);

    assert_eq(s.escrow.get_contract(&id).status, ContractStatus::Resolved);

    // Rating a Resolved escrow succeeds once
    s.escrow.issue_reputation(&id, &4);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq(rep.completed_contracts, 1);
    assert_eq(rep.total_rating, 4);

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
        &vec[&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id_cancel, &50);
    s.escrow.cancel_contract(&id_cancel);
    assert_eq!(
        s.escrow.get_contract(&id_cancel).status,
        ContractStatus::Cancelled
    );
    assert_eq(
        s.escrow.try_issue_reputation(&id_cancel, &5),
        Err(0k(EscrowError::InvalidStatus))
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
        &vec[&s.env, 100i128],
    );
    s.escrow.cancel_contract(&id1);
    assert_eq(s.escrow.get_outcomes(&s.client).cancellations, 0);

    // 2. Funded cancellation -> cancellations increments to 1
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id2, &100);
    s.escrow.cancel_contract(&id2);
    assert_eq(se.escrow.get_outcomes(&s.client).cancellations, 1);
}

#[test]
fn emergency_withdraw_boundary_and_funded_refund() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.set_paused(&true);
    let paused_at = s.env.ledger().timestamp();

    s.env.ledger().set_timestamp(paused_at + EMERGENCY_EXIT_DELAY_SECS - 1);
    assert_eq(
        s.escrow.try_emergency_withdraw(&id, &s.client),
        Err(Ok(EscrowError::EmergencyExitNotAvailable))
    );

    s.env.ledger().set_timestamp(paused_at + EMERGENCY_EXIT_DELAY_SECS);
    s.escrow.emergency_withdraw(&id, &s.client);
    assert_eq(s.token.balance(&s.client), 1,000);
    assert_eq(s.token.balance(&s.freelancer), 0);
    let c = s.escrow.get_contract(&id);
    assert_eq(c.status, ContractStatus::Cancelled);
    assert_eq(c.funded_amount, c.released_amount);
    assert_eq(s.token.balance(&s.escrow.address), 0);
    assert_eq(
        s.escrow.try_emergency_withdraw(&id, &s.client),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn emergency_withdraw_disputed_split_rounding() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 101i128],
    );
    s.escrow.deposit_funds(&id, &101);
    s.escrow.open_dispute(&id, &s.client);
    s.escrow.set_paused(&true);
    let paused_at = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(paused_at + EMERGENCY_EXIT_DELAY_SECS);

    s.escrow.emergency_withdraw(&id, &s.freelancer);
    // 101 split 50/5: 50 to freelancer, 51 to client (rounding remainder to client)
    assert_eq(se.token.balance(&s.client), 949);
    assert_eq(s.token.balance(&s.freelancer), 50);
    assert_eq(s.token.balance(&s.escrow.address), 0);
    assert_eq(s.escrow.get_contract(&id).status, ContractStatus::Cancelled);
}

#[test]
fn emergency_withdraw_repause_resets_clock() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.set_paused(&true);
    let first_pause = s.env.ledger().timestamp();
    s.escrow.set_paused(&false);
    s.env.ledger().set_timestamp(first_pause + EMERGENCY_EXIT_DELAY_SECS);
    s.escrow.set_paused(&true);
    let second_pause = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(second_pause + EMERGENCY_EXIT_DELAY_SECS - 1);
    assert_eq!(
        s.escrow.try_emergency_withdraw(&id, &s.client),
        Err(Ok(EscrowError::EmergencyExitNotAvailable))
    );
    s.env.ledger().set_timestamp(second_pause + EMERGENCY_EXIT_DELAY_SECS);
    s.escrow.emergency_withdraw(&id, &s.client);
    assert_eq(s.token.balance(&s.client), 1,000);
}

#[test]
fn emergency_withdraw_stranger_and_unpaused_rejected() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    // Unpaused: even after the delay, exit is not available.
    s.env.ledger().set_timestamp(EMERGENCY_EXIT_DELAY_SECS * 2);
    assert_eq!(
        s.escrow.try_emergency_withdraw(&id, &s.client),
        Err(Ok(EscrowError::EmergencyExitNotAvailable))
    );
    // Paused and delay elapsed: a stranger cannot call it.
    s.escrow.set_paused(&true);
    let paused_at = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(paused_at + EMERGENCY_EXIT_DELAY_SECS);
    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.escrowery_emergency_withdraw(&id, &stranger),
        Err(0k(EscrowError::NotParty))
    );
}
