//! Lightweight resource-baseline smoke tests for the escrow hot paths.
//!
//! These tests use conservative ceilings that reflect the Soroban simulator's
//! cost model and are intended as a quick sanity check.  For the full
//! parametric budget suite (typical vs. max-load, all entrypoints), see
//! [`super::budget`].

use super::{EscrowFixture, MILESTONE_ONE, MILESTONE_THREE, MILESTONE_TWO};
use soroban_sdk::{token::StellarAssetClient, vec, Env};

// ---------------------------------------------------------------------------
// Shared resource helpers (duplicated from budget.rs to keep modules independent)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Baseline {
    max_instructions: i64,
    max_mem_bytes: i64,
    max_read_entries: u32,
    max_write_entries: u32,
    max_read_bytes: u32,
    max_write_bytes: u32,
    max_fee_total: i64,
}

fn measure(env: &Env) -> (i64, i64, u32, u32, u32, u32, i64) {
    let r = env.cost_estimate().resources();
    let f = env.cost_estimate().fee();
    (
        r.instructions,
        r.mem_bytes,
        r.read_entries,
        r.write_entries,
        r.read_bytes,
        r.write_bytes,
        f.total,
    )
}

fn assert_baseline(label: &str, baseline: Baseline, env: &Env) {
    let (instr, mem, re, we, rb, wb, fee) = measure(env);
    assert!(
        instr <= baseline.max_instructions,
        "[perf] {} instruction regression: {} > {}",
        label,
        instr,
        baseline.max_instructions
    );
    assert!(
        mem <= baseline.max_mem_bytes,
        "[perf] {} memory regression: {} > {}",
        label,
        mem,
        baseline.max_mem_bytes
    );
    assert!(
        re <= baseline.max_read_entries,
        "[perf] {} read-entry regression: {} > {}",
        label,
        re,
        baseline.max_read_entries
    );
    assert!(
        we <= baseline.max_write_entries,
        "[perf] {} write-entry regression: {} > {}",
        label,
        we,
        baseline.max_write_entries
    );
    assert!(
        rb <= baseline.max_read_bytes,
        "[perf] {} read-byte regression: {} > {}",
        label,
        rb,
        baseline.max_read_bytes
    );
    assert!(
        wb <= baseline.max_write_bytes,
        "[perf] {} write-byte regression: {} > {}",
        label,
        wb,
        baseline.max_write_bytes
    );
    assert!(
        fee <= baseline.max_fee_total,
        "[perf] {} fee regression: {} > {}",
        label,
        fee,
        baseline.max_fee_total
    );
}

// ---------------------------------------------------------------------------
// Baselines (3× headroom over measured values)
// ---------------------------------------------------------------------------

const CREATE_BASELINE: Baseline = Baseline {
    max_instructions: 30_000_000,
    max_mem_bytes: 3_000_000,
    max_read_entries: 12,
    max_write_entries: 9,
    max_read_bytes: 24_576,
    max_write_bytes: 49_152,
    max_fee_total: 6_000_000,
};

const DEPOSIT_BASELINE: Baseline = Baseline {
    max_instructions: 30_000_000,
    max_mem_bytes: 3_000_000,
    max_read_entries: 12,
    max_write_entries: 6,
    max_read_bytes: 24_576,
    max_write_bytes: 32_768,
    max_fee_total: 6_000_000,
};

const RELEASE_BASELINE: Baseline = Baseline {
    max_instructions: 30_000_000,
    max_mem_bytes: 3_000_000,
    max_read_entries: 12,
    max_write_entries: 9,
    max_read_bytes: 24_576,
    max_write_bytes: 49_152,
    max_fee_total: 6_000_000,
};

const CANCEL_BASELINE: Baseline = Baseline {
    max_instructions: 30_000_000,
    max_mem_bytes: 3_000_000,
    max_read_entries: 12,
    max_write_entries: 6,
    max_read_bytes: 24_576,
    max_write_bytes: 32_768,
    max_fee_total: 6_000_000,
};

const REFUND_BASELINE: Baseline = Baseline {
    max_instructions: 30_000_000,
    max_mem_bytes: 3_000_000,
    max_read_entries: 12,
    max_write_entries: 9,
    max_read_bytes: 24_576,
    max_write_bytes: 49_152,
    max_fee_total: 6_000_000,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn perf_create_contract_resource_baseline() {
    let fixture = EscrowFixture::builder().build();
    let escrow = fixture.escrow();

    escrow.create_contract(
        &fixture.client,
        &fixture.freelancer,
        &None,
        &vec![&fixture.env, MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE],
        &crate::ReleaseAuthorization::ClientOnly,
    );

    assert_baseline("create_contract", CREATE_BASELINE, &fixture.env);
}

#[test]
fn perf_deposit_funds_resource_baseline() {
    let fixture = EscrowFixture::builder().with_settlement_token().build();
    let escrow = fixture.escrow();
    let total = fixture.total_amount();
    let token = fixture.settlement_token.as_ref().unwrap();
    soroban_sdk::token::StellarAssetClient::new(&fixture.env, token).mint(&fixture.client, &total);

    escrow.deposit_funds(&fixture.escrow_id, &fixture.client, &total);

    assert_baseline("deposit_funds", DEPOSIT_BASELINE, &fixture.env);
}

#[test]
fn perf_release_milestone_resource_baseline() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0, &0);

    assert_baseline("release_milestone", RELEASE_BASELINE, &fixture.env);
}

#[test]
fn perf_cancel_contract_resource_baseline() {
    // Cancel on an unfunded contract (no SAC transfer, cheapest cancel path).
    let fixture = EscrowFixture::builder().build();
    let escrow = fixture.escrow();

    escrow.cancel_contract(&fixture.escrow_id, &fixture.client);

    assert_baseline("cancel_contract", CANCEL_BASELINE, &fixture.env);
}

#[test]
fn perf_refund_unreleased_milestones_resource_baseline() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();

    escrow.refund_unreleased_milestones(&fixture.escrow_id, &vec![&fixture.env, 0_u32, 1, 2]);

    assert_baseline(
        "refund_unreleased_milestones",
        REFUND_BASELINE,
        &fixture.env,
    );
}
