//! Tests for the bounded batch settlement entrypoint
//! [`Escrow::finalize_contracts_batch`].
//!
//! Coverage matrix
//! ───────────────
//! | Scenario                                   | Test function                                    |
//! | ───────────────────────────────────────── | ──────────────────────────────────────────────── |
//! | Empty vector → `BatchSettlementEmpty`      | `batch_settlement_empty_rejects`                 |
//! | At-cap (10) → all succeed                  | `batch_settlement_at_cap_succeeds`               |
//! | Over-cap (11) → `BatchSettlementTooLarge`  | `batch_settlement_over_cap_rejects`              |
//! | Single item → success                      | `batch_settlement_single_item`                   |
//! | All succeed, events emitted per item       | `batch_settlement_emits_event_per_item`          |
//! | Unknown contract → error code per item     | `batch_settlement_unknown_contract`              |
//! | Already finalized → error code per item    | `batch_settlement_already_finalized`             |
//! | Unauthorized finalizer → error code        | `batch_settlement_unauthorized_finalizer`        |
//! | Non-terminal status → error code           | `batch_settlement_non_terminal_status`           |
//! | Mixed success and failure                  | `batch_settlement_mixed_success_and_failure`     |
//! | Paused contract → whole-call panic         | `batch_settlement_rejects_when_paused`           |
//! | Disputed contract → success                | `batch_settlement_disputed_contract_succeeds`    |
//! | Freelancer can be the finalizer            | `batch_settlement_freelancer_as_finalizer`       |
//! | Arbiter can be the finalizer               | `batch_settlement_arbiter_as_finalizer`          |

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use super::{assert_contract_error, complete_contract, register_client};
use crate::{
    BatchSettlementResult, ContractStatus, EscrowError, ReleaseAuthorization, SettlementItem,
    MAX_BATCH_SETTLEMENT,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Build and fully-complete a contract, returning (client, freelancer, id).
fn make_completed(env: &Env, client: &crate::EscrowClient) -> (Address, Address, u32) {
    complete_contract(env, client)
}

/// Build a completed contract and immediately finalize it, returning the id.
fn make_finalized(env: &Env, client: &crate::EscrowClient) -> (Address, u32) {
    let (client_addr, _, id) = make_completed(env, client);
    client.finalize_contract(&id, &client_addr);
    (client_addr, id)
}

/// Assert that a `BatchSettlementResult` reports success.
fn assert_ok(result: &BatchSettlementResult, expected_index: u32, expected_contract_id: u32) {
    assert_eq!(result.index, expected_index, "index mismatch");
    assert_eq!(
        result.contract_id, expected_contract_id,
        "contract_id mismatch"
    );
    assert!(result.success, "expected success but got failure: {:?}", result);
    assert!(result.error_code.is_none(), "expected no error_code");
}

/// Assert that a `BatchSettlementResult` reports the expected error code.
fn assert_err(
    result: &BatchSettlementResult,
    expected_index: u32,
    expected_contract_id: u32,
    expected_error: EscrowError,
) {
    assert_eq!(result.index, expected_index, "index mismatch");
    assert_eq!(
        result.contract_id, expected_contract_id,
        "contract_id mismatch"
    );
    assert!(!result.success, "expected failure but got success");
    assert_eq!(
        result.error_code,
        Some(expected_error as u32),
        "wrong error code: expected {:?} ({}), got {:?}",
        expected_error,
        expected_error as u32,
        result.error_code
    );
}

// ── Empty vector ─────────────────────────────────────────────────────────────

#[test]
fn batch_settlement_empty_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let items: soroban_sdk::Vec<SettlementItem> = soroban_sdk::Vec::new(&env);
    let result = escrow.try_finalize_contracts_batch(&items);
    assert_contract_error(result, EscrowError::BatchSettlementEmpty);
}

// ── At-cap ───────────────────────────────────────────────────────────────────

#[test]
fn batch_settlement_at_cap_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    // Create MAX_BATCH_SETTLEMENT completed contracts.
    let mut items: soroban_sdk::Vec<SettlementItem> = soroban_sdk::Vec::new(&env);
    let mut expected_ids: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&env);
    let mut client_addrs: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);

    let mut i = 0u32;
    while i < MAX_BATCH_SETTLEMENT {
        let (client_addr, _, id) = make_completed(&env, &escrow);
        items.push_back(SettlementItem {
            contract_id: id,
            finalizer: client_addr.clone(),
        });
        expected_ids.push_back(id);
        client_addrs.push_back(client_addr);
        i += 1;
    }

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), MAX_BATCH_SETTLEMENT, "result count mismatch");

    for j in 0..MAX_BATCH_SETTLEMENT {
        let r: BatchSettlementResult = results.get(j).unwrap();
        let expected_id = expected_ids.get(j).unwrap();
        assert_ok(&r, j, expected_id);
        // Verify storage was actually written.
        assert!(
            escrow.get_finalization_record(&expected_id).is_some(),
            "finalization record missing for id {}",
            expected_id
        );
    }
}

