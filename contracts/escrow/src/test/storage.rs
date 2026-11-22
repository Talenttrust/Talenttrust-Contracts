//! Storage layout, TTL, compaction and persistence behaviour.

use super::*;

#[test]
fn compact_cancelled_escrow() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
    assert_eq!(r.total_amount, 100 * U);
    assert_eq!(r.released_amount, 0);
    assert_eq!(r.refunded_amount, 100 * U);
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
        &vec![&s.env, 400 * U],
    );
    s.escrow.deposit_funds(&id, &(400 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);
    resolve_refunding_bond(&s, &id, &2_500); // 25% to freelancer

    // Resolved escrows can still be rated, so compaction waits for the rating
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
    s.escrow.issue_reputation(&id, &4);

    // Should be able to compact a rated resolved escrow
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
    assert_eq!(r.total_amount, 400 * U);
    assert_eq!(r.released_amount, 100 * U); // 25% of 400
    assert_eq!(r.refunded_amount, 300 * U); // 75% of 400
    assert_eq!(r.milestone_count, 1);
    assert!(r.reputation_issued);
}

#[test]
fn compact_completed_escrow_after_rating() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U, 200 * U],
    );
    s.escrow.deposit_funds(&id, &(300 * U));
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
    assert_eq!(r.total_amount, 300 * U);
    assert_eq!(r.released_amount, 300 * U);
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
        &vec![&s.env, 100 * U],
    );

    // Created status cannot be compacted
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    s.escrow.deposit_funds(&id, &(100 * U));

    // Funded status cannot be compacted
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );

    open_dispute_bonded(&s, &id, &s.freelancer);

    // Disputed status cannot be compacted
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
}

#[test]
fn cannot_compact_completed_before_30_days_without_rating() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
        &vec![&s.env, 100 * U, 200 * U, 300 * U],
    );
    s.escrow.deposit_funds(&id, &(600 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
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
        &vec![&s.env, 100 * U],
    );

    // Receipt should not exist for non-compacted escrow
    let receipt = s.escrow.get_receipt(&id);
    assert!(receipt.is_none());
}

#[test]
fn compact_resolved_unrated_escrow_after_30_days() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100 * U],
    );
    s.escrow.deposit_funds(&id, &(100 * U));
    open_dispute_bonded(&s, &id, &s.freelancer);
    resolve_refunding_bond(&s, &id, &5_000);

    s.env.ledger().set_sequence_number(THIRTY_DAYS_IN_LEDGERS - 1);
    assert_eq!(
        s.escrow.try_compact_escrow(&id),
        Err(Ok(EscrowError::NotCompactable))
    );
    s.env.ledger().set_sequence_number(THIRTY_DAYS_IN_LEDGERS);
    s.escrow.compact_escrow(&id);
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &5),
        Err(Ok(EscrowError::EscrowCompacted))
    );
    assert_eq!(s.escrow.get_receipt(&id).unwrap().closed_at, 0);
}

#[test]
fn legacy_entry_is_migrated_on_first_mutation() {
    let s = setup();
    write_legacy(&s, &legacy_escrow(&s, 99, &[100 * U]));
    assert!(has_legacy_key(&s, 99));
    assert!(!has_new_key(&s, 99));

    // Reads work on the legacy entry and do not write.
    let read = s.escrow.get_contract(&99u64);
    assert_eq!(read.id, 99);
    assert_eq!(read.status, ContractStatus::Created);
    assert!(has_legacy_key(&s, 99));
    assert!(!has_new_key(&s, 99));

    // The first mutation migrates the entry.
    s.escrow.deposit_funds(&99u64, &(100 * U));
    assert!(has_new_key(&s, 99));
    assert!(!has_legacy_key(&s, 99));
    let after = s.escrow.get_contract(&99u64);
    assert_eq!(after.status, ContractStatus::Funded);
    assert_eq!(after.funded_amount, 100 * U);

    s.escrow.release_milestone(&99u64, &0u32);
    assert_eq!(s.token.balance(&s.freelancer), 100 * U);
    assert_eq!(
        s.escrow.get_contract(&99u64).status,
        ContractStatus::Completed
    );
}

#[test]
fn reads_on_legacy_entry_do_not_mutate_storage() {
    let s = setup();
    write_legacy(&s, &legacy_escrow(&s, 42, &[50 * U]));
    let _ = s.escrow.get_contract(&42u64);
    let _ = s.escrow.get_escrow_summary(&42u64);
    let _ = s.escrow.get_milestone(&42u64, &0);
    let _ = s.escrow.next_releasable(&42u64);
    assert!(has_legacy_key(&s, 42));
    assert!(!has_new_key(&s, 42));
}

#[test]
fn deposit_and_release_work_on_legacy_entry() {
    let s = setup();
    write_legacy(&s, &legacy_escrow(&s, 77, &[200 * U, 300 * U]));
    s.escrow.deposit_funds(&77u64, &(500 * U));
    assert!(has_new_key(&s, 77));
    assert!(!has_legacy_key(&s, 77));
    assert_eq!(s.escrow.get_contract(&77u64).status, ContractStatus::Funded);
    s.escrow.release_milestone(&77u64, &0u32);
    s.escrow.release_milestone(&77u64, &1u32);
    assert_eq!(s.token.balance(&s.freelancer), 500 * U);
    assert_eq!(
        s.escrow.get_contract(&77u64).status,
        ContractStatus::Completed
    );
}

