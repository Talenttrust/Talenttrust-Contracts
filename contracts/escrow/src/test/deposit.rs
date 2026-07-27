use super::{assert_contract_error, EscrowFixture};
use crate::{ContractStatus, Error};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};

/// A fully-funded fixture records the complete milestone total and custody balance.
#[test]
fn funded_fixture_deposits_the_configured_total() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let token = fixture.settlement_token.as_ref().unwrap();

    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        fixture.total_amount()
    );
}

/// Deposits can be staged while the fixture keeps token custody setup uniform.
#[test]
fn deposit_transitions_from_partially_funded_to_funded() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let partial = total / 2;
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &partial));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::PartiallyFunded
    );
    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &(total - partial)));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
}

/// Invalid deposit amounts fail before touching the configured SAC balance.
#[test]
fn deposit_rejects_non_positive_amounts() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    for amount in [0_i128, -1_i128] {
        assert_contract_error(
            escrow.try_deposit_funds(&fixture.escrow_id, &fixture.client, &amount),
            Error::AmountMustBePositive,
        );
    }
}