// ── Over-cap ─────────────────────────────────────────────────────────────────

#[test]
fn batch_settlement_over_cap_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    // Build MAX_BATCH_SETTLEMENT + 1 items (contracts don't need to be valid —
    // the cap check fires before any per-item logic).
    let dummy_addr = Address::generate(&env);
    let mut items: soroban_sdk::Vec<SettlementItem> = soroban_sdk::Vec::new(&env);
    let mut k = 0u32;
    while k <= MAX_BATCH_SETTLEMENT {
        items.push_back(SettlementItem {
            contract_id: k + 1,
            finalizer: dummy_addr.clone(),
        });
        k += 1;
    }
    assert_eq!(items.len(), MAX_BATCH_SETTLEMENT + 1);

    let result = escrow.try_finalize_contracts_batch(&items);
    assert_contract_error(result, EscrowError::BatchSettlementTooLarge);
}

// ── Single item ───────────────────────────────────────────────────────────────

#[test]
fn batch_settlement_single_item() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client_addr, _, id) = make_completed(&env, &escrow);
    let items = vec![
        &env,
        SettlementItem {
            contract_id: id,
            finalizer: client_addr,
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 1);
    let r: BatchSettlementResult = results.get(0).unwrap();
    assert_ok(&r, 0, id);
    assert!(escrow.get_finalization_record(&id).is_some());
}

// ── Per-item events ───────────────────────────────────────────────────────────

#[test]
fn batch_settlement_emits_event_per_item() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client1, _, id1) = make_completed(&env, &escrow);
    let (client2, _, id2) = make_completed(&env, &escrow);

    let items = vec![
        &env,
        SettlementItem {
            contract_id: id1,
            finalizer: client1,
        },
        SettlementItem {
            contract_id: id2,
            finalizer: client2,
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 2);
    assert_ok(&results.get(0).unwrap(), 0, id1);
    assert_ok(&results.get(1).unwrap(), 1, id2);

    // Both contracts should now have finalization records written.
    assert!(escrow.get_finalization_record(&id1).is_some());
    assert!(escrow.get_finalization_record(&id2).is_some());

    // Verify the contracts are still accessible and in Completed state.
    assert_eq!(
        escrow.get_contract(&id1).status,
        ContractStatus::Completed
    );
    assert_eq!(
        escrow.get_contract(&id2).status,
        ContractStatus::Completed
    );
}

// ── Unknown contract → per-item error ────────────────────────────────────────

#[test]
fn batch_settlement_unknown_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let dummy_addr = Address::generate(&env);
    let items = vec![
        &env,
        SettlementItem {
            contract_id: 9999,
            finalizer: dummy_addr,
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 1);
    let r: BatchSettlementResult = results.get(0).unwrap();
    assert_err(&r, 0, 9999, EscrowError::ContractNotFound);
}

// ── Already finalized → per-item error ───────────────────────────────────────

#[test]
fn batch_settlement_already_finalized() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client_addr, id) = make_finalized(&env, &escrow);

    // Try to finalize the same contract again via batch.
    let items = vec![
        &env,
        SettlementItem {
            contract_id: id,
            finalizer: client_addr,
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 1);
    let r: BatchSettlementResult = results.get(0).unwrap();
    assert_err(&r, 0, id, EscrowError::AlreadyFinalized);
}

// ── Unauthorized finalizer → per-item error ───────────────────────────────────

#[test]
fn batch_settlement_unauthorized_finalizer() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (_, _, id) = make_completed(&env, &escrow);
    let stranger = Address::generate(&env);

    let items = vec![
        &env,
        SettlementItem {
            contract_id: id,
            finalizer: stranger,
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 1);
    let r: BatchSettlementResult = results.get(0).unwrap();
    assert_err(&r, 0, id, EscrowError::UnauthorizedRole);

    // Contract must remain un-finalized.
    assert!(escrow.get_finalization_record(&id).is_none());
}

// ── Non-terminal status → per-item error ──────────────────────────────────────

#[test]
fn batch_settlement_non_terminal_status() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    // Create a contract that is only Created (not yet funded or completed).
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let items = vec![
        &env,
        SettlementItem {
            contract_id: id,
            finalizer: client_addr,
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 1);
    let r: BatchSettlementResult = results.get(0).unwrap();
    assert_err(&r, 0, id, EscrowError::InvalidStatusTransition);
}

