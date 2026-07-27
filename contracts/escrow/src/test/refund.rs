use soroban_sdk::vec;

use super::{assert_contract_error, EscrowFixture, MILESTONE_TWO};
use crate::{ContractStatus, Error};

/// Refunds are available immediately from a fixture funded through real SAC custody.
#[test]
fn refund_returns_an_unreleased_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 1_u32];

    assert_eq!(
        escrow.refund_unreleased_milestones(&fixture.escrow_id, &ids),
        MILESTONE_TWO
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
}

/// A completed fixture rejects refunds, preserving its terminal accounting state.
#[test]
fn refund_rejects_completed_contract() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    for index in 0..3_u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &index);
        escrow.release_milestone(&fixture.escrow_id, &fixture.client, &index);
    }
    let ids = vec![&fixture.env, 0_u32];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        Error::InvalidState,
    );
}
