use soroban_sdk::{vec, Vec};

use super::{assert_contract_error, EscrowFixture, MILESTONE_ONE, MILESTONE_THREE, MILESTONE_TWO};
use crate::{ContractStatus, Error, EscrowError};

/// Releases use the funded shortcut and complete after every milestone settles.
#[test]
fn release_funded_milestones_completes_contract() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    for index in 0..3_u32 {
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &index));
        assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &index));
    }

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, fixture.total_amount());
}

/// A release cannot be repeated after the fixture has settled that milestone.
#[test]
fn release_rejects_an_already_released_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));

    assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
    assert_contract_error(
        escrow.try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        EscrowError::AlreadyReleased,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).released_amount,
        MILESTONE_ONE
    );
}

// ── Batch release_milestones tests ───────────────────────────────────────────

/// Batch release of all milestones completes the contract and pays the full amount.
#[test]
fn batch_release_all_milestones_completes_contract() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Approve all milestones individually
    for index in 0..3_u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &index);
    }

    let indices = vec![&fixture.env, 0_u32, 1_u32, 2_u32];
    assert!(escrow.release_milestones(&fixture.escrow_id, &fixture.client, &indices));

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, fixture.total_amount());
}

/// Batch release of a subset leaves the contract in Funded state.
#[test]
fn batch_release_subset_does_not_complete() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &1);

    let indices = vec![&fixture.env, 0_u32, 1_u32];
    assert!(escrow.release_milestones(&fixture.escrow_id, &fixture.client, &indices));

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, ContractStatus::Funded);
    assert_eq!(contract.released_amount, MILESTONE_ONE + MILESTONE_TWO);
}

/// Empty indices vector panics with EmptyReleaseIndices.
#[test]
fn batch_release_empty_indices_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let indices: Vec<u32> = Vec::new(&fixture.env);
    assert_contract_error(
        escrow.try_release_milestones(&fixture.escrow_id, &fixture.client, &indices),
        EscrowError::EmptyReleaseIndices,
    );
}

/// Duplicate indices in the batch are rejected.
#[test]
fn batch_release_duplicate_indices_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    let indices = vec![&fixture.env, 0_u32, 0_u32];
    assert_contract_error(
        escrow.try_release_milestones(&fixture.escrow_id, &fixture.client, &indices),
        EscrowError::DuplicateMilestoneInRelease,
    );
}

/// Out-of-bounds index panics with IndexOutOfBounds.
#[test]
fn batch_release_out_of_bounds_index_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &1);

    let indices = vec![&fixture.env, 0_u32, 99_u32];
    assert_contract_error(
        escrow.try_release_milestones(&fixture.escrow_id, &fixture.client, &indices),
        Error::IndexOutOfBounds,
    );
}

/// Already-released milestone in the batch fails the entire operation.
#[test]
fn batch_release_rejects_already_released_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Release milestone 0 first
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0);

    // Try batch with milestone 0 (already released) and milestone 1
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &1);
    let indices = vec![&fixture.env, 0_u32, 1_u32];
    assert_contract_error(
        escrow.try_release_milestones(&fixture.escrow_id, &fixture.client, &indices),
        Error::MilestoneAlreadyReleased,
    );

    // State is unchanged: only milestone 0 was released
    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.released_amount, MILESTONE_ONE);
}

/// Insufficient balance for the aggregate batch panics with InsufficientFunds.
#[test]
fn batch_release_insufficient_balance_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Approve all three
    for index in 0..3_u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &index);
    }

    // Release first milestone individually to reduce available balance
    escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0);

    // Now try batch-releasing the remaining two
    let indices = vec![&fixture.env, 1_u32, 2_u32];
    assert!(escrow.release_milestones(&fixture.escrow_id, &fixture.client, &indices));

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(
        contract.released_amount,
        MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
    );
}

/// Exact balance batch succeeds and completes the contract.
#[test]
fn batch_release_exact_balance_succeeds() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &1);
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &2);

    let indices = vec![&fixture.env, 0_u32, 1_u32, 2_u32];
    assert!(escrow.release_milestones(&fixture.escrow_id, &fixture.client, &indices));

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(
        contract.released_amount,
        MILESTONE_ONE + MILESTONE_TWO + MILESTONE_THREE
    );
}