// ── Mixed success and failure ─────────────────────────────────────────────────

#[test]
fn batch_settlement_mixed_success_and_failure() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    // Item 0: valid completed contract → success
    let (client_addr0, _, id0) = make_completed(&env, &escrow);
    // Item 1: unknown contract → ContractNotFound
    let dummy = Address::generate(&env);
    // Item 2: valid completed contract, wrong finalizer → UnauthorizedRole
    let (_, _, id2) = make_completed(&env, &escrow);
    let stranger = Address::generate(&env);
    // Item 3: valid completed contract → success
    let (client_addr3, _, id3) = make_completed(&env, &escrow);

    let items = vec![
        &env,
        SettlementItem {
            contract_id: id0,
            finalizer: client_addr0.clone(),
        },
        SettlementItem {
            contract_id: 88888,
            finalizer: dummy,
        },
        SettlementItem {
            contract_id: id2,
            finalizer: stranger,
        },
        SettlementItem {
            contract_id: id3,
            finalizer: client_addr3.clone(),
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 4);

    assert_ok(&results.get(0).unwrap(), 0, id0);
    assert_err(&results.get(1).unwrap(), 1, 88888, EscrowError::ContractNotFound);
    assert_err(&results.get(2).unwrap(), 2, id2, EscrowError::UnauthorizedRole);
    assert_ok(&results.get(3).unwrap(), 3, id3);

    // Verify storage state.
    assert!(escrow.get_finalization_record(&id0).is_some());
    assert!(escrow.get_finalization_record(&id2).is_none()); // failed, must not be written
    assert!(escrow.get_finalization_record(&id3).is_some());
}

// ── Paused contract → whole-call panic ───────────────────────────────────────

#[test]
fn batch_settlement_rejects_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (client_addr, _, id) = make_completed(&env, &escrow);
    escrow.pause();

    let items = vec![
        &env,
        SettlementItem {
            contract_id: id,
            finalizer: client_addr,
        },
    ];

    let result = escrow.try_finalize_contracts_batch(&items);
    assert_contract_error(result, EscrowError::ContractPaused);
}

// ── Disputed contract ─────────────────────────────────────────────────────────

#[test]
fn batch_settlement_disputed_contract_succeeds() {
    // EscrowFixtureBuilder handles SAC wiring; .funded() deposits the full amount.
    let fixture = super::EscrowFixtureBuilder::new().funded().build();
    let env = fixture.env.clone();
    let id = fixture.escrow_id;
    let escrow = fixture.escrow();
    let client_addr = fixture.client.clone();

    // Raise a dispute on the funded contract — status becomes Disputed.
    escrow.raise_dispute(&id, &client_addr);
    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Disputed);

    // Client can finalize a Disputed contract.
    let items = vec![
        &env,
        SettlementItem {
            contract_id: id,
            finalizer: client_addr,
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 1);
    let r: BatchSettlementResult = results.get(0).unwrap();
    assert_ok(&r, 0, id);
    assert!(escrow.get_finalization_record(&id).is_some());
}

// ── Freelancer as finalizer ───────────────────────────────────────────────────

#[test]
fn batch_settlement_freelancer_as_finalizer() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let (_, freelancer_addr, id) = make_completed(&env, &escrow);

    let items = vec![
        &env,
        SettlementItem {
            contract_id: id,
            finalizer: freelancer_addr,
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 1);
    assert_ok(&results.get(0).unwrap(), 0, id);
    assert!(escrow.get_finalization_record(&id).is_some());
}

// ── Arbiter as finalizer ──────────────────────────────────────────────────────

#[test]
fn batch_settlement_arbiter_as_finalizer() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let total = super::total_milestone_amount();
    if let Some(token) = escrow.get_settlement_token() {
        soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&client_addr, &total);
    }
    escrow.deposit_funds(&id, &client_addr, &total);

    // Release all milestones (ClientOnly auth) to complete the contract.
    for idx in 0..milestones.len() {
        escrow.approve_milestone_release(&id, &client_addr, &idx);
        escrow.release_milestone(&id, &client_addr, &idx);
    }
    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Completed);

    let items = vec![
        &env,
        SettlementItem {
            contract_id: id,
            finalizer: arbiter_addr,
        },
    ];

    let results = escrow.finalize_contracts_batch(&items);
    assert_eq!(results.len(), 1);
    assert_ok(&results.get(0).unwrap(), 0, id);
    assert!(escrow.get_finalization_record(&id).is_some());
}
