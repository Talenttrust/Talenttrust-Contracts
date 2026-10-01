//! Focused boundary tests for the release validation boundary defined in
//! [`crate::release`] and enforced by `release_milestone` /
//! `release_milestone_batch`.
//!
//! The boundary has to make four classes of request deterministic — **accepted**,
//! **rejected**, **duplicate** submissions, and exact **boundary** values — and
//! every rejection has to leave the escrow byte-identical so a client can retry
//! the same request safely. These tests drive that contract through the public
//! entrypoints and assert the exact typed error plus an unchanged snapshot.
//!
//! | Scenario | Asserted outcome |
//! | --- | --- |
//! | One approved milestone | `true`, amount released, custody moves to the freelancer |
//! | First / last index of the schedule | accepted |
//! | Batch of one index (minimum shape) | accepted |
//! | Batch of distinct indices, any order | every target released, total exact |
//! | Whole schedule in one batch | `Completed`, `released == funded`, custody empty |
//! | Batch of exactly `MAX_BATCH_MILESTONES` | accepted (upper boundary) |
//! | Empty batch | `EmptyBatch` |
//! | Duplicate indices, adjacent and spanning | `DuplicateMilestoneInBatch` |
//! | Batch of `MAX_BATCH_MILESTONES + 1` | `BatchLimitExceeded` |
//! | Index equal to / above the schedule length, `u32::MAX` | `IndexOutOfBounds` |
//! | Caller not admitted by the release mode | `UnauthorizedRole` |
//! | `MultiSig` with one approval only | `InsufficientApprovals` |
//! | `MultiSig` with both approvals | accepted, by either party |
//! | `ArbiterOnly` release by the client | `UnauthorizedRole` |
//! | Disputed / completed contract | `InvalidState` |
//! | No approval recorded | `InsufficientApprovals` |
//! | Replay of an accepted release | `MilestoneAlreadyReleased`, no state change |
//! | Already refunded milestone | `AlreadyRefunded` |
//! | Unknown contract id | `ContractNotFound` |
//! | Paused contract | `ContractPaused` |
//! | Batch with one invalid target | whole batch reverts, valid target untouched |
//! | Same request after fixing the blocker | succeeds |
//! | Same rejected request twice | identical error both times |
//!
//! # Accounting invariant
//! Every accepted release is checked against
//! `released_amount + refunded_amount + accumulated_fees <= funded_amount`, and
//! every rejected release must leave it intact — asserted on each rejection by
//! comparing a full snapshot of the contract record, the milestone schedule and
//! all three token balances.
//!
//! # Protocol fees
//! Fee-specific boundaries (rate ceiling, net payout, fee accrual) are pinned by
//! the unit tests in [`crate::release`] and by `test::protocol_fees`; the
//! fixtures here leave the fee at `0` so the assertions stay on custody.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token, vec, Vec};

use super::{assert_contract_error, EscrowFixture, MILESTONE_ONE, MILESTONE_THREE, MILESTONE_TWO};
use crate::milestones_consts::MAX_BATCH_MILESTONES;
use crate::{Contract, ContractStatus, DataKey, Error, Milestone, ReleaseAuthorization};

/// One whole-escrow snapshot: a rejected request must leave both copies equal.
#[derive(Clone, Debug, PartialEq)]
struct EscrowSnapshot {
    contract: Contract,
    milestones: Vec<Milestone>,
    escrow_balance: i128,
    client_balance: i128,
    freelancer_balance: i128,
}

fn snapshot(fixture: &EscrowFixture) -> EscrowSnapshot {
    let escrow = fixture.escrow();
    let settlement_token = fixture
        .settlement_token
        .clone()
        .expect("funded fixtures bind a settlement token");
    let token_client = token::Client::new(&fixture.env, &settlement_token);

    EscrowSnapshot {
        contract: escrow.get_contract(&fixture.escrow_id),
        milestones: escrow.get_milestones(&fixture.escrow_id),
        escrow_balance: token_client.balance(&fixture.escrow_address),
        client_balance: token_client.balance(&fixture.client),
        freelancer_balance: token_client.balance(&fixture.freelancer),
    }
}

