//! Focused invariant tests for `contracts/escrow/src/rollback.rs`.
//!
//! `rollback_dispute` reverses an unchanged, unresolved dispute. These tests
//! pin the state-transition, authorization, and data-integrity guarantees that
//! make the reversal safe. Each invariant `R1`–`R7` from the module docs maps to
//! a named test below.
//!
//! | Invariant | Test |
//! | --- | --- |
//! | R1 rollback only from an allowed pre-state | [`rollback_is_rejected_unless_contract_is_disputed`] |
//! | R2 exact prior state restored (no partial restore) | [`rollback_restores_exact_pre_dispute_state`] |
//! | R3 no double-apply | [`rollback_cannot_double_apply`] |
//! | R4 unauthorized caller rejected | [`rollback_requires_admin_authorization`] |
//! | R5 terminal/irreversible state not rollbackable | [`finalized_contract_cannot_be_rolled_back`] |
//! | R6 related approvals voided | [`rollback_clears_related_approvals`] |
//! | R7 diverged snapshot rejected | [`rollback_rejects_diverged_contract_snapshot`], [`rollback_rejects_diverged_milestone_snapshot`] |

use super::{assert_contract_error, EscrowFixture};
use crate::{Contract, ContractStatus, DataKey, Error, Milestone, MilestoneApprovals};
use soroban_sdk::Vec;

/// A funded `ClientOnly` contract with an assigned arbiter, driven to
/// `Disputed` with a live pre-dispute rollback snapshot.
fn disputed_contract() -> EscrowFixture {
    let fixture = EscrowFixture::builder()
        .with_generated_arbiter()
        .funded()
        .build();
    let escrow = fixture.escrow();
    assert!(escrow.raise_dispute(&fixture.escrow_id, &fixture.client));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Disputed
    );
    assert!(
        has_rollback_record(&fixture),
        "raising a dispute must capture a rollback snapshot"
    );
    drop(escrow);
    fixture
}

/// Whether a pre-dispute rollback snapshot is currently stored.
fn has_rollback_record(fixture: &EscrowFixture) -> bool {
    fixture.env.as_contract(&fixture.escrow_address, || {
        fixture
            .env
            .storage()
            .persistent()
            .has(&DataKey::DisputeRollback(fixture.escrow_id))
    })
}

/// Overwrite the persisted contract, bypassing the entrypoints. Used to stage
/// state that no legitimate flow can produce.
fn force_contract(fixture: &EscrowFixture, mutate: impl FnOnce(&mut Contract)) {
    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = DataKey::Contract(fixture.escrow_id);
        let mut contract: Contract = fixture
            .env
            .storage()
            .persistent()
            .get(&key)
            .expect("contract must exist");
        mutate(&mut contract);
        fixture.env.storage().persistent().set(&key, &contract);
    });
}

/// Overwrite the persisted milestone vector, bypassing the entrypoints.
fn force_milestones(fixture: &EscrowFixture, mutate: impl FnOnce(&mut Vec<Milestone>)) {
    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = crate::keys::milestone_key(&fixture.env, fixture.escrow_id);
        let mut milestones: Vec<Milestone> = fixture
            .env
            .storage()
            .persistent()
            .get(&key)
            .expect("milestones must exist");
        mutate(&mut milestones);
        fixture.env.storage().persistent().set(&key, &milestones);
    });
}

// ── R1: allowed pre-state ───────────────────────────────────────────────────

