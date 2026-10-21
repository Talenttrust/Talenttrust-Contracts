extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, token::StellarAssetClient, token::TokenClient, vec};

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
fn compact_cancelled_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.cancel_contract(&id);

    // Should be able to compact a cancelled escrow
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );

    // Receipt should exist and match the escrow
    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_some());
    let r = receipt.unwrap();
    assert_eq!(r.id, id);
    assert_eq!(r.status, ContractStatus::Cancelled);
    assert_eq!(r.total_amount, 100);
    assert_eq!(r.released_amount, 0);
    assert_eq!(r.refunded_amount, 100);
    assert_eq!(r.milestone_count, 1);
    assert!(!r.reputation_issued);
}

#[test]
fn compact_resolved_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&id, &2_500); // 25% to freelancer

    // Should be able to compact a resolved escrow
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );

    // Receipt should exist and match the escrow
    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_some());
    let r = receipt.unwrap();
    assert_eq!(r.id, id);
    assert_eq!(r.status, ContractStatus::Resolved);
    assert_eq!(r.total_amount, 400);
    assert_eq!(r.released_amount, 100); // 25% of 400
    assert_eq!(r.refunded_amount, 300); // 75% of 400
    assert_eq!(r.milestone_count, 1);
    assert!(!r.reputation_issued);
}

#[test]
fn compact_completed_escrow_after_rating() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128],
    );
    s.escrow.deposit_funds(&id, &300);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    s.escrow.issue_reputation(&id, &5);

    // Should be able to compact a completed escrow after rating
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );

    // Receipt should exist and match the escrow
    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_some());
    let r = receipt.unwrap();
    assert_eq!(r.id, id);
    assert_eq!(r.status, ContractStatus::Completed);
    assert_eq!(r.total_amount, 300);
    assert_eq!(r.released_amount, 300);
    assert_eq!(r.refunded_amount, 0);
    assert_eq!(r.milestone_count, 2);
    assert!(r.reputation_issued);
}

#[test]
fn compact_completed_escrow_after_30_days() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);

    // Cannot compact immediately without rating
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    // Advance 30 days in ledgers
    s.env.ledger().set_sequence_number(518_400u32);

    // Now should be able to compact
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );

    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_some());
    let r = receipt.unwrap();
    assert_eq!(r.status, ContractStatus::Completed);
    assert!(!r.reputation_issued);
}

#[test]
fn cannot_compact_live_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );

    // Created status cannot be compacted
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    s.escrow.deposit_funds(&id, &100);

    // Funded status cannot be compacted
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    s.escrow.open_dispute(&id, &s.freelancer);

    // Disputed status cannot be compacted
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
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
    s.escrow.open_dispute(&id, &s.freelancer);
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
fn cannot_compact_completed_before_30_days_without_rating() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);

    // Cannot compact without rating and before 30 days
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    // Advance 29 days (just under the threshold)
    s.env.ledger().set_sequence_number(518_400u32 - 1);

    // Still cannot compact
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
}

#[test]
fn cannot_double_compact() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.cancel_contract(&id);

    // First compaction succeeds
    s.escrow.compact_escrow(&id);

    // Second compaction fails - receipt already exists
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
}

#[test]
fn receipt_fields_match_escrow_before_compaction() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128, 300i128],
    );
    s.escrow.deposit_funds(&id, &600);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);

    // Cancel to make it terminal
    s.escrow.cancel_contract(&id);

    // Get the escrow after cancellation (it has the final state)
    let escrow = s.escrow.get_contract(&id);

    // Compact
    s.escrow.compact_escrow(&id);

    // Check receipt matches
    let receipt = s.escrow.get_receipt(&id).unwrap();
    assert_eq!(receipt.id, escrow.id);
    assert_eq!(receipt.client, escrow.client);
    assert_eq!(receipt.freelancer, escrow.freelancer);
    assert_eq!(receipt.token, escrow.token);
    assert_eq!(receipt.total_amount, escrow.total_amount);
    assert_eq!(receipt.released_amount, escrow.released_amount);
    assert_eq!(receipt.refunded_amount, escrow.refunded_amount);
    assert_eq!(receipt.status, escrow.status);
    assert_eq!(receipt.milestone_count, escrow.milestones.len() as u32);
    assert_eq!(receipt.reputation_issued, escrow.reputation_issued);
}

#[test]
fn issue_reputation_on_compacted_unrated_escrow_before_30_days() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);

    // Try to compact before 30 days - should fail
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    // Should still be able to issue reputation
    s.escrow.issue_reputation(&id, &5);

    // Now compaction should succeed
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );
}



#[test]
fn get_contract_returns_escrow_compacted() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.cancel_contract(&id);
    s.escrow.compact_escrow(&id);

    // get_contract should return EscrowCompacted error
    assert_eq!(
        s.escrow.try_get_contract(&id),
        Err(Ok(EscrowError::EscrowCompacted))
    );
}

#[test]
fn get_receipt_returns_none_for_non_compacted() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );

    // Receipt should not exist for non-compacted escrow
    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_none());
}

