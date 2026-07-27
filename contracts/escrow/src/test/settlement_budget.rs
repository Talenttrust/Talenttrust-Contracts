use super::{create_contract, register_client, EscrowFixture, MILESTONE_ONE};
use crate::{ContractStatus, EscrowClient, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, Env, Vec};

const RELEASE_MILESTONE_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 10_000_000,
    max_mem_bytes: 1_000_000,
    max_read_entries: 4,
    max_write_entries: 3,
    max_read_bytes: 4_096,
    max_write_bytes: 14_336,
    max_fee_total: 2_100_000,
};

const REFUND_ALL_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 10_000_000,
    max_mem_bytes: 1_000_000,
    max_read_entries: 4,
    max_write_entries: 3,
    max_read_bytes: 4_096,
    max_write_bytes: 12_288,
    max_fee_total: 2_000_000,
};

#[derive(Clone, Copy)]
struct ResourceBaseline {
    max_instructions: i64,
    max_mem_bytes: i64,
    max_read_entries: u32,
    max_write_entries: u32,
    max_read_bytes: u32,
    max_write_bytes: u32,
    max_fee_total: i64,
}

#[derive(Clone, Copy)]
struct MeasuredResources {
    instructions: i64,
    mem_bytes: i64,
    read_entries: u32,
    write_entries: u32,
    read_bytes: u32,
    write_bytes: u32,
}

fn measure_last_invocation(env: &Env) -> (MeasuredResources, i64) {
    let resources = env.cost_estimate().resources();
    let fee = env.cost_estimate().fee();

    (
        MeasuredResources {
            instructions: resources.instructions,
            mem_bytes: resources.mem_bytes,
            read_entries: resources.read_entries,
            write_entries: resources.write_entries,
            read_bytes: resources.read_bytes,
            write_bytes: resources.write_bytes,
        },
        fee.total,
    )
}

fn assert_within_baseline(
    label: &str,
    resources: MeasuredResources,
    fee_total: i64,
    baseline: ResourceBaseline,
) {
    assert!(
        resources.instructions <= baseline.max_instructions,
        "{} instruction regression: {} > {}",
        label,
        resources.instructions,
        baseline.max_instructions
    );
    assert!(
        resources.mem_bytes <= baseline.max_mem_bytes,
        "{} memory regression: {} > {}",
        label,
        resources.mem_bytes,
        baseline.max_mem_bytes
    );
    assert!(
        resources.read_entries <= baseline.max_read_entries,
        "{} read-entry regression: {} > {}",
        label,
        resources.read_entries,
        baseline.max_read_entries
    );
    assert!(
        resources.write_entries <= baseline.max_write_entries,
        "{} write-entry regression: {} > {}",
        label,
        resources.write_entries,
        baseline.max_write_entries
    );
    assert!(
        resources.read_bytes <= baseline.max_read_bytes,
        "{} read-byte regression: {} > {}",
        label,
        resources.read_bytes,
        baseline.max_read_bytes
    );
    assert!(
        resources.write_bytes <= baseline.max_write_bytes,
        "{} write-byte regression: {} > {}",
        label,
        resources.write_bytes,
        baseline.max_write_bytes
    );
    assert!(
        fee_total <= baseline.max_fee_total,
        "{} fee regression: {} > {}",
        label,
        fee_total,
        baseline.max_fee_total
    );
}

/// Typical release_milestone call stays within the resource budget for standard-sized inputs.
#[test]
fn release_milestone_stays_within_budget() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "release_milestone",
        resources,
        fee_total,
        RELEASE_MILESTONE_BASELINE,
    );
}

/// A large funded release of all milestones is bounded and does not regress.
#[test]
fn release_all_milestones_bounded() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    for index in 0..3_u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &index);
        escrow.release_milestone(&fixture.escrow_id, &fixture.client, &index);
    }

    let contract = escrow.get_contract(&fixture.escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "release_all_milestones",
        resources,
        fee_total,
        RELEASE_MILESTONE_BASELINE,
    );
}

/// Typical refund_unreleased_milestones call stays within the resource budget for standard-sized inputs.
#[test]
fn refund_unreleased_stays_within_budget() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.refund_unreleased_milestones(&fixture.escrow_id, &vec![&fixture.env, 0]);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "refund_unreleased_milestones",
        resources,
        fee_total,
        REFUND_ALL_BASELINE,
    );
}

/// Refund of all unreleased milestones is bounded and does not regress.
#[test]
fn refund_all_unreleased_bounded() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let indices: Vec<u32> = vec![&fixture.env, 0, 1, 2];
    escrow.refund_unreleased_milestones(&fixture.escrow_id, &indices);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "refund_all_unreleased",
        resources,
        fee_total,
        REFUND_ALL_BASELINE,
    );
}