/// Assert that a rejected call left the whole escrow byte-identical.
///
/// The snapshot comparison is the retry guarantee: identical state means the
/// identical request can be resubmitted, and the accounting invariant is
/// re-checked here so a rejection can never be observed as a silent drift.
#[track_caller]
fn assert_rejected(fixture: &EscrowFixture, before: &EscrowSnapshot) {
    let after = snapshot(fixture);
    assert_eq!(
        &after, before,
        "a rejected release must leave the escrow byte-identical so the request can be retried"
    );
    assert!(
        after.contract.released_amount + after.contract.refunded_amount
            <= after.contract.funded_amount,
        "accounting invariant must survive a rejected release"
    );
}

/// Record a client approval for `index` on the fixture's contract.
fn approve(fixture: &EscrowFixture, index: u32) {
    fixture
        .escrow()
        .approve_milestone_release(&fixture.escrow_id, &fixture.client, &index);
}

/// Approve every milestone of the fixture's schedule.
fn approve_all(fixture: &EscrowFixture) {
    let count = fixture.escrow().get_milestones(&fixture.escrow_id).len();
    for index in 0..count {
        approve(fixture, index);
    }
}

/// Overwrite the contract record directly in persistent storage.
///
/// Used only to reach states no entrypoint can produce from a funded fixture
/// (a disputed contract), mirroring the pattern in `refund_validation_boundaries`.
fn set_contract(fixture: &EscrowFixture, mutate: impl FnOnce(&mut Contract)) {
    fixture.env.as_contract(&fixture.escrow_address, || {
        let key = DataKey::Contract(fixture.escrow_id);
        let mut contract: Contract = fixture
            .env
            .storage()
            .persistent()
            .get(&key)
            .expect("contract is stored by the funded fixture");
        mutate(&mut contract);
        fixture.env.storage().persistent().set(&key, &contract);
    });
}

/// Pause the contract through the admin entrypoint.
///
/// The fixture initializes the admin with nonce `1` as its first available
/// nonce, so the first admin operation uses it.
fn pause(fixture: &EscrowFixture) {
    fixture.escrow().pause(&1u64);
}

/// The default fixture: three funded milestones of 200 / 400 / 600, `ClientOnly`.
fn funded_fixture() -> EscrowFixture {
    EscrowFixture::builder().funded().build()
}

/// A contract with exactly `MAX_BATCH_MILESTONES` milestones, so the batch-size
/// boundary can be exercised from both sides.
fn max_milestone_fixture() -> EscrowFixture {
    EscrowFixture::builder()
        .funded()
        .with_milestone_amounts(&[1_0000000; MAX_BATCH_MILESTONES as usize])
        .build()
}

// ── Accepted input ──────────────────────────────────────────────────────────

#[test]
fn release_accepts_a_single_approved_milestone() {
    let fixture = funded_fixture();
    let before = snapshot(&fixture);
    approve(&fixture, 0);

    assert!(fixture
        .escrow()
        .release_milestone(&fixture.escrow_id, &fixture.client, &0));

    let after = snapshot(&fixture);
    assert_eq!(after.contract.released_amount, MILESTONE_ONE);
    assert_eq!(
        after.contract.status,
        ContractStatus::Funded,
        "one outstanding milestone keeps the contract active"
    );
    assert!(after.milestones.get(0).unwrap().released);
    assert!(!after.milestones.get(1).unwrap().released);
    assert_eq!(
        after.escrow_balance,
        before.escrow_balance - MILESTONE_ONE,
        "custody pays exactly the milestone amount"
    );
    assert_eq!(
        after.freelancer_balance,
        before.freelancer_balance + MILESTONE_ONE
    );
    assert_eq!(after.client_balance, before.client_balance);
    assert_eq!(
        fixture.escrow().get_refundable_balance(&fixture.escrow_id),
        MILESTONE_TWO + MILESTONE_THREE
    );
}

/// The first and the last index of the schedule are both legal boundaries.
#[test]
fn release_accepts_the_first_and_last_index() {
    let fixture = funded_fixture();
    let escrow = fixture.escrow();
    approve(&fixture, 0);
    approve(&fixture, 2);

    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &2));

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.released_amount, MILESTONE_ONE + MILESTONE_THREE);
    let milestones = escrow.get_milestones(&fixture.escrow_id);
    assert!(milestones.get(0).unwrap().released);
    assert!(!milestones.get(1).unwrap().released);
    assert!(milestones.get(2).unwrap().released);
}

