//! Resource-budget regression tests for the milestone entrypoints.
//!
//! These tests use the Soroban test budget API (`Env::cost_estimate()`) to pin
//! down CPU-instruction, memory, storage, and fee ceilings for the milestone
//! lifecycle: approval, release, refund, overdue checks, and milestone reads.
//! Each assertion compares the *last* root invocation's measured cost against a
//! fixed baseline with headroom, so an unexpected regression in any milestone
//! path fails the suite instead of silently shipping.
//!
//! Two shapes are covered per issue guidance:
//! - a typical, small (3-milestone) contract, and
//! - a large, bounded input at `MAX_MILESTONES` (10 milestones), so the
//!   duplicate-index scan in `refund_unreleased_milestones` and the
//!   completion scan in `release_milestone` are exercised at their worst case.
//!
//! None of the measured paths come close to Soroban's network-enforced
//! per-transaction instruction ceiling (order of 100M); see the module doc
//! for headroom notes on each baseline.

use super::EscrowFixture;
use crate::{Escrow, EscrowClient, MAX_MILESTONES};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

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

// Typical shape: a small (3-milestone) contract, acted on one milestone at a
// time. Ceilings carry roughly 35-40% headroom over the measured cost on the
// commit this test was written against (see PR description for raw numbers).
const APPROVE_MILESTONE_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 160_000,
    max_mem_bytes: 30_000,
    max_read_entries: 7,
    max_write_entries: 2,
    max_read_bytes: 2_048,
    max_write_bytes: 512,
    max_fee_total: 200_000,
};

const RELEASE_MILESTONE_TYPICAL_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 620_000,
    max_mem_bytes: 110_000,
    max_read_entries: 12,
    max_write_entries: 7,
    max_read_bytes: 4_096,
    max_write_bytes: 2_560,
    max_fee_total: 2_700_000,
};

const REFUND_SINGLE_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 480_000,
    max_mem_bytes: 85_000,
    max_read_entries: 8,
    max_write_entries: 6,
    max_read_bytes: 4_096,
    max_write_bytes: 2_560,
    max_fee_total: 1_900_000,
};

const IS_MILESTONE_OVERDUE_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 85_000,
    max_mem_bytes: 12_000,
    max_read_entries: 4,
    max_write_entries: 1,
    max_read_bytes: 2_048,
    max_write_bytes: 0,
    max_fee_total: 30_000,
};

const GET_MILESTONES_TYPICAL_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 65_000,
    max_mem_bytes: 10_000,
    max_read_entries: 3,
    max_write_entries: 1,
    max_read_bytes: 1_536,
    max_write_bytes: 0,
    max_fee_total: 20_000,
};

// Large-input shape: MAX_MILESTONES (10) milestones. `refund_unreleased_milestones`
// runs an O(n^2) duplicate-index scan and `release_milestone` scans the full
// milestone vector to detect contract completion, so both are expected to cost
// more than the typical 1-3 milestone case above; the point of these baselines
// is to bound how much more, not to forbid growth outright.
const CREATE_CONTRACT_MAX_MILESTONES_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 300_000,
    max_mem_bytes: 60_000,
    max_read_entries: 6,
    max_write_entries: 5,
    max_read_bytes: 512,
    max_write_bytes: 4_096,
    max_fee_total: 2_200_000,
};

const RELEASE_MILESTONE_MAX_MILESTONES_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 850_000,
    max_mem_bytes: 155_000,
    max_read_entries: 11,
    max_write_entries: 8,
    max_read_bytes: 6_144,
    max_write_bytes: 4_864,
    max_fee_total: 2_000_000,
};

const REFUND_ALL_MAX_MILESTONES_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 750_000,
    max_mem_bytes: 120_000,
    max_read_entries: 8,
    max_write_entries: 6,
    max_read_bytes: 6_144,
    max_write_bytes: 4_608,
    max_fee_total: 1_900_000,
};

const GET_MILESTONES_MAX_MILESTONES_BASELINE: ResourceBaseline = ResourceBaseline {
    max_instructions: 110_000,
    max_mem_bytes: 16_000,
    max_read_entries: 3,
    max_write_entries: 1,
    max_read_bytes: 4_096,
    max_write_bytes: 0,
    max_fee_total: 25_000,
};

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

