use super::{assert_contract_error, EscrowFixture, MILESTONE_ONE};
use crate::{ContractStatus, EscrowError};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};

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

/// A failed SAC payout leaves the milestone and accounting retryable.
#[test]
fn failed_release_preserves_state_and_can_be_retried() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let token_address = fixture
        .settlement_token
        .as_ref()
        .expect("funded fixture binds its settlement token");
    let token = TokenClient::new(&fixture.env, token_address);
    let token_admin = StellarAssetClient::new(&fixture.env, token_address);

    assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
    token.burn(&fixture.escrow_address, &MILESTONE_ONE);

    assert!(escrow
        .try_release_milestone(&fixture.escrow_id, &fixture.client, &0)
        .is_err());
    assert_eq!(escrow.get_contract(&fixture.escrow_id).released_amount, 0);
    assert!(
        !escrow
            .get_milestone(&fixture.escrow_id, &0)
            .expect("milestone exists")
            .released
    );
    assert_eq!(token.balance(&fixture.freelancer), 0);

    token_admin.mint(&fixture.escrow_address, &MILESTONE_ONE);
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));

    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).released_amount,
        MILESTONE_ONE
    );
    assert!(
        escrow
            .get_milestone(&fixture.escrow_id, &0)
            .expect("milestone exists")
            .released
    );
    assert_eq!(token.balance(&fixture.freelancer), MILESTONE_ONE);
}
