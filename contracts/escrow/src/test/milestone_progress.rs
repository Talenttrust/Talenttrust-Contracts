use super::{register_client, EscrowFixture};
use crate::MilestoneProgress;

// ── unknown contract ─────────────────────────────────────────────────────────

/// Unknown contract id returns MilestoneProgress { completed: 0, total: 0 } rather than panicking.
#[test]
fn get_milestone_progress_returns_zero_for_unknown_contract() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let progress = client.get_milestone_progress(&999);
    assert_eq!(
        progress,
        MilestoneProgress {
            completed: 0,
            total: 0
        }
    );
}

/// Zero id (never allocated) also returns MilestoneProgress { completed: 0, total: 0 }.
#[test]
fn get_milestone_progress_returns_zero_for_zero_id() {
    let env = soroban_sdk::Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let progress = client.get_milestone_progress(&0);
    assert_eq!(
        progress,
        MilestoneProgress {
            completed: 0,
            total: 0
        }
    );
}

// ── none complete ────────────────────────────────────────────────────────────

/// Freshly created, unreleased contract: none of its milestones are complete.
#[test]
fn get_milestone_progress_none_complete() {
    let fixture = EscrowFixture::builder().build();
    let escrow = fixture.escrow();

    let progress = escrow.get_milestone_progress(&fixture.escrow_id);
    assert_eq!(
        progress,
        MilestoneProgress {
            completed: 0,
            total: 3
        }
    );
}

// ── some complete ────────────────────────────────────────────────────────────

/// One of several milestones released: progress reflects the partial state.
#[test]
fn get_milestone_progress_some_complete() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0, &0));

    let progress = escrow.get_milestone_progress(&fixture.escrow_id);
    assert_eq!(
        progress,
        MilestoneProgress {
            completed: 1,
            total: 3
        }
    );
}

// ── all complete ─────────────────────────────────────────────────────────────

/// Fully completed contract: completed count equals total.
#[test]
fn get_milestone_progress_all_complete() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    for milestone_index in 0..3u32 {
        escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &milestone_index);
        assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &milestone_index, &0));
    }

    let progress = escrow.get_milestone_progress(&fixture.escrow_id);
    assert_eq!(
        progress,
        MilestoneProgress {
            completed: 3,
            total: 3
        }
    );
}

// ── purity ───────────────────────────────────────────────────────────────────

/// Repeated reads don't change the result.
#[test]
fn get_milestone_progress_observations_are_pure() {
    let fixture = EscrowFixture::builder().funded().build();
    let escrow = fixture.escrow();
    escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0);
    assert!(escrow.release_milestone(&fixture.escrow_id, &fixture.client, &0, &0));

    let initial = escrow.get_milestone_progress(&fixture.escrow_id);
    for _ in 0..8 {
        assert_eq!(escrow.get_milestone_progress(&fixture.escrow_id), initial);
    }
}