/// Build `count` equal-sized (100 token) milestone amounts.
fn milestones_of_len(env: &Env, count: u32) -> Vec<i128> {
    let mut milestones = Vec::new(env);
    for _ in 0..count {
        milestones.push_back(100_0000000_i128);
    }
    milestones
}

/// Build a funded fixture with `count` equal-sized milestones.
fn funded_fixture_with_milestone_count(count: u32) -> EscrowFixture {
    let builder = EscrowFixture::builder();
    let milestones = milestones_of_len(builder.env(), count);
    builder.with_milestones(milestones).funded().build()
}

#[test]
fn approve_milestone_release_resource_baseline() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let _ = escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "approve_milestone_release (typical)",
        resources,
        fee_total,
        APPROVE_MILESTONE_BASELINE,
    );
}

#[test]
fn release_milestone_resource_baseline() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);

    let _ = escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "release_milestone (typical)",
        resources,
        fee_total,
        RELEASE_MILESTONE_TYPICAL_BASELINE,
    );
}

#[test]
fn refund_unreleased_milestones_single_resource_baseline() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let indices = soroban_sdk::vec![&fixture.env, 0u32];
    let _ = escrow.refund_unreleased_milestones(&fixture.escrow_id, &indices);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "refund_unreleased_milestones (single index)",
        resources,
        fee_total,
        REFUND_SINGLE_BASELINE,
    );
}

#[test]
fn is_milestone_overdue_resource_baseline() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let _ = escrow.is_milestone_overdue(&fixture.escrow_id, &0);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "is_milestone_overdue (typical)",
        resources,
        fee_total,
        IS_MILESTONE_OVERDUE_BASELINE,
    );
}

#[test]
fn get_milestones_resource_baseline() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    let _ = escrow.get_milestones(&fixture.escrow_id);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "get_milestones (typical, 3 milestones)",
        resources,
        fee_total,
        GET_MILESTONES_TYPICAL_BASELINE,
    );
}

#[test]
fn create_contract_at_max_milestones_resource_baseline() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);

    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = milestones_of_len(&env, MAX_MILESTONES);

    let _ = escrow.create_contract(
        &client,
        &freelancer,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );

    let (resources, fee_total) = measure_last_invocation(&env);
    assert_within_baseline(
        "create_contract (large input, MAX_MILESTONES)",
        resources,
        fee_total,
        CREATE_CONTRACT_MAX_MILESTONES_BASELINE,
    );
}

#[test]
fn release_last_of_max_milestones_resource_baseline() {
    let fixture = funded_fixture_with_milestone_count(MAX_MILESTONES);
    let escrow = fixture.escrow();

    // Release every milestone but the last so the final release's completion
    // scan (`milestones.iter().all(...)`) walks the full, worst-case vector.
    for index in 0..(MAX_MILESTONES - 1) {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &index);
        escrow.release_milestone(&fixture.escrow_id, &fixture.client, &index);
    }
    let last_index = MAX_MILESTONES - 1;
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &last_index);

    let _ = escrow.release_milestone(&fixture.escrow_id, &fixture.client, &last_index);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "release_milestone (large input, completing MAX_MILESTONES)",
        resources,
        fee_total,
        RELEASE_MILESTONE_MAX_MILESTONES_BASELINE,
    );
}

#[test]
fn refund_all_max_milestones_resource_baseline() {
    let fixture = funded_fixture_with_milestone_count(MAX_MILESTONES);
    let escrow = fixture.escrow();

    let mut indices: Vec<u32> = Vec::new(&fixture.env);
    for i in 0..MAX_MILESTONES {
        indices.push_back(i);
    }

    let _ = escrow.refund_unreleased_milestones(&fixture.escrow_id, &indices);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "refund_unreleased_milestones (large input, all MAX_MILESTONES indices)",
        resources,
        fee_total,
        REFUND_ALL_MAX_MILESTONES_BASELINE,
    );
}

#[test]
fn get_milestones_at_max_milestones_resource_baseline() {
    let fixture = funded_fixture_with_milestone_count(MAX_MILESTONES);
    let escrow = fixture.escrow();

    let _ = escrow.get_milestones(&fixture.escrow_id);

    let (resources, fee_total) = measure_last_invocation(&fixture.env);
    assert_within_baseline(
        "get_milestones (large input, MAX_MILESTONES)",
        resources,
        fee_total,
        GET_MILESTONES_MAX_MILESTONES_BASELINE,
    );
}
