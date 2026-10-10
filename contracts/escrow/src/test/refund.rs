use soroban_sdk::vec;

use super::{assert_contract_error, EscrowFixture, MILESTONE_TWO};
use crate::{ContractStatus, Error, EscrowError};
use soroban_sdk::Vec;

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
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).released_amount,
        0
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
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Completed
    );
}

/// Refunding the same milestone twice is rejected, preventing double refunds.
#[test]
fn refund_rejects_duplicate_refund() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 1_u32];

    assert_eq!(
        escrow.refund_unreleased_milestones(&fixture.escrow_id, &ids),
        MILESTONE_TWO
    );
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        Error::InvalidState,
    );
}

/// Refunding a milestone that was already released is rejected.
#[test]
fn refund_rejects_released_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &1_u32);
    escrow.release_milestone(&fixture.escrow_id, &fixture.client, &1_u32);

    let ids = vec![&fixture.env, 1_u32];
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        Error::InvalidState,
    );
}

/// Refunding an out-of-range milestone index is rejected without mutating state.
#[test]
fn refund_rejects_out_of_range_milestone() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 999_u32];

    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        Error::IndexOutOfBounds,
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
}

/// Refunding an empty milestone list is a no-op that preserves the contract state.
#[test]
fn refund_empty_list_is_no_op() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env];

    escrow.refund_unreleased_milestones(&fixture.escrow_id, &ids);
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).status,
        ContractStatus::Funded
    );
}

/// Refunds from an unfunded contract are rejected.
#[test]
fn refund_rejects_unfunded_contract() {
    let fixture = EscrowFixture::builder().build();
    let escrow = fixture.escrow();
    let ids = vec![&fixture.env, 1_u32];

    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &ids),
        Error::InvalidState,
    );
}

// Soroban commits conflicting transactions in a serial order. Exercise both
// possible winners rather than sharing a non-thread-safe test Env across threads.
#[test]
fn refund_release_race_has_only_one_winner() {
    for refund_first in [true, false] {
        let f = EscrowFixture::builder().funded().build();
        let c = f.escrow();
        c.approve_milestone_release(&f.escrow_id, &f.client, &0);
        let ids = vec![&f.env, 0_u32];
        if refund_first {
            c.refund_unreleased_milestones(&f.escrow_id, &ids);
            assert_contract_error(
                c.try_release_milestone(&f.escrow_id, &f.client, &0),
                Error::AlreadyRefunded,
            );
        } else {
            c.release_milestone(&f.escrow_id, &f.client, &0);
            assert_contract_error(
                c.try_refund_unreleased_milestones(&f.escrow_id, &ids),
                Error::MilestoneAlreadyReleased,
            );
        }
        let m = c.get_milestones(&f.escrow_id).get(0).unwrap();
        assert_ne!(m.released, m.refunded);
        let token = soroban_sdk::token::Client::new(&f.env, f.settlement_token.as_ref().unwrap());
        assert_eq!(
            token.balance(&f.escrow_address),
            f.total_amount() - super::MILESTONE_ONE
        );
    }
}

#[test]
fn overlapping_refund_batches_and_retries_do_not_double_pay() {
    for first in [0_u32, 1_u32] {
        let f = EscrowFixture::builder().funded().build();
        let c = f.escrow();
        let ids = vec![&f.env, first];
        let paid = c.refund_unreleased_milestones(&f.escrow_id, &ids);
        let token = soroban_sdk::token::Client::new(&f.env, f.settlement_token.as_ref().unwrap());
        let balance = token.balance(&f.client);
        assert_contract_error(
            c.try_refund_unreleased_milestones(&f.escrow_id, &ids),
            Error::AlreadyRefunded,
        );
        assert_contract_error(
            c.try_refund_unreleased_milestones(&f.escrow_id, &vec![&f.env, 0_u32, 1_u32]),
            Error::AlreadyRefunded,
        );
        assert_eq!(token.balance(&f.client), balance);
        assert_eq!(c.get_contract(&f.escrow_id).refunded_amount, paid);
        assert!(
            !c.get_milestones(&f.escrow_id)
                .get(1 - first)
                .unwrap()
                .refunded
        );
    }
}