/// A contract that was never disputed has no snapshot to restore, so a rollback
/// is rejected as `RollbackNotAllowed` and leaves the contract untouched. The
/// same call succeeds once the dispute actually exists.
#[test]
fn rollback_is_rejected_unless_contract_is_disputed() {
    let fixture = EscrowFixture::builder()
        .with_generated_arbiter()
        .funded()
        .build();
    let escrow = fixture.escrow();

    assert_contract_error(
        escrow.try_rollback_dispute(&fixture.escrow_id),
        Error::RollbackNotAllowed,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
    assert!(!has_rollback_record(&fixture));

    assert!(escrow.raise_dispute(&fixture.escrow_id, &fixture.client));
    assert!(escrow.rollback_dispute(&fixture.escrow_id));
}

// ── R2: exact prior state restored ──────────────────────────────────────────

/// Rolling back restores the captured contract and milestone vector exactly;
/// there is no partial restore and the snapshot is retired afterwards.
#[test]
fn rollback_restores_exact_pre_dispute_state() {
    let fixture = EscrowFixture::builder()
        .with_generated_arbiter()
        .funded()
        .build();
    let escrow = fixture.escrow();

    let contract_before = escrow.get_contract(&fixture.escrow_id);
    let milestones_before = escrow.get_milestones(&fixture.escrow_id);

    assert!(escrow.raise_dispute(&fixture.escrow_id, &fixture.client));
    assert!(escrow.rollback_dispute(&fixture.escrow_id));

    assert_eq!(
        escrow.get_contract(&fixture.escrow_id),
        contract_before,
        "rollback must restore the whole pre-dispute contract"
    );
    assert_eq!(
        escrow.get_milestones(&fixture.escrow_id),
        milestones_before,
        "rollback must restore the whole pre-dispute milestone vector"
    );
    assert!(!has_rollback_record(&fixture));
}

// ── R3: no double-apply ─────────────────────────────────────────────────────

/// A successful rollback consumes its snapshot, so replaying the call fails
/// closed instead of re-applying the transition.
#[test]
fn rollback_cannot_double_apply() {
    let fixture = disputed_contract();
    let escrow = fixture.escrow();

    assert!(escrow.rollback_dispute(&fixture.escrow_id));
    assert!(!has_rollback_record(&fixture));

    assert_contract_error(
        escrow.try_rollback_dispute(&fixture.escrow_id),
        Error::RollbackNotAllowed,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
}

// ── R4: authorization ───────────────────────────────────────────────────────

/// The rollback gate is the stored admin's authorization. Without it the call
/// is rejected and the dispute (and its snapshot) is left intact.
#[test]
fn rollback_requires_admin_authorization() {
    let fixture = disputed_contract();
    let escrow = fixture.escrow();

    fixture.env.mock_auths(&[]);
    assert!(escrow.try_rollback_dispute(&fixture.escrow_id).is_err());
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Disputed
    );
    assert!(has_rollback_record(&fixture));
}

// ── R5: terminal / irreversible states ──────────────────────────────────────

/// Finalizing a disputed contract is irreversible; the seal guard fires before
/// the snapshot is consulted, so the rollback reports `AlreadyFinalized`.
#[test]
fn finalized_contract_cannot_be_rolled_back() {
    let fixture = disputed_contract();
    let escrow = fixture.escrow();
    let arbiter = fixture
        .arbiter
        .clone()
        .expect("fixture is created with an arbiter");

    assert!(escrow.finalize_contract(&fixture.escrow_id, &arbiter));
    assert_contract_error(
        escrow.try_rollback_dispute(&fixture.escrow_id),
        Error::AlreadyFinalized,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Disputed
    );
}

// ── R6: related approvals voided ────────────────────────────────────────────

/// A live approval is voided by a rollback, so consent recorded before the
/// dispute cannot be spent once the pre-dispute status is restored. No public
/// entrypoint can create an approval while `Disputed`, so the record is staged
/// directly to model one written before the raise-time clear existed.
#[test]
fn rollback_clears_related_approvals() {
    let fixture = disputed_contract();
    let escrow = fixture.escrow();

    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = crate::keys::milestone_approval_key(fixture.escrow_id, 0);
        fixture.env.storage().temporary().set(
            &key,
            &MilestoneApprovals {
                client_approved: true,
                freelancer_approved: false,
                arbiter_approved: false,
            },
        );
    });
    assert!(escrow
        .get_milestone_approvals(&fixture.escrow_id, &0)
        .is_some());

    assert!(escrow.rollback_dispute(&fixture.escrow_id));

    assert_eq!(escrow.get_milestone_approvals(&fixture.escrow_id, &0), None);
}

// ── R7: diverged snapshot rejected ──────────────────────────────────────────

/// If the live contract has diverged from the captured snapshot, the rollback
/// refuses rather than restoring a partial view, and the dispute stays intact.
#[test]
fn rollback_rejects_diverged_contract_snapshot() {
    let fixture = disputed_contract();
    let escrow = fixture.escrow();

    force_contract(&fixture, |contract| {
        contract.funded_amount -= 1;
    });

    assert_contract_error(
        escrow.try_rollback_dispute(&fixture.escrow_id),
        Error::RollbackNotAllowed,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Disputed
    );
    assert!(has_rollback_record(&fixture));
}

/// The milestone vector is part of the snapshot's integrity check; mutating any
/// milestone while disputed makes the rollback fail closed.
#[test]
fn rollback_rejects_diverged_milestone_snapshot() {
    let fixture = disputed_contract();
    let escrow = fixture.escrow();

    force_milestones(&fixture, |milestones| {
        let mut first = milestones.get(0).expect("contract has milestones");
        first.amount += 1;
        milestones.set(0, first);
    });

    assert_contract_error(
        escrow.try_rollback_dispute(&fixture.escrow_id),
        Error::RollbackNotAllowed,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Disputed
    );
    assert!(has_rollback_record(&fixture));
}