/// Boundary: a release of exactly the remaining balance is accepted and settles
/// the contract with empty custody.
#[test]
fn release_accepts_exactly_the_remaining_balance() {
    let fixture = EscrowFixture::builder()
        .funded()
        .with_single_milestone(MILESTONE_ONE)
        .build();
    approve(&fixture, 0);
    let before = snapshot(&fixture);

    assert!(fixture
        .escrow()
        .release_milestone(&fixture.escrow_id, &fixture.client, &0));

    let after = snapshot(&fixture);
    assert_eq!(after.escrow_balance, 0, "custody is fully consumed");
    assert_eq!(after.contract.released_amount, after.contract.funded_amount);
    assert_eq!(after.contract.status, ContractStatus::Completed);
    assert_eq!(after.contract.refunded_amount, 0);
    assert!(after.escrow_balance < before.escrow_balance);
}

/// Boundary: a batch of one index is the minimum accepted shape.
#[test]
fn batch_accepts_a_single_index() {
    let fixture = funded_fixture();
    approve(&fixture, 1);

    assert!(fixture.escrow().release_milestone_batch(
        &fixture.escrow_id,
        &fixture.client,
        &vec![&fixture.env, 1_u32],
    ));

    let contract = fixture.escrow().get_contract(&fixture.escrow_id);
    assert_eq!(contract.released_amount, MILESTONE_TWO);
    let milestones = fixture.escrow().get_milestones(&fixture.escrow_id);
    assert!(!milestones.get(0).unwrap().released);
    assert!(milestones.get(1).unwrap().released);
    assert!(!milestones.get(2).unwrap().released);
}

#[test]
fn batch_accepts_distinct_indices_in_request_order() {
    let fixture = funded_fixture();
    let before = snapshot(&fixture);
    approve(&fixture, 2);
    approve(&fixture, 0);

    assert!(fixture.escrow().release_milestone_batch(
        &fixture.escrow_id,
        &fixture.client,
        &vec![&fixture.env, 2_u32, 0_u32],
    ));

    let after = snapshot(&fixture);
    assert_eq!(
        after.contract.released_amount,
        MILESTONE_ONE + MILESTONE_THREE
    );
    assert_eq!(
        after.contract.status,
        ContractStatus::Funded,
        "the middle milestone is still outstanding"
    );
    let milestones = fixture.escrow().get_milestones(&fixture.escrow_id);
    assert!(milestones.get(0).unwrap().released);
    assert!(!milestones.get(1).unwrap().released);
    assert!(milestones.get(2).unwrap().released);
    assert_eq!(
        after.escrow_balance,
        before.escrow_balance - MILESTONE_ONE - MILESTONE_THREE
    );
    assert_eq!(
        after.freelancer_balance,
        before.freelancer_balance + MILESTONE_ONE + MILESTONE_THREE
    );
}

/// Releasing the whole schedule in one batch settles the contract.
#[test]
fn batch_accepts_the_whole_schedule() {
    let fixture = funded_fixture();
    approve_all(&fixture);

    assert!(fixture.escrow().release_milestone_batch(
        &fixture.escrow_id,
        &fixture.client,
        &vec![&fixture.env, 0_u32, 1_u32, 2_u32],
    ));

    let after = snapshot(&fixture);
    assert_eq!(after.contract.status, ContractStatus::Completed);
    assert_eq!(after.contract.released_amount, after.contract.funded_amount);
    assert_eq!(after.escrow_balance, 0);
    assert!(after.milestones.iter().all(|m| m.released));
}

/// Boundary: a batch of exactly `MAX_BATCH_MILESTONES` is accepted.
#[test]
fn batch_accepts_the_protocol_maximum() {
    let fixture = max_milestone_fixture();
    approve_all(&fixture);
    let mut indices = Vec::new(&fixture.env);
    for index in 0..MAX_BATCH_MILESTONES {
        indices.push_back(index);
    }

    assert!(fixture.escrow().release_milestone_batch(
        &fixture.escrow_id,
        &fixture.client,
        &indices
    ));

    let contract = fixture.escrow().get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, contract.funded_amount);
    assert_eq!(
        contract.funded_amount,
        MAX_BATCH_MILESTONES as i128 * 1_0000000
    );
}

// ── Rejected input: request shape ───────────────────────────────────────────

#[test]
fn batch_rejects_an_empty_request() {
    let fixture = funded_fixture();
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture.escrow().try_release_milestone_batch(
            &fixture.escrow_id,
            &fixture.client,
            &vec![&fixture.env],
        ),
        Error::EmptyBatch,
    );
    assert_rejected(&fixture, &before);
}

