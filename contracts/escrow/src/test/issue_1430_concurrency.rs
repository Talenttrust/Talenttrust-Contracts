//! Production-entrypoint tests for optimistic milestone concurrency (#1430).

use super::{assert_contract_error, EscrowFixture};
use crate::EscrowError;
use soroban_sdk::testutils::Events;

#[test]
fn versioned_release_rejects_stale_request_without_second_event_or_mutation() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let milestone_index = 0;
    let amount = escrow
        .get_milestone(&fixture.escrow_id, &milestone_index)
        .unwrap()
        .amount;
    let observed = escrow.get_milestone_version(&fixture.escrow_id, &milestone_index);
    assert_eq!(observed, 0);

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &milestone_index);
    let before_release = fixture.env.events().all().len();
    assert!(escrow.release_milestone_with_version(
        &fixture.escrow_id,
        &fixture.client,
        &milestone_index,
        &observed,
    ));
    assert_eq!(fixture.env.events().all().len(), before_release + 1);
    assert_eq!(
        escrow.get_milestone_version(&fixture.escrow_id, &milestone_index),
        1
    );

    let before_stale = fixture.env.events().all().len();
    let stale = escrow.try_release_milestone_with_version(
        &fixture.escrow_id,
        &fixture.client,
        &milestone_index,
        &observed,
    );
    assert_contract_error(stale, EscrowError::StaleMilestoneVersion);
    assert_eq!(fixture.env.events().all().len(), before_stale);
    assert!(
        escrow
            .get_milestone(&fixture.escrow_id, &milestone_index)
            .unwrap()
            .released
    );
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).released_amount,
        amount
    );

    // A caller that refreshes the version reaches normal state validation;
    // the terminal transition is rejected without a second event.
    let refreshed = escrow.get_milestone_version(&fixture.escrow_id, &milestone_index);
    let refreshed_retry = escrow.try_release_milestone_with_version(
        &fixture.escrow_id,
        &fixture.client,
        &milestone_index,
        &refreshed,
    );
    assert_contract_error(refreshed_retry, EscrowError::MilestoneAlreadyReleased);
    assert_eq!(fixture.env.events().all().len(), before_stale);
}

#[test]
fn legacy_release_entrypoint_remains_compatible_and_initial_version_is_zero() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    assert_eq!(escrow.get_milestone_version(&fixture.escrow_id, &0), 0);
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));
    assert_eq!(escrow.get_milestone_version(&fixture.escrow_id, &0), 1);
    let stale_legacy_result =
        escrow.try_release_milestone_with_version(&fixture.escrow_id, &fixture.client, &0, &0);
    assert_contract_error(stale_legacy_result, EscrowError::StaleMilestoneVersion);
}

#[test]
fn versioned_batch_validates_all_versions_before_any_release_or_event() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &1);
    let indices = soroban_sdk::vec![&fixture.env, 0u32, 1u32];
    let versions = soroban_sdk::vec![&fixture.env, 0u32, 0u32];
    // A stale second item must reject the whole batch before item zero mutates.
    let stale_second = soroban_sdk::vec![&fixture.env, 0u32, 1u32];
    let rejected =
        escrow.try_release_batch_v(&fixture.escrow_id, &fixture.client, &indices, &stale_second);
    assert_contract_error(rejected, EscrowError::StaleMilestoneVersion);
    assert_eq!(fixture.env.events().all().len(), 0);
    assert!(
        !escrow
            .get_milestone(&fixture.escrow_id, &0)
            .unwrap()
            .released
    );
    assert!(
        !escrow
            .get_milestone(&fixture.escrow_id, &1)
            .unwrap()
            .released
    );

    let before_success = fixture.env.events().all().len();

    assert!(escrow.release_batch_v(&fixture.escrow_id, &fixture.client, &indices, &versions));
    let after_success = fixture.env.events().all().len();
    assert!(after_success > before_success);
    assert_eq!(escrow.get_milestone_version(&fixture.escrow_id, &0), 1);
    assert_eq!(escrow.get_milestone_version(&fixture.escrow_id, &1), 1);

    let before_stale = fixture.env.events().all().len();
    let stale_batch =
        escrow.try_release_batch_v(&fixture.escrow_id, &fixture.client, &indices, &versions);
    assert_contract_error(stale_batch, EscrowError::StaleMilestoneVersion);
    assert_eq!(fixture.env.events().all().len(), before_stale);
    assert!(
        escrow
            .get_milestone(&fixture.escrow_id, &0)
            .unwrap()
            .released
    );
    assert!(
        escrow
            .get_milestone(&fixture.escrow_id, &1)
            .unwrap()
            .released
    );
}