#[test]
fn invalid_batch_leaves_every_milestone_unmodified() {
    let f = EscrowFixture::builder().funded().build();
    let c = f.escrow();
    for (ids, error) in [
        (vec![&f.env], Error::EmptyRefundRequest),
        (
            vec![&f.env, 0_u32, 0_u32],
            Error::DuplicateMilestoneInRefund,
        ),
        (vec![&f.env, 0_u32, 3_u32], Error::IndexOutOfBounds),
    ] {
        assert_contract_error(
            c.try_refund_unreleased_milestones(&f.escrow_id, &ids),
            error,
        );
        assert_eq!(c.get_contract(&f.escrow_id).refunded_amount, 0);
        assert!(c.get_milestones(&f.escrow_id).iter().all(|m| !m.refunded));
    }
}

#[test]
fn refund_deadline_boundary_and_retry() {
    use soroban_sdk::testutils::Ledger;
    let f = EscrowFixture::builder().funded().build();
    f.env.as_contract(&f.escrow_address, || {
        let mut milestones = crate::ttl::load_milestones(&f.env, f.escrow_id);
        let mut m = milestones.get(0).unwrap();
        m.deadline = Some(100);
        milestones.set(0, m);
        crate::ttl::store_milestones(&f.env, f.escrow_id, &milestones);
    });
    let c = f.escrow();
    let ids = vec![&f.env, 0_u32];
    for now in [99, 100] {
        f.env.ledger().with_mut(|ledger| ledger.timestamp = now);
        assert_contract_error(
            c.try_refund_unreleased_milestones(&f.escrow_id, &ids),
            Error::MilestoneNotOverdue,
        );
        assert_eq!(c.get_contract(&f.escrow_id).refunded_amount, 0);
    }
    f.env.ledger().with_mut(|ledger| ledger.timestamp = 101);
    assert_eq!(
        c.refund_unreleased_milestones(&f.escrow_id, &ids),
        super::MILESTONE_ONE
    );
}

#[test]
fn failed_token_transfer_rolls_back_and_can_be_retried() {
    for release_first in [false, true] {
        let f = EscrowFixture::builder().funded().build();
        let c = f.escrow();
        if release_first {
            c.approve_milestone_release(&f.escrow_id, &f.client, &0);
            c.release_milestone(&f.escrow_id, &f.client, &0);
        }
        let asset = soroban_sdk::token::StellarAssetClient::new(
            &f.env,
            f.settlement_token.as_ref().unwrap(),
        );
        let token = soroban_sdk::token::Client::new(&f.env, f.settlement_token.as_ref().unwrap());
        let remaining = token.balance(&f.escrow_address);
        f.env.as_contract(&f.escrow_address, || {
            token.transfer(&f.escrow_address, &f.admin, &remaining);
        });
        let ids = if release_first {
            vec![&f.env, 1_u32, 2_u32]
        } else {
            vec![&f.env, 0_u32, 1_u32, 2_u32]
        };
        assert!(c
            .try_refund_unreleased_milestones(&f.escrow_id, &ids)
            .is_err());
        assert_eq!(c.get_contract(&f.escrow_id).status, ContractStatus::Funded);
        assert_eq!(c.get_contract(&f.escrow_id).refunded_amount, 0);
        assert!(c.get_milestones(&f.escrow_id).iter().all(|m| !m.refunded));
        assert_eq!(c.get_pending_reputation_credits(&f.freelancer), 0);
        assert_eq!(token.balance(&f.escrow_address), 0);
        asset.mint(&f.escrow_address, &remaining);
        assert_eq!(
            c.refund_unreleased_milestones(&f.escrow_id, &ids),
            remaining
        );
        let expected = if release_first {
            ContractStatus::Completed
        } else {
            ContractStatus::Refunded
        };
        assert_eq!(c.get_contract(&f.escrow_id).status, expected);
        assert_eq!(
            c.get_pending_reputation_credits(&f.freelancer),
            if release_first { 1 } else { 0 }
        );
        assert_contract_error(
            c.try_refund_unreleased_milestones(&f.escrow_id, &ids),
            Error::InvalidState,
        );
        assert_eq!(
            c.get_pending_reputation_credits(&f.freelancer),
            if release_first { 1 } else { 0 }
        );
    }
}