#[test]
fn migrate_escrows_is_idempotent() {
    let s = setup();
    write_legacy(&s, &legacy_escrow(&s, 10, &[10 * U]));
    assert_eq!(s.escrow.migrate_escrows(&vec![&s.env, 10u64]), 1);
    assert!(has_new_key(&s, 10));
    assert!(!has_legacy_key(&s, 10));
    assert_eq!(s.escrow.migrate_escrows(&vec![&s.env, 10u64]), 0);
    let c = s.escrow.get_contract(&10u64);
    assert_eq!(c, legacy_escrow(&s, 10, &[10 * U]));
}

#[test]
fn migrate_escrows_skips_unknown_and_enforces_cap() {
    let s = setup();
    assert_eq!(s.escrow.migrate_escrows(&vec![&s.env, 9999u64]), 0);
    let mut ids = Vec::new(&s.env);
    for i in 1000u64..1050u64 {
        ids.push_back(i);
    }
    assert_eq!(ids.len(), MAX_MIGRATE_BATCH);
    assert_eq!(s.escrow.migrate_escrows(&ids), 0);
    ids.push_back(2000u64);
    assert_eq!(
        s.escrow.try_migrate_escrows(&ids),
        Err(Ok(EscrowError::InvalidAmount))
    );
}

#[test]
fn new_contracts_use_versioned_key() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 10 * U],
    );
    assert!(has_new_key(&s, id));
    assert!(!has_legacy_key(&s, id));
    // Freeze and compaction work against the versioned key too
    s.escrow.freeze_escrow(&id, &symbol_short!("check"));
    s.escrow.unfreeze_escrow(&id);
    s.escrow.cancel_contract(&id);
    s.escrow.compact_escrow(&id);
    assert!(!has_new_key(&s, id));
    assert!(s.escrow.get_receipt(&id).is_some());
}

#[test]
fn party_indices_paginate_in_creation_order() {
    let s = setup();
    let second_freelancer = Address::generate(&s.env);
    let expected: std::vec::Vec<u64> = (1..=120).collect();

    for index in 0..120 {
        let freelancer = if index % 2 == 0 {
            s.freelancer.clone()
        } else {
            second_freelancer.clone()
        };
        assert_eq!(create_indexed_escrow(&s, &freelancer), index as u64 + 1);
    }

    let first = s.escrow.get_client_escrows(&s.client, &0, &50);
    let second = s.escrow.get_client_escrows(&s.client, &50, &50);
    let third = s.escrow.get_client_escrows(&s.client, &100, &50);

    assert_ids(&first, &expected[0..50]);
    assert_ids(&second, &expected[50..100]);
    assert_ids(&third, &expected[100..120]);
    assert_eq!(s.escrow.count_client_escrows(&s.client), 120);

    let freelancer_ids: std::vec::Vec<u64> =
        expected.iter().copied().filter(|id| id % 2 == 1).collect();
    assert_eq!(
        s.escrow.count_freelancer_escrows(&s.freelancer),
        freelancer_ids.len() as u32
    );
    assert_eq!(
        s.escrow.count_freelancer_escrows(&second_freelancer),
        freelancer_ids.len() as u32
    );

    let freelancer_first = s.escrow.get_freelancer_escrows(&s.freelancer, &0, &50);
    let freelancer_second = s.escrow.get_freelancer_escrows(&s.freelancer, &50, &50);
    assert_ids(&freelancer_first, &freelancer_ids[0..50]);
    assert_ids(&freelancer_second, &freelancer_ids[50..60]);
}

#[test]
fn party_index_reads_clamp_and_bound_the_page() {
    let s = setup();
    for _ in 0..120 {
        create_indexed_escrow(&s, &s.freelancer);
    }

    assert_eq!(s.escrow.get_client_escrows(&s.client, &0, &0).len(), 0);
    assert_eq!(s.escrow.get_client_escrows(&s.client, &120, &50).len(), 0);
    assert_eq!(
        s.escrow.get_client_escrows(&s.client, &u32::MAX, &50).len(),
        0
    );

    let clamped = s.escrow.get_client_escrows(&s.client, &0, &500);
    assert_eq!(clamped.len(), 50);
    assert_eq!(clamped.get(0), Some(1));
    assert_eq!(clamped.get(49), Some(50));

    let crossing = s.escrow.get_client_escrows(&s.client, &45, &50);
    assert_eq!(crossing.len(), 50);
    assert_eq!(crossing.get(0), Some(46));
    assert_eq!(crossing.get(49), Some(95));

    let tail = s.escrow.get_client_escrows(&s.client, &115, &500);
    assert_eq!(tail.len(), 5);
    assert_eq!(tail.get(0), Some(116));
    assert_eq!(tail.get(4), Some(120));
}

#[test]
fn party_index_pages_never_exceed_page_size() {
    let s = setup();
    let second_freelancer = Address::generate(&s.env);
    for index in 0..120 {
        let freelancer = if index % 2 == 0 {
            s.freelancer.clone()
        } else {
            second_freelancer.clone()
        };
        create_indexed_escrow(&s, &freelancer);
    }

    let client_count = s.escrow.count_client_escrows(&s.client);
    let client_pages = client_count.div_ceil(ESCROW_INDEX_PAGE_SIZE);
    for page_index in 0..client_pages {
        let page = client_page(&s, &s.client, page_index);
        assert!(
            page.len() <= ESCROW_INDEX_PAGE_SIZE,
            "client page {page_index} holds {} ids",
            page.len()
        );
    }

    for freelancer in [&s.freelancer, &second_freelancer] {
        let count = s.escrow.count_freelancer_escrows(freelancer);
        let pages = count.div_ceil(ESCROW_INDEX_PAGE_SIZE);
        for page_index in 0..pages {
            let page = freelancer_page(&s, freelancer, page_index);
            assert!(
                page.len() <= ESCROW_INDEX_PAGE_SIZE,
                "freelancer page {page_index} holds {} ids",
                page.len()
            );
        }
    }
}
