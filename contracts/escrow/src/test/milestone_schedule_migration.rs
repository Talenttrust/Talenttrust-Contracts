//! Migration and default-value tests for `MilestoneSchedule`.
//!
//! These tests focus on the migration path:
//!   - contracts created before the schedule feature have no schedule entries
//!   - `migrate_milestone_schedules` writes default `None` entries idempotently
//!   - existing entries are never overwritten by a second migration run
//!   - `set_milestone_schedule` validates deadlines and immutability

use super::{create_contract, register_client};
use crate::{EscrowError, MilestoneSchedule};
use soroban_sdk::Env;

// ── helpers ──────────────────────────────────────────────────────────────────

fn future(env: &Env, offset_secs: u64) -> u64 {
    env.ledger().timestamp() + offset_secs
}

// ── default-value tests ───────────────────────────────────────────────────────

/// Before migration, `get_milestone_schedule` returns `None` for every index.
#[test]
fn get_schedule_returns_none_before_migration() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    assert!(client.get_milestone_schedule(&contract_id, &0).is_none());
    assert!(client.get_milestone_schedule(&contract_id, &1).is_none());
    assert!(client.get_milestone_schedule(&contract_id, &2).is_none());
}

/// After migration, every milestone has a schedule entry with both date fields `None`.
#[test]
fn migrate_writes_default_none_entries_for_all_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let migrated = client.migrate_milestone_schedules(&contract_id);
    assert_eq!(migrated, 3); // create_contract uses 3 milestones

    for idx in 0u32..3 {
        let sched = client
            .get_milestone_schedule(&contract_id, &idx)
            .expect("schedule entry should exist after migration");
        assert!(sched.deadline.is_none());
        assert!(sched.expected_delivery.is_none());
    }
}

// ── idempotency tests ─────────────────────────────────────────────────────────

/// A second call to `migrate_milestone_schedules` returns 0 (nothing new written).
#[test]
fn migrate_is_idempotent_returns_zero_on_second_call() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    assert_eq!(client.migrate_milestone_schedules(&contract_id), 3);
    assert_eq!(client.migrate_milestone_schedules(&contract_id), 0);
}

/// Migration does not overwrite a schedule entry that was set before migration ran.
#[test]
fn migrate_does_not_overwrite_existing_schedule_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    // Set a schedule on milestone 0 before running migration.
    let dl = future(&env, 86_400);
    client.set_milestone_schedule(
        &contract_id,
        &0,
        &MilestoneSchedule {
            deadline: Some(dl),
            expected_delivery: None,
            updated_at: 0,
        },
    );

    // Migration should skip milestone 0 (already has an entry) and write 2 defaults.
    let migrated = client.migrate_milestone_schedules(&contract_id);
    assert_eq!(migrated, 2);

    // Milestone 0's deadline must be preserved.
    let s0 = client.get_milestone_schedule(&contract_id, &0).unwrap();
    assert_eq!(s0.deadline, Some(dl));

    // Milestones 1 and 2 got default entries.
    let s1 = client.get_milestone_schedule(&contract_id, &1).unwrap();
    assert!(s1.deadline.is_none());
}

// ── set_milestone_schedule validation ────────────────────────────────────────

/// A deadline strictly in the future is accepted.
#[test]
fn set_schedule_accepts_future_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let dl = future(&env, 3_600);
    assert!(client.set_milestone_schedule(
        &contract_id,
        &0,
        &MilestoneSchedule {
            deadline: Some(dl),
            expected_delivery: None,
            updated_at: 0,
        },
    ));

    let stored = client.get_milestone_schedule(&contract_id, &0).unwrap();
    assert_eq!(stored.deadline, Some(dl));
    // Contract stamps updated_at; caller-supplied value is ignored.
    assert_ne!(stored.updated_at, 0);
}

/// A deadline equal to the current ledger timestamp is rejected.
#[test]
#[should_panic]
fn set_schedule_rejects_deadline_at_present() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let now = env.ledger().timestamp();
    client.set_milestone_schedule(
        &contract_id,
        &0,
        &MilestoneSchedule {
            deadline: Some(now),
            expected_delivery: None,
            updated_at: 0,
        },
    );
}

/// A deadline in the past is rejected.
#[test]
#[should_panic]
fn set_schedule_rejects_past_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let now = env.ledger().timestamp();
    let past = if now > 0 { now - 1 } else { 0 };
    client.set_milestone_schedule(
        &contract_id,
        &0,
        &MilestoneSchedule {
            deadline: Some(past),
            expected_delivery: None,
            updated_at: 0,
        },
    );
}

/// An out-of-range milestone index is rejected.
#[test]
#[should_panic]
fn set_schedule_rejects_out_of_range_index() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    client.set_milestone_schedule(
        &contract_id,
        &99,
        &MilestoneSchedule {
            deadline: Some(future(&env, 1_000)),
            expected_delivery: None,
            updated_at: 0,
        },
    );
}

/// Schedule update on a released milestone is rejected.
#[test]
#[should_panic]
fn set_schedule_rejects_update_after_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    // Fund and release milestone 0.
    client.deposit_funds(&contract_id, &super::total_milestone_amount());
    client.release_milestone(&contract_id, &0);

    // Attempt to set a schedule on the released milestone.
    client.set_milestone_schedule(
        &contract_id,
        &0,
        &MilestoneSchedule {
            deadline: Some(future(&env, 1_000)),
            expected_delivery: None,
            updated_at: 0,
        },
    );
}

/// Both `deadline` and `expected_delivery` can be set when both are in the future
/// and `deadline >= expected_delivery`.
#[test]
fn set_schedule_accepts_both_fields_when_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let ed = future(&env, 3_600);
    let dl = future(&env, 7_200); // deadline after expected_delivery

    assert!(client.set_milestone_schedule(
        &contract_id,
        &0,
        &MilestoneSchedule {
            deadline: Some(dl),
            expected_delivery: Some(ed),
            updated_at: 0,
        },
    ));

    let stored = client.get_milestone_schedule(&contract_id, &0).unwrap();
    assert_eq!(stored.deadline, Some(dl));
    assert_eq!(stored.expected_delivery, Some(ed));
}

/// `deadline` < `expected_delivery` is rejected.
#[test]
#[should_panic]
fn set_schedule_rejects_deadline_before_expected_delivery() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_, _, contract_id) = create_contract(&env, &client);

    let dl = future(&env, 3_600);
    let ed = future(&env, 7_200); // expected_delivery after deadline — invalid

    client.set_milestone_schedule(
        &contract_id,
        &0,
        &MilestoneSchedule {
            deadline: Some(dl),
            expected_delivery: Some(ed),
            updated_at: 0,
        },
    );
}