#[test]
fn partial_funding_cannot_refund_retained_release_fees() {
    let f = EscrowFixture::builder().with_settlement_token().build();
    let c = f.escrow();
    let token_address = f.settlement_token.as_ref().unwrap();
    let asset = soroban_sdk::token::StellarAssetClient::new(&f.env, token_address);
    let token = soroban_sdk::token::Client::new(&f.env, token_address);
    let partial = super::MILESTONE_ONE + MILESTONE_TWO - 1;
    asset.mint(&f.client, &partial);
    c.deposit_funds(&f.escrow_id, &f.client, &partial);
    // Current callers require full funding before releases. Seed a legacy or
    // inconsistent Funded record to ensure the refund guard still fails closed.
    f.env.as_contract(&f.escrow_address, || {
        let key = crate::DataKey::Contract(f.escrow_id);
        let mut data: crate::Contract = f.env.storage().persistent().get(&key).unwrap();
        data.status = ContractStatus::Funded;
        f.env.storage().persistent().set(&key, &data);
    });
    c.set_protocol_fee_bps(&1000_u32, &1_u64);
    c.approve_milestone_release(&f.escrow_id, &f.client, &0);
    c.release_milestone(&f.escrow_id, &f.client, &0);
    let balance = token.balance(&f.escrow_address);
    assert_contract_error(
        c.try_refund_unreleased_milestones(&f.escrow_id, &vec![&f.env, 1_u32]),
        Error::InsufficientFunds,
    );
    assert_eq!(token.balance(&f.escrow_address), balance);
    assert_eq!(c.get_contract(&f.escrow_id).refunded_amount, 0);
    // Correcting the legacy funding record by the exact missing stroop makes
    // the same retry safe. Public deposits reject post-release changes.
    asset.mint(&f.escrow_address, &1);
    f.env.as_contract(&f.escrow_address, || {
        let key = crate::DataKey::Contract(f.escrow_id);
        let mut data: crate::Contract = f.env.storage().persistent().get(&key).unwrap();
        data.funded_amount += 1;
        f.env.storage().persistent().set(&key, &data);
    });
    assert_eq!(
        c.refund_unreleased_milestones(&f.escrow_id, &vec![&f.env, 1_u32]),
        MILESTONE_TWO
    );
    assert_eq!(token.balance(&f.escrow_address), super::MILESTONE_ONE / 10);
}

#[test]
fn disjoint_refunds_commute_and_preserve_terminal_accounting() {
    for order in [[0_u32, 1, 2], [2_u32, 1, 0]] {
        let f = EscrowFixture::builder().funded().build();
        let c = f.escrow();
        for index in order {
            c.refund_unreleased_milestones(&f.escrow_id, &vec![&f.env, index]);
        }
        let data = c.get_contract(&f.escrow_id);
        assert_eq!(data.refunded_amount, f.total_amount());
        assert_eq!(data.released_amount, 0);
        assert_eq!(data.status, ContractStatus::Refunded);
        let token = soroban_sdk::token::Client::new(&f.env, f.settlement_token.as_ref().unwrap());
        assert_eq!(token.balance(&f.escrow_address), 0);
        assert_eq!(token.balance(&f.client), f.total_amount());
    }
}

#[test]
fn refund_requires_client_authorization_and_respects_pause() {
    let f = EscrowFixture::builder().funded().build();
    let c = f.escrow();
    c.pause(&1_u64);
    assert_contract_error(
        c.try_refund_unreleased_milestones(&f.escrow_id, &vec![&f.env, 0_u32]),
        Error::ContractPaused,
    );
    c.unpause();
    f.env.mock_auths(&[]);
    assert!(c
        .try_refund_unreleased_milestones(&f.escrow_id, &vec![&f.env, 0_u32])
        .is_err());
    assert_eq!(c.get_contract(&f.escrow_id).refunded_amount, 0);
}