#[test]
fn batch_rejects_adjacent_duplicates() {
    let fixture = funded_fixture();
    approve_all(&fixture);
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture.escrow().try_release_milestone_batch(
            &fixture.escrow_id,
            &fixture.client,
            &vec![&fixture.env, 0_u32, 0_u32],
        ),
        Error::DuplicateMilestoneInBatch,
    );
    assert_rejected(&fixture, &before);
}

#[test]
fn batch_rejects_duplicates_spanning_the_batch() {
    let fixture = funded_fixture();
    approve_all(&fixture);
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture.escrow().try_release_milestone_batch(
            &fixture.escrow_id,
            &fixture.client,
            &vec![&fixture.env, 0_u32, 1_u32, 2_u32, 0_u32],
        ),
        Error::DuplicateMilestoneInBatch,
    );
    assert_rejected(&fixture, &before);
}

/// Boundary: one index past the protocol maximum is the first rejection.
#[test]
fn batch_rejects_one_index_past_the_protocol_maximum() {
    let fixture = max_milestone_fixture();
    approve_all(&fixture);
    let before = snapshot(&fixture);
    let mut indices = Vec::new(&fixture.env);
    for index in 0..=MAX_BATCH_MILESTONES {
        indices.push_back(index);
    }

    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone_batch(&fixture.escrow_id, &fixture.client, &indices),
        Error::BatchLimitExceeded,
    );
    assert_rejected(&fixture, &before);
}

/// The batch-size cap is reported before the per-index bounds check, so a
/// hostile oversized request is refused without scanning the schedule.
#[test]
fn batch_reports_the_size_cap_before_an_out_of_range_index() {
    let fixture = max_milestone_fixture();
    let mut indices = Vec::new(&fixture.env);
    for index in 0..=MAX_BATCH_MILESTONES {
        indices.push_back(index * 7);
    }

    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone_batch(&fixture.escrow_id, &fixture.client, &indices),
        Error::BatchLimitExceeded,
    );
}

// ── Rejected input: index bounds ────────────────────────────────────────────

#[test]
fn rejects_an_index_equal_to_the_schedule_length() {
    let fixture = funded_fixture();
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &3),
        Error::IndexOutOfBounds,
    );
    assert_rejected(&fixture, &before);
}

#[test]
fn rejects_an_index_above_the_schedule_length() {
    let fixture = funded_fixture();
    let before = snapshot(&fixture);

    for index in [4_u32, u32::MAX] {
        assert_contract_error(
            fixture
                .escrow()
                .try_release_milestone(&fixture.escrow_id, &fixture.client, &index),
            Error::IndexOutOfBounds,
        );
    }
    assert_rejected(&fixture, &before);
}

// ── Rejected input: authority ───────────────────────────────────────────────

#[test]
fn rejects_a_caller_the_release_mode_does_not_admit() {
    let fixture = funded_fixture();
    approve(&fixture, 0);
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.freelancer, &0),
        Error::UnauthorizedRole,
    );
    assert_rejected(&fixture, &before);
}

#[test]
fn rejects_an_unknown_caller_in_every_release_mode() {
    for mode in [
        ReleaseAuthorization::ClientOnly,
        ReleaseAuthorization::ClientAndArbiter,
        ReleaseAuthorization::MultiSig,
    ] {
        let fixture = EscrowFixture::builder()
            .funded()
            .release_authorization(mode)
            .build();
        let stranger = soroban_sdk::Address::generate(&fixture.env);
        let before = snapshot(&fixture);

        assert_contract_error(
            fixture
                .escrow()
                .try_release_milestone(&fixture.escrow_id, &stranger, &0),
            Error::UnauthorizedRole,
        );
        assert_rejected(&fixture, &before);
    }
}

#[test]
fn multisig_requires_both_approvals() {
    let fixture = EscrowFixture::builder()
        .funded()
        .release_authorization(ReleaseAuthorization::MultiSig)
        .build();
    let escrow = fixture.escrow();
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    let before = snapshot(&fixture);

    assert_contract_error(
        escrow.try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        Error::InsufficientApprovals,
    );
    assert_rejected(&fixture, &before);

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.freelancer, &0);
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).released_amount,
        MILESTONE_ONE
    );
}

/// Either party may submit a `MultiSig` release once both approvals exist.
#[test]
fn multisig_accepts_the_freelancer_once_both_parties_approved() {
    let fixture = EscrowFixture::builder()
        .funded()
        .release_authorization(ReleaseAuthorization::MultiSig)
        .build();
    let escrow = fixture.escrow();
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.freelancer, &0);

    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.freelancer, &0));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).released_amount,
        MILESTONE_ONE
    );
}

