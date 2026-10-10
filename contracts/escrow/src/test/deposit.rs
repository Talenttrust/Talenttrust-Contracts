use super::{assert_contract_error, EscrowFixture};
use crate::{deposit, ContractStatus, Error};
use soroban_sdk::testutils::Address as _;
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

    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Created
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        0
    );

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &partial));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::PartiallyFunded
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        partial
    );

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &(total - partial)));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        total
    );
}

/// Invalid deposit amounts fail before touching the configured SAC balance.
#[test]
fn deposit_rejects_non_positive_amounts() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &fixture.total_amount());

    for amount in [0_i128, -1_i128, i128::MIN] {
        assert_contract_error(
            escrow.try_deposit_funds(&fixture.escrow_id, &fixture.client, &amount),
            Error::AmountMustBePositive,
        );
    }

    // Rejected deposits must not move tokens or advance state.
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Created
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        0
    );
}

/// Exactly the remaining amount is the boundary accepted deposit.
#[test]
fn deposit_accepts_exact_remaining_amount() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &total));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        total
    );
}

/// A duplicate deposit after funding is rejected and leaves the balance intact.
#[test]
fn deposit_rejects_duplicate_after_funding() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();

    assert_contract_error(
        escrow.try_deposit_funds(&fixture.escrow_id, &fixture.client, &1),
        Error::InvalidMilestoneAmount,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        total
    );
}

/// Deposits from an unauthorized address are rejected without moving funds.
#[test]
fn deposit_rejects_unauthorized_depositor() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert_contract_error(
        escrow.try_deposit_funds(&fixture.escrow_id, &soroban_sdk::Address::generate(&fixture.env), &1),
        Error::UnauthorizedRole,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Created
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        0
    );
}

/// Deposits against a non-existent contract are rejected.
#[test]
fn deposit_rejects_unknown_contract() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert_contract_error(
        escrow.try_deposit_funds(&(fixture.escrow_id + 1), &fixture.client, &1),
        Error::ContractNotFound,
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        0
    );
}

/// A stale validated deposit must be rejected if another deposit advanced the
/// contract state first. This guards the preflight+apply pattern against replay
/// and racing writes that would otherwise silently regress the escrow balance.
#[test]
fn stale_validated_deposit_rejected_after_state_change() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let stale_amount = total / 4;
    let fresh_amount = total / 2;
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    let stale_validated = deposit::validate_deposit(
        &fixture.env,
        fixture.escrow_id,
        &fixture.client,
        stale_amount,
    );

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &fresh_amount));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).funded_amount,
        fresh_amount
    );

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        deposit::apply_validated_deposit(
            &fixture.env,
            fixture.escrow_id,
            fixture.client.clone(),
            stale_validated,
        );
    }));

    assert!(result.is_err(), "stale validated deposits must be rejected");
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).funded_amount,
        fresh_amount,
        "stale write must not regress escrow funding state"
    );
}

/// Deposits are rejected once the contract is already fully funded.
#[test]
fn deposit_rejects_when_already_funded() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert_contract_error(
        escrow.try_deposit_funds(&fixture.escrow_id, &fixture.client, &total),
        Error::InvalidMilestoneAmount,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        total
    );
}

/// Exactly filling the remaining balance transitions to Funded and preserves custody.
#[test]
fn deposit_accepts_exact_remaining_boundary() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &total));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        total
    );
}

/// Repeated deposits from the same client accumulate deterministically without losing funds.
#[test]
fn deposits_are_additive_and_deterministic() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    let step = total / 4;
    for _ in 0..3 {
        assert!(escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &step));
    }
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::PartiallyFunded
    );
    assert!(escrow.deposit_funds(
        &fixture.escrow_id,
        &fixture.client,
        &(total - step * 3)
    ));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        total
    );
}

/// Deposits against an unknown escrow id fail without moving any tokens.
#[test]
fn deposit_rejects_unknown_escrow() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    let unknown_id = fixture.escrow_id + 1;
    assert_contract_error(
        escrow.try_deposit_funds(&unknown_id, &fixture.client, &total),
        Error::ContractNotFound,
    );
    assert_eq!(
        TokenClient::new(&fixture.env, token).balance(&fixture.escrow_address),
        0
    );
}
