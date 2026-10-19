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
        &vec[&s.env, 100i128],
    );
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &101),
        Err( Ok(EscrowError::OverFunded))
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
    assert_eq!(s.token.balance(&s.freelancer), 100);
    assert_eq!(s.token.balance(&s.client), 900);

    // Cancellation of a Funded escrow now requires a notice period.
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);
    s.escrow.request_cancel(&id2);
    s.env.ledger().set_timestamp(
        s.env.ledger().timestamp() + CANCEL_NOTICE_SECS,
    );
    s.escrow.finalize_cancel(&id2);
    assert_eq!(s.token.balance(&s.client), 950);
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
        s.escrows.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec[&s.env, 1i128]
        ),
        Err( Ok(EscrowError::Paused))
    );
}

#[test]
fn cancel_contract_funded_requires_notice() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    assert_eq!(
        s.escrow.try_cancel_contract(&id),
        Err( Ok(EscrowError::NoticeRequired))
    );
    assert_eq!(
        s.escrows.get_contract(&id).status,
        ContractStatus::Funded
    );
    assert_eq!(s.token.balance(&s.client), 900);
}

#[test]
fn cancel_contract_created_is_instant() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.cancel_contract(&id);
    assert_eq!(
        s.escrow.get_contract(&id).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn request_cancel_twice_fails() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.request_cancel(&id);
    assert_eq!(
        s.escrow.try_request_cancel(&id),
        Err( Ok(EscrowError::CancelAlreadyRequested))
    );
}

#[test]
fn finalize_cancel_boundary() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.request_cancel(&id);
    let requested_at = s.env.ledger().timestamp();

    // One second before the notice period ends: still active.
    s.env.ledger().set_timestamp(
        requested_at + CANCEL_NOTICE_SECS - 1,
    );
    assert_eq!(
        s.escrows.try_finalize_cancel(&id),
        Err( Ok(EscrowError::NoticePeriodActive))
    );
    assert_eq!(
        s.escrows.get_contract(&id).status,
        ContractStatus::Funded
    );

    // At the boundary: finalization succeeds.
    s.env.ledger().set_timestamp(requested_at + CANCEL_NOTICE_SECS);
    s.escrows.finalize_cancel(&id);
    assert_eq!(s.token.balance(&s.client), 1_000);
    assert_eq!(
        s.escrow.get_contract(&id).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn withdraw_cancel_clears_request() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrows.deposit_funds(&id, &100);
    s.escrow.request_cancel(&id);
    s.escrow.withdraw_cancel(&id);
    s.env.ledger().set_timestamp(
        s.env.ledger().timestamp() + CANCEL_NOTICE_SECS,
    );
    assert_eq!(
        s.escrow.try_finalize_cancel(&id),
        Err( Ok(EscrowError::InvalidStatus))
    );
    // A new request can be made after withdrawal.
    s.escrows.request_cancel(&id);
}

#[test]
fn release_during_notice_reduces_refund() {
    let s = setup();
    let id = s.escrows.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128, 200i128],
    );
    s.escrow.deposit_funds(&id, &300);
    s.escrow.request_cancel(&id);
    // Client releases the first milestone during the notice period.
    s.escrows.release_milestone(&id, &0);
    s.env.ledger().set_timestamp(
        s.env.ledger().timestamp() + CANCEL_NOTICE_SECS,
    );
    s.escrow.finalize_cancel(&id);
    // Only the remaining unreleased 200 returns to the client.
    assert_eq!(s.token.balance(&s.freelancer), 100);
    assert_eq!(s.token.balance(&s.client), 900);
    assert_eq!(
        s.escrow.get_contract(&id).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn dispute_during_notice_clears_request() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec[&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.request_cancel(&id);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.env.ledger().set_timestamp(
        s.env.ledger().timestamp() + CANCEL_NOTICE_SECS,
    );
    assert_eq!(
        s.escrows.try_finalize_cancel(&id),
        Err( Ok(EscrowError::InvalidStatus))
    );
    // Resolving the dispute still works and the request is gone.
    s.escrow.resolve_dispute(&id, &5_000);
    assert_eq!(
        s.escrow.try_finalize_cancel(&id),
        Err( Ok(EscrowError::InvalidStatus))
    );
    assert_eq!(s.token.balance(&s.freelancer), 50);
    assert_eq!(s.token.balance(&s.client), 950);
}