/// Unauthorized caller is rejected.
#[test]
fn batch_release_unauthorized_caller_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let indices = vec![&fixture.env, 0_u32];
    assert_contract_error(
        escrow.try_release_milestones(&fixture.escrow_id, &fixture.freelancer, &indices),
        EscrowError::UnauthorizedRole,
    );
}

/// Batch release from Created state (not Funded) is rejected.
#[test]
fn batch_release_invalid_state_rejected() {
    let fixture = EscrowFixture::builder().build();
    let escrow = fixture.escrow();

    let indices = vec![&fixture.env, 0_u32];
    assert_contract_error(
        escrow.try_release_milestones(&fixture.escrow_id, &fixture.client, &indices),
        Error::InvalidState,
    );
}

/// Missing approvals cause the batch to fail (all-or-nothing).
#[test]
fn batch_release_missing_approvals_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Only approve milestone 0, not milestone 1
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    let indices = vec![&fixture.env, 0_u32, 1_u32];
    assert_contract_error(
        escrow.try_release_milestones(&fixture.escrow_id, &fixture.client, &indices),
        Error::InsufficientApprovals,
    );

    // State unchanged
    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.released_amount, 0);
}

/// Protocol fees are correctly accumulated in a batch release.
#[test]
fn batch_release_accumulates_protocol_fees() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Set a 10% protocol fee
    escrow.set_protocol_fee_bps(&1000);

    // Approve all three milestones
    for index in 0..3_u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &index);
    }

    let total = fixture.total_amount();
    let expected_fee = total * 1000 / 10_000;
    let expected_net = total - expected_fee;

    let indices = vec![&fixture.env, 0_u32, 1_u32, 2_u32];
    assert!(escrow.release_milestones(&fixture.escrow_id, &fixture.client, &indices));

    assert_eq!(escrow.get_accumulated_protocol_fees(), expected_fee);

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.released_amount, expected_net);
    assert_eq!(contract.status, ContractStatus::Completed);
}

/// Single-index batch behaves identically to release_milestone.
#[test]
fn batch_release_single_index_works() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    let indices = vec![&fixture.env, 0_u32];
    assert!(escrow.release_milestones(&fixture.escrow_id, &fixture.client, &indices));

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.released_amount, MILESTONE_ONE);
    assert_eq!(contract.status, ContractStatus::Funded);
}

/// Milestones are actually marked released in storage after batch.
#[test]
fn batch_release_marks_milestones_released() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    for index in 0..2_u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &index);
    }

    let indices = vec![&fixture.env, 0_u32, 1_u32];
    assert!(escrow.release_milestones(&fixture.escrow_id, &fixture.client, &indices));

    let milestones = escrow.get_milestones(&fixture.escrow_id);
    assert!(milestones.get(0).unwrap().released);
    assert!(milestones.get(1).unwrap().released);
    assert!(!milestones.get(2).unwrap().released);
}

/// Approvals are cleared after a successful batch release.
#[test]
fn batch_release_clears_approvals() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &1);

    // Verify approvals exist before release
    assert!(escrow
        .get_milestone_approvals(&fixture.escrow_id, &0)
        .is_some());
    assert!(escrow
        .get_milestone_approvals(&fixture.escrow_id, &1)
        .is_some());

    let indices = vec![&fixture.env, 0_u32, 1_u32];
    assert!(escrow.release_milestones(&fixture.escrow_id, &fixture.client, &indices));

    // Approvals should be cleared
    assert!(escrow
        .get_milestone_approvals(&fixture.escrow_id, &0)
        .is_none());
    assert!(escrow
        .get_milestone_approvals(&fixture.escrow_id, &1)
        .is_none());
}

/// Empty batch panics with EmptyReleaseIndices (EscrowError).
#[test]
fn batch_release_empty_indices_uses_escrow_error() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let indices: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&fixture.env);
    assert_contract_error(
        escrow.try_release_milestones(&fixture.escrow_id, &fixture.client, &indices),
        EscrowError::EmptyReleaseIndices,
    );
}

/// Duplicate milestones in batch are rejected before any state change.
#[test]
fn batch_release_duplicate_rejected_no_state_change() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    let indices = vec![&fixture.env, 0_u32, 0_u32];
    assert_contract_error(
        escrow.try_release_milestones(&fixture.escrow_id, &fixture.client, &indices),
        EscrowError::DuplicateMilestoneInRelease,
    );

    // State unchanged
    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.released_amount, 0);
    assert_eq!(contract.status, ContractStatus::Funded);
}