#[test]
fn arbiter_only_rejects_the_client() {
    let fixture = EscrowFixture::builder()
        .funded()
        .with_generated_arbiter()
        .release_authorization(ReleaseAuthorization::ArbiterOnly)
        .build();
    let arbiter = fixture.arbiter.clone().expect("arbiter configured");
    let escrow = fixture.escrow();
    escrow.approve_milestone_release(&fixture.escrow_id, &arbiter, &0);
    let before = snapshot(&fixture);

    assert_contract_error(
        escrow.try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        Error::UnauthorizedRole,
    );
    assert_rejected(&fixture, &before);
}

#[test]
fn arbiter_only_accepts_the_arbiter() {
    let fixture = EscrowFixture::builder()
        .funded()
        .with_generated_arbiter()
        .release_authorization(ReleaseAuthorization::ArbiterOnly)
        .build();
    let arbiter = fixture.arbiter.clone().expect("arbiter configured");
    let escrow = fixture.escrow();
    escrow.approve_milestone_release(&fixture.escrow_id, &arbiter, &0);

    assert!(escrow.release_milestone(&fixture.escrow_id, &arbiter, &0));
    assert_eq!(
        escrow.get_contract(&fixture.escrow_id).released_amount,
        MILESTONE_ONE
    );
}

#[test]
fn rejects_a_release_without_an_approval() {
    let fixture = funded_fixture();
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        Error::InsufficientApprovals,
    );
    assert_rejected(&fixture, &before);
}

// ── Rejected input: lifecycle state ─────────────────────────────────────────

#[test]
fn rejects_a_disputed_contract() {
    let fixture = funded_fixture();
    approve(&fixture, 0);
    set_contract(&fixture, |contract| {
        contract.status = ContractStatus::Disputed
    });
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        Error::InvalidState,
    );
    assert_rejected(&fixture, &before);
}

#[test]
fn rejects_a_completed_contract() {
    let fixture = EscrowFixture::builder().completed().build();
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        Error::InvalidState,
    );
    assert_rejected(&fixture, &before);
}

#[test]
fn rejects_a_paused_contract() {
    let fixture = funded_fixture();
    approve(&fixture, 0);
    pause(&fixture);
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        Error::ContractPaused,
    );
    assert_rejected(&fixture, &before);
}

#[test]
fn rejects_an_unknown_contract_id() {
    let fixture = funded_fixture();
    approve(&fixture, 0);
    let unknown = fixture.escrow_id + 1_000;

    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&unknown, &fixture.client, &0),
        Error::ContractNotFound,
    );
}

// ── Rejected input: already settled ─────────────────────────────────────────

/// A replay of an accepted release is refused and moves nothing.
#[test]
fn rejects_a_replayed_submission() {
    let fixture = funded_fixture();
    approve(&fixture, 0);
    fixture
        .escrow()
        .release_milestone(&fixture.escrow_id, &fixture.client, &0);
    let after_first = snapshot(&fixture);

    // The approval was consumed by the accepted release, so a replay is
    // rejected on the settled milestone rather than paying out twice.
    assert_contract_error(
        fixture
            .escrow()
            .try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
        Error::MilestoneAlreadyReleased,
    );
    assert_rejected(&fixture, &after_first);
    assert_eq!(
        after_first.contract.released_amount, MILESTONE_ONE,
        "the replay must not add to released_amount"
    );
}

#[test]
fn rejects_an_already_refunded_milestone() {
    let fixture = funded_fixture();
    let escrow = fixture.escrow();
    escrow.refund_unreleased_milestones(&fixture.escrow_id, &vec![&fixture.env, 1_u32]);
    let before = snapshot(&fixture);

    // No approval is recorded: a settled milestone is refused on its own state
    // before the release mode's quorum is ever consulted.
    assert_contract_error(
        escrow.try_release_milestone(&fixture.escrow_id, &fixture.client, &1),
        Error::AlreadyRefunded,
    );
    assert_rejected(&fixture, &before);
    assert_eq!(
        before.contract.refunded_amount, MILESTONE_TWO,
        "the refund itself is intact"
    );
    assert_eq!(before.contract.released_amount, 0);
}

