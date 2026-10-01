#![cfg(test)]

/// Confirms disputes' existing pause guard: `raise_dispute` and
/// `resolve_dispute` already call `Self::require_not_paused`, which rejects
/// while `Paused` or `Emergency` is set and allows otherwise. This adds the
/// regression coverage that was missing for that behaviour.

use soroban_sdk::{testutils::Address as_, Address};

use soroban_sdk::token::StellarAssetClient;

use crate::test::EscrowFixture;
use crate::{DisputeResolution, Error};

/// Build a fresh arbitered escrow that is funded and ready for dispute flows.
/// This avoids retrofitting an arbiter onto an already-funded contract and
/// keeps each test self-contained and deterministic.
fn funded_arbitered_escrow(
    fixture: &EscrowFixture,
    arbiter: &Address,
) -> u32 {
    let escrow = fixture.escrow();
    let contract_id = escrow.create_contract(
        &fixture.client,
        &fixture.freelancer,
        &Some(arbiter.clone()),
        &crate::test::default_milestones(&fixture.env),
        &crate::ReleaseAuthorization::ClientOnly,
    );
    let total = crate::test::total_milestone_amount();
    StellarAssetClient::new(&fixture.env, fixture.settlement_token.as_ref().unwrap())
        .mint(&fixture.client, &total);
    escrow.deposit_funds(&contract_id, &fixture.client, &total);
    contract_id
}

#[test]
fn raise_dispute_rejected_while_paused() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    escrow.pause();

    let result = escrow.try_raise_dispute(&contract_id, &fixture.client);
    crate::test::assert_contract_error(result, Error::ContractPaused);
}

#[test]
fn raise_dispute_allowed_when_unpaused() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    // Never paused: should succeed.
    let result = escrow.raise_dispute(&contract_id, &fixture.client);
    assert!(result);
}

#[cfg(test)]
fn raise_dispute_rejected_while_emergency() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    escrow.set_emergency();

    let result = escrow.try_raise_dispute(&contract_id, &fixture.client);
    crate::test::assert_contract_error(result, Error::ContractPaused);
}

#[test]
fn raise_dispute_rejected_for_non_client() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    // Authorization invariant must hold regardless of pause state.
    let result = escrow.try_raise_dispute(&contract_id, &fixture.freelancer);
    crate::test::assert_contract_error(result, Error::Unauthorized);
}

#[test]
fn raise_dispute_duplicate_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    assert!(escrow.raise_dispute(&contract_id, &fixture.client));
    let result = escrow.try_raise_dispute(&contract_id, &fixture.client);
    crate::test::assert_contract_error(result, Error::DisputeAlreadyRaised);
}

#[test]
fn resolve_dispute_rejected_while_paused() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    assert!(escrow.raise_dispute(&contract_id, &fixture.client));
    escrow.pause();

    let result = escrow.try_resolve_dispute(&contract_id, &arbiter, &DisputeResolution::FullRefund);
    crate::test::assert_contract_error(result, Error::ContractPaused);
}

#[cfg(test)]
fn resolve_dispute_rejected_while_emergency() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    assert!(escrow.raise_dispute(&contract_id, &fixture.client));
    escrow.set_emergency();

    let result = escrow.try_resolve_dispute(&contract_id, &arbiter, &DisputeResolution::FullRefund);
    crate::test::assert_contract_error(result, Error::ContractPaused);
}

#[cfg(test)]
fn resolve_dispute_rejected_for_non_arbiter() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    assert!(escrow.raise_dispute(&contract_id, &fixture.client));

    // Authorization invariant must hold regardless of pause state.
    let result = escrow.try_resolve_dispute(
        &contract_id,
        &fixture.freelancer,
        &DisputeResolution::FullRefund,
    );
    crate::test::assert_contract_error(result, Error::Unauthorized);
}

#[test]
fn resolve_dispute_allowed_when_unpaused() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    assert!(escrow.raise_dispute(&contract_id, &fixture.client));

    // Never paused: should succeed.
    let result = escrow.resolve_dispute(&contract_id, &arbiter, &DisputeResolution::FullRefund);
    assert!(result);
}

#[cfg(test)]
fn resolve_dispute_rejected_without_dispute() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    // No dispute raised: resolution must fail deterministically.
    let result = escrow.try_resolve_dispute(&contract_id, &arbiter, &DisputeResolution::FullRefund);
    crate::test::assert_contract_error(result, Error::DisputeNotRaised);
}

#[cfg(test)]
fn resolve_dispute_duplicate_rejected() {
    let fixture = EscrowFixture::builder().funded().build();
    let arbiter = Address::generate(&fixture.env);
    let contract_id = funded_arbitered_escrow(&fixture, &arbiter);
    let escrow = fixture.escrow();

    assert!(escrow.raise_dispute(&contract_id, &fixture.client));
    assert!(escrow.resolve_dispute(&contract_id, &arbiter, &DisputeResolution::FullRefund));

    // Resolution is terminal; a second attempt must fail deterministically.
    let result = escrow.try_resolve_dispute(&contract_id, &arbiter, &DisputeResolution::FullRefund);
    crate::test::assert_contract_error(result, Error::DisputeNotRaised);
}
