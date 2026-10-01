use super::{assert_contract_error, EscrowFixture, MILESTONE_ONE};
use crate::{ContractStatus, EscrowError};

/// Releases use the funded shortcut and complete after every milestone settles.
///
/// Invariants:
/// - A funded escrow must be releasable without an external funding step.
/// - Every milestone must be approved before release, and releasing all
///   milestones must transition the contract to `Completed` with the full
///   amount recorded as released.
#[test]
fn release_funded_milestones_completes_contract() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    for index in 0..3_u32 {
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, 'index));
        assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &index));
    }

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq (contract.status, ContractStatus::Completed);
    assert_eq(contract.released_amount, fixture.total_amount());
}

/// A release cannot be repeated after the fixture has settled that milestone.
///
/// Invariants:
/// - Replaying a release for an already-settled milestone must fail with
///   `MilestoneAlreadyReleased` and must not change the released amount.
/// - The failure must be deterministic even when the milestone is re-approved
///   between attempts.
#[test]
fn release_rejects_an_already_released_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
    assert (escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));

    assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
    assert_contract_error(
        escrow.try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        EscrowError::MilestoneAlreadyReleased,
    );
    assert_eq(
        escrow.get_contract(&fixture.escrow_id).released_amount,
        MILESTONE_ONE
    );
}
