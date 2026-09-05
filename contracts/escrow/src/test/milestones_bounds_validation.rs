use super::{assert_contract_error, EscrowFixture};
use crate::{milestones_consts::{MAX_WORK_EVIDENCE_BYTES, MIN_WORK_EVIDENCE_BYTES}, EscrowError, Error};
use soroban_sdk::{String, Vec};

#[test]
fn test_release_milestone_bounds() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let env = &fixture.env;

    let total_milestones = 3;
    // Exactly last valid index -> Ok (after approvals)
    assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &(total_milestones - 1)));
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &(total_milestones - 1, &0)));

    // Out of bounds by 1 -> IndexOutOfBounds
    assert_contract_error(
        escrow.try_approve_milestone_release(&fixture.escrow_id, &fixture.client, &total_milestones),
        EscrowError::IndexOutOfBounds,
    );
    assert_contract_error(
        escrow.try_release_milestone(&fixture.escrow_id, &fixture.client, &total_milestones, &0),
        EscrowError::IndexOutOfBounds,
    );
}

#[test]
fn test_refund_unreleased_milestones_bounds() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let env = &fixture.env;

    // Empty vector -> EmptyRefundRequest
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &fixture.client, &Vec::new(env)),
        EscrowError::EmptyRefundRequest,
    );

    // Duplicate indices -> DuplicateMilestoneInRefund
    let mut dup = Vec::new(env);
    dup.push_back(0);
    dup.push_back(0);
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &fixture.client, &dup),
        EscrowError::DuplicateMilestoneInRefund,
    );

    // Out of bounds single index -> IndexOutOfBounds
    let mut oob = Vec::new(env);
    oob.push_back(3); // only 3 milestones, index 3 is out of bounds
    assert_contract_error(
        escrow.try_refund_unreleased_milestones(&fixture.escrow_id, &fixture.client, &oob),
        EscrowError::IndexOutOfBounds,
    );

    // Valid single index -> ok
    let mut valid = Vec::new(env);
    valid.push_back(1); // unreleased index
    let refunded = escrow.refund_unreleased_milestones(&fixture.escrow_id, &fixture.client, &valid);
    assert!(refunded > 0);
}

#[test]
fn test_submit_work_evidence_bounds() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    let env = &fixture.env;

    // Exact min -> ok
    let min_str_buf = alloc::vec![b'a'; MIN_WORK_EVIDENCE_BYTES as usize];
    let min_evidence = String::from_utf8(env, min_str_buf.as_slice());
    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &min_evidence));

    // Zero length -> EmptyEvidence
    let empty_evidence = String::from_utf8(env, b"");
    assert_contract_error(
        escrow.try_submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &empty_evidence),
        Error::EmptyEvidence,
    );

    // One above max -> EvidenceTooLong
    let over_str_buf = alloc::vec![b'a'; (MAX_WORK_EVIDENCE_BYTES + 1) as usize];
    let over_evidence = String::from_utf8(env, over_str_buf.as_slice());
    assert_contract_error(
        escrow.try_submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &0, &over_evidence),
        Error::EvidenceTooLong,
    );
    
    // Exact max -> ok (for another milestone to avoid already submitted/released)
    let max_str_buf = alloc::vec![b'a'; MAX_WORK_EVIDENCE_BYTES as usize];
    let max_evidence = String::from_utf8(env, max_str_buf.as_slice());
    assert!(escrow.submit_work_evidence(&fixture.escrow_id, &fixture.freelancer, &1, &max_evidence));
}

#[test]
fn test_read_methods_index_out_of_bounds() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    // Out of bounds (3 milestones, index 3)
    let idx = 3;

    assert_contract_error(
        escrow.try_get_milestone(&fixture.escrow_id, &idx),
        Error::IndexOutOfBounds,
    );

    assert_contract_error(
        escrow.try_get_milestone_approvals(&fixture.escrow_id, &idx),
        Error::IndexOutOfBounds,
    );

    assert_contract_error(
        escrow.try_get_approval_deadline(&fixture.escrow_id, &idx),
        Error::IndexOutOfBounds,
    );

    assert_contract_error(
        escrow.try_get_work_evidence(&fixture.escrow_id, &idx),
        Error::IndexOutOfBounds,
    );
}