#[test]
fn batch_rejects_a_milestone_that_is_already_released() {
    let fixture = funded_fixture();
    let escrow = fixture.escrow();
    approve(&fixture, 0);
    escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0);
    approve(&fixture, 1);
    let before = snapshot(&fixture);

    assert_contract_error(
        escrow.try_release_milestone_batch(
            &fixture.escrow_id,
            &fixture.client,
            &vec![&fixture.env, 0_u32, 1_u32],
        ),
        Error::MilestoneAlreadyReleased,
    );
    assert_rejected(&fixture, &before);
    assert!(
        !snapshot(&fixture).milestones.get(1).unwrap().released,
        "the valid target of a rejected batch must stay unreleased"
    );
}

// ── Atomicity, retry, and determinism ───────────────────────────────────────

/// The strongest atomicity case: a batch whose second target is out of range
/// releases neither target, even though the first was valid and approved.
#[test]
fn a_rejected_batch_leaves_every_target_untouched() {
    let fixture = funded_fixture();
    approve(&fixture, 0);
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture.escrow().try_release_milestone_batch(
            &fixture.escrow_id,
            &fixture.client,
            &vec![&fixture.env, 0_u32, 7_u32],
        ),
        Error::IndexOutOfBounds,
    );
    assert_rejected(&fixture, &before);
    assert!(
        !snapshot(&fixture).milestones.get(0).unwrap().released,
        "the valid target of a rejected batch must stay unreleased"
    );
}

/// A batch is all-or-nothing on an unapproved target as well.
#[test]
fn a_rejected_batch_rolls_back_the_approved_targets() {
    let fixture = funded_fixture();
    approve(&fixture, 0);
    let before = snapshot(&fixture);

    assert_contract_error(
        fixture.escrow().try_release_milestone_batch(
            &fixture.escrow_id,
            &fixture.client,
            &vec![&fixture.env, 0_u32, 2_u32],
        ),
        Error::InsufficientApprovals,
    );
    assert_rejected(&fixture, &before);
}

/// A rejection is not sticky: the identical request succeeds once the blocking
/// condition is removed.
#[test]
fn a_retry_after_the_blocking_condition_is_removed_succeeds() {
    let fixture = funded_fixture();
    let escrow = fixture.escrow();
    let request = vec![&fixture.env, 0_u32, 2_u32];

    assert_contract_error(
        escrow.try_release_milestone_batch(&fixture.escrow_id, &fixture.client, &request),
        Error::InsufficientApprovals,
    );

    approve(&fixture, 0);
    approve(&fixture, 2);
    assert!(escrow.release_milestone_batch(&fixture.escrow_id, &fixture.client, &request));

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.released_amount, MILESTONE_ONE + MILESTONE_THREE);
    let milestones = escrow.get_milestones(&fixture.escrow_id);
    assert!(milestones.get(0).unwrap().released);
    assert!(
        !milestones.get(1).unwrap().released,
        "the untouched target stays outstanding"
    );
    assert!(milestones.get(2).unwrap().released);
}

/// The same rejected request produces the same error every time.
#[test]
fn a_repeated_rejection_is_deterministic() {
    let fixture = funded_fixture();
    let escrow = fixture.escrow();
    let before = snapshot(&fixture);

    for _ in 0..3 {
        assert_contract_error(
            escrow.try_release_milestone_batch(
                &fixture.escrow_id,
                &fixture.client,
                &vec![&fixture.env, 1_u32, 1_u32],
            ),
            Error::DuplicateMilestoneInBatch,
        );
    }
    assert_rejected(&fixture, &before);
}

/// Releasing a subset, then the rest, ends in the same terminal state as one
/// batch covering the whole schedule.
#[test]
fn sequential_releases_settle_the_whole_schedule() {
    let fixture = funded_fixture();
    let escrow = fixture.escrow();
    approve_all(&fixture);

    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0));
    assert!(escrow.release_milestone_batch(
        &fixture.escrow_id,
        &fixture.client,
        &vec![&fixture.env, 1_u32, 2_u32],
    ));

    let after = snapshot(&fixture);
    assert_eq!(after.contract.status, ContractStatus::Completed);
    assert_eq!(after.contract.released_amount, after.contract.funded_amount);
    assert_eq!(after.contract.refunded_amount, 0);
    assert_eq!(after.escrow_balance, 0);
    assert!(after.milestones.iter().all(|m| m.released));
    super::assert_escrow_invariants(&escrow, &fixture.escrow_id);
}