#[test]
fn overflowing_refund_sum_fails_before_any_effects() {
    let f = EscrowFixture::builder().funded().build();
    f.env.as_contract(&f.escrow_address, || {
        let mut milestones = crate::ttl::load_milestones(&f.env, f.escrow_id);
        let mut first = milestones.get(0).unwrap();
        first.amount = i128::MAX;
        milestones.set(0, first);
        crate::ttl::store_milestones(&f.env, f.escrow_id, &milestones);
    });
    let c = f.escrow();
    assert_contract_error(
        c.try_refund_unreleased_milestones(&f.escrow_id, &vec![&f.env, 0_u32, 1_u32]),
        Error::PotentialOverflow,
    );
    assert_eq!(c.get_contract(&f.escrow_id).refunded_amount, 0);
    assert!(c.get_milestones(&f.escrow_id).iter().all(|m| !m.refunded));
}

#[test]
fn negative_accounting_cannot_increase_refundable_funds() {
    let f = EscrowFixture::builder().funded().build();
    f.env.as_contract(&f.escrow_address, || {
        let key = crate::DataKey::Contract(f.escrow_id);
        let mut data: crate::Contract = f.env.storage().persistent().get(&key).unwrap();
        data.refunded_amount = -1;
        f.env.storage().persistent().set(&key, &data);
    });
    assert_contract_error(
        f.escrow()
            .try_refund_unreleased_milestones(&f.escrow_id, &vec![&f.env, 0_u32]),
        Error::AccountingInvariantViolated,
    );
    assert!(
        !f.escrow()
            .get_milestones(&f.escrow_id)
            .get(0)
            .unwrap()
            .refunded
    );
}

/// Invalid request shapes are rejected before escrow accounting or milestone flags change.
#[test]
fn refund_rejects_empty_duplicate_and_out_of_range_requests_without_mutation() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &vec![&fixture.env]),
        EscrowError::EmptyRefundRequest,
    );
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(
            &fixture.escrow_id,
            &vec![&fixture.env, 1_u32, 1_u32],
        ),
        EscrowError::DuplicateMilestoneInRefund,
    );
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &vec![&fixture.env, u32::MAX]),
        Error::IndexOutOfBounds,
    );

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.refunded_amount, 0);
    assert_eq!(
        escrow.get_refundable_balance(&fixture.escrow_id),
        fixture.total_amount()
    );
    assert!(escrow
        .get_milestones(&fixture.escrow_id)
        .iter()
        .all(|m| !m.refunded));
}

/// Corrupt accounting fails closed with a diagnostic invariant error.
#[test]
fn refund_rejects_corrupt_available_balance_before_transfer() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = crate::DataKey::Contract(fixture.escrow_id);
        let mut contract: crate::Contract = fixture.env.storage().persistent().get(&key).unwrap();
        contract.released_amount = contract.funded_amount + 1;
        fixture.env.storage().persistent().set(&key, &contract);
    });

    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &vec![&fixture.env, 0_u32]),
        Error::AccountingInvariantViolated,
    );

    let milestones = escrow.get_milestones(&fixture.escrow_id);
    assert!(!milestones.get(0).unwrap().refunded);
}

/// Malformed stored milestone amounts cannot produce zero or negative SAC transfers.
#[test]
fn refund_rejects_non_positive_milestone_amount() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = (
            crate::DataKey::Contract(fixture.escrow_id),
            soroban_sdk::Symbol::new(&fixture.env, "milestones"),
        );
        let mut milestones: Vec<crate::Milestone> =
            fixture.env.storage().persistent().get(&key).unwrap();
        let mut milestone = milestones.get(0).unwrap();
        milestone.amount = 0;
        milestones.set(0, milestone);
        fixture.env.storage().persistent().set(&key, &milestones);
    });

    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &vec![&fixture.env, 0_u32]),
        Error::AmountMustBePositive,
    );
    assert!(
        !escrow
            .get_milestones(&fixture.escrow_id)
            .get(0)
            .unwrap()
            .refunded
    );
}
