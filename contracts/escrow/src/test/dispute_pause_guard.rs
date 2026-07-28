#![cfg(test)]

//! Confirms disputes' existing pause guard: `raise_dispute` and
//! `resolve_dispute` already call `Self::require_not_paused`, which rejects
//! while `Paused` or `Emergency` is set and allows otherwise. This adds the
//! regression coverage that was missing for that behaviour.

use soroban_sdk::{testutils::Address as _, Address};

use soroban_sdk::token::StellarAssetClient;

use crate::test::EscrowFixture;
use crate::{DisputeResolution, Error};

#[test]
fn raise_dispute_rejected_while_paused() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let arbiter = Address::generate(&fixture.env);
    // Re-create with an arbiter: fund flow already done, so raise a fresh
    // arbitered contract instead of retrofitting one.
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

    escrow.pause();

    let result = escrow.try_raise_dispute(&contract_id, &fixture.client);
    crate::test::assert_contract_error(result, Error::ContractPaused);
}

#[test]
fn raise_dispute_allowed_when_unpaused() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let arbiter = Address::generate(&fixture.env);
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

    // Never paused: should succeed.
    let result = escrow.raise_dispute(&contract_id, &fixture.client);
    assert!(result);
}

#[test]
fn resolve_dispute_rejected_while_paused() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let arbiter = Address::generate(&fixture.env);
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
    escrow.raise_dispute(&contract_id, &fixture.client);

    escrow.pause();

    let result = escrow.try_resolve_dispute(&contract_id, &arbiter, &DisputeResolution::FullRefund);
    crate::test::assert_contract_error(result, Error::ContractPaused);
}

#[test]
fn resolve_dispute_allowed_when_unpaused() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let arbiter = Address::generate(&fixture.env);
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
    escrow.raise_dispute(&contract_id, &fixture.client);

    // Never paused: should succeed.
    let result = escrow.resolve_dispute(&contract_id, &arbiter, &DisputeResolution::FullRefund);
    assert!(result);
}
