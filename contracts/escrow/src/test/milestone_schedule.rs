//! # Milestone Schedule Metadata — Test Suite
//!
//! Covers every validation path, storage operation, and edge-case for the
//! [`MilestoneSchedule`] feature introduced in `contracts-13`.
//!
//! ## Test organisation
//!
//! | Section | What is tested |
//! |---------|---------------|
//! | `valid_*` | Happy-path creation and retrieval |
//! | `error_due_date_*` | Due-date validation rejections |
//! | `error_monotonic_*` | Monotonicity enforcement |
//! | `error_string_*` | Length-bound enforcement |
//! | `error_immutable_*` | Post-release immutability |
//! | `set_schedule_*` | `set_milestone_schedule` mutations |
//! | `integration_*` | End-to-end flows with schedule metadata |

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env, String, Vec};

use crate::{
    Escrow, EscrowClient, MilestoneSchedule, ReleaseAuthorization,
    MAX_SCHEDULE_DESCRIPTION_LEN, MAX_SCHEDULE_TITLE_LEN,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Register the contract and return a client.
fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

/// Generate a client/freelancer address pair.
fn participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

/// A two-milestone amount vector (100 + 200 = 300 stroops total).
fn two_milestones(env: &Env) -> Vec<i128> {
    vec![env, 100_i128, 200_i128]
}

/// A three-milestone amount vector (100 + 200 + 300 = 600 stroops).
fn three_milestones(env: &Env) -> Vec<i128> {
    vec![env, 100_i128, 200_i128, 300_i128]
}

/// Returns a future ledger timestamp offset by `offset_secs` from now.
fn future(env: &Env, offset_secs: u64) -> u64 {
    env.ledger().timestamp() + offset_secs
}

/// Build a `Vec<Option<MilestoneSchedule>>` of `n` `None` entries.
fn no_schedules(env: &Env, n: u32) -> Vec<Option<MilestoneSchedule>> {
    let mut v: Vec<Option<MilestoneSchedule>> = Vec::new(env);
    for _ in 0..n {
        v.push_back(None);
    }
    v
}

/// Build a minimal schedule with only a `due_date`.
fn dated_schedule(env: &Env, due: u64) -> MilestoneSchedule {
    MilestoneSchedule {
        due_date: Some(due),
        title: None,
        description: None,
        updated_at: 0, // overwritten by contract
    }
}

/// Build a fully-populated schedule entry.
fn full_schedule(env: &Env, due: u64, title: &str, desc: &str) -> MilestoneSchedule {
    MilestoneSchedule {
        due_date: Some(due),
        title: Some(String::from_str(env, title)),
        description: Some(String::from_str(env, desc)),
        updated_at: 0,
    }
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

/// A contract can be created with no schedule metadata (empty `schedules` vec).
#[test]
fn valid_create_without_schedules() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &two_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &Vec::new(&env),
    );

    // No schedule data should be stored.
    assert!(client.get_milestone_schedule(&id, &0).is_none());
    assert!(client.get_milestone_schedule(&id, &1).is_none());
}

/// A contract can be created with partial schedule metadata (some `None` entries).
#[test]
fn valid_create_with_partial_schedules() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due = future(&env, 86_400); // 1 day ahead
    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, due)));
    scheds.push_back(None);

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &two_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );

    let stored = client.get_milestone_schedule(&id, &0).expect("schedule should exist");
    assert_eq!(stored.due_date, Some(due));
    assert!(client.get_milestone_schedule(&id, &1).is_none());
}

/// All milestones can carry full schedule metadata.
#[test]
fn valid_create_with_all_schedules_populated() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due0 = future(&env, 100_000);
    let due1 = future(&env, 200_000);
    let due2 = future(&env, 300_000);

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(full_schedule(&env, due0, "Phase 1", "Initial deliverable")));
    scheds.push_back(Some(full_schedule(&env, due1, "Phase 2", "Mid-point review")));
    scheds.push_back(Some(full_schedule(&env, due2, "Phase 3", "Final delivery")));

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &three_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );

    for (idx, expected_due) in [(0u32, due0), (1, due1), (2, due2)] {
        let s = client
            .get_milestone_schedule(&id, &idx)
            .expect("schedule should be stored");
        assert_eq!(s.due_date, Some(expected_due));
    }
}

/// `updated_at` is stamped with the current ledger timestamp, not the caller value.
#[test]
fn valid_updated_at_is_stamped_by_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due = future(&env, 50_000);
    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(MilestoneSchedule {
        due_date: Some(due),
        title: None,
        description: None,
        updated_at: 999_999, // caller-supplied value must be overwritten
    }));

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );

    let stored = client.get_milestone_schedule(&id, &0).unwrap();
    // The contract stamps `updated_at` from `env.ledger().timestamp()`.
    assert_eq!(stored.updated_at, env.ledger().timestamp());
    assert_ne!(stored.updated_at, 999_999);
}

/// `get_milestone_schedule` returns `None` for a non-existent index.
#[test]
fn valid_get_schedule_returns_none_for_missing_index() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &Vec::new(&env),
    );

    assert!(client.get_milestone_schedule(&id, &99).is_none());
}

// ---------------------------------------------------------------------------
// Due-date validation
// ---------------------------------------------------------------------------

/// A due date equal to the current ledger timestamp is rejected.
#[test]
#[should_panic(expected = "invalid schedule metadata")]
fn error_due_date_at_present_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let now = env.ledger().timestamp();
    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, now))); // equal to now — invalid

    client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );
}

/// A due date in the past is rejected.
#[test]
#[should_panic(expected = "invalid schedule metadata")]
fn error_due_date_in_past_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let now = env.ledger().timestamp();
    let past = if now > 1 { now - 1 } else { 0 };
    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, past)));

    client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );
}

/// A due date of `u64::MAX` (far future) is accepted.
#[test]
fn valid_due_date_max_u64_is_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, u64::MAX)));

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );

    let stored = client.get_milestone_schedule(&id, &0).unwrap();
    assert_eq!(stored.due_date, Some(u64::MAX));
}

// ---------------------------------------------------------------------------
// Monotonicity enforcement
// ---------------------------------------------------------------------------

/// Equal due dates across adjacent milestones are rejected.
#[test]
#[should_panic(expected = "strictly increasing")]
fn error_monotonic_equal_dates_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due = future(&env, 100_000);
    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, due)));
    scheds.push_back(Some(dated_schedule(&env, due))); // same — invalid

    client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &two_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );
}

/// A later milestone with an earlier due date is rejected.
#[test]
#[should_panic(expected = "strictly increasing")]
fn error_monotonic_decreasing_dates_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due0 = future(&env, 200_000);
    let due1 = future(&env, 100_000); // earlier than due0 — invalid

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, due0)));
    scheds.push_back(Some(dated_schedule(&env, due1)));

    client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &two_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );
}

/// Milestones without a `due_date` are transparently skipped in the
/// monotonicity check; surrounding dated milestones must still be ordered.
#[test]
fn valid_monotonic_skips_undated_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due0 = future(&env, 100_000);
    let due2 = future(&env, 300_000); // milestone 1 has no date — gap is OK

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, due0)));
    scheds.push_back(None);
    scheds.push_back(Some(dated_schedule(&env, due2)));

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &three_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );

    assert!(client.get_milestone_schedule(&id, &0).is_some());
    assert!(client.get_milestone_schedule(&id, &1).is_none());
    assert!(client.get_milestone_schedule(&id, &2).is_some());
}

// ---------------------------------------------------------------------------
// String-length enforcement
// ---------------------------------------------------------------------------

/// A `title` exactly at the length limit is accepted.
#[test]
fn valid_title_at_max_length_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    // Build a string of exactly MAX_SCHEDULE_TITLE_LEN bytes.
    let title_bytes = "a".repeat(MAX_SCHEDULE_TITLE_LEN as usize);
    let title_str = String::from_str(&env, &title_bytes);

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(MilestoneSchedule {
        due_date: Some(future(&env, 1_000)),
        title: Some(title_str),
        description: None,
        updated_at: 0,
    }));

    // Should not panic.
    client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );
}

/// A `title` one byte over the limit is rejected.
#[test]
#[should_panic(expected = "invalid schedule metadata")]
fn error_title_exceeds_max_length_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let title_bytes = "a".repeat(MAX_SCHEDULE_TITLE_LEN as usize + 1);
    let title_str = String::from_str(&env, &title_bytes);

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(MilestoneSchedule {
        due_date: Some(future(&env, 1_000)),
        title: Some(title_str),
        description: None,
        updated_at: 0,
    }));

    client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );
}

/// A `description` one byte over the limit is rejected.
#[test]
#[should_panic(expected = "invalid schedule metadata")]
fn error_description_exceeds_max_length_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let desc_bytes = "x".repeat(MAX_SCHEDULE_DESCRIPTION_LEN as usize + 1);
    let desc_str = String::from_str(&env, &desc_bytes);

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(MilestoneSchedule {
        due_date: Some(future(&env, 1_000)),
        title: None,
        description: Some(desc_str),
        updated_at: 0,
    }));

    client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );
}

// ---------------------------------------------------------------------------
// `set_milestone_schedule` — mutation after creation
// ---------------------------------------------------------------------------

/// The client can update a schedule entry before the milestone is released.
#[test]
fn set_schedule_client_can_update_before_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &Vec::new(&env),
    );

    let new_due = future(&env, 50_000);
    let new_sched = full_schedule(&env, new_due, "Updated title", "Updated desc");

    assert!(client.set_milestone_schedule(&id, &0, &new_sched));

    let stored = client.get_milestone_schedule(&id, &0).expect("should exist after set");
    assert_eq!(stored.due_date, Some(new_due));
}

/// A schedule update is rejected when the milestone has already been released.
#[test]
#[should_panic(expected = "immutable after milestone release")]
fn error_immutable_set_schedule_after_release_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
        &Vec::new(&env),
    );

    client.deposit_funds(&id, &c, &300_i128);
    client.approve_milestone_release(&id, &c, &0);
    client.release_milestone(&id, &c, &0);

    // Now attempt to update the released milestone's schedule.
    let sched = dated_schedule(&env, future(&env, 10_000));
    client.set_milestone_schedule(&id, &0, &sched);
}

/// An update that violates monotonicity with the next milestone is rejected.
#[test]
#[should_panic(expected = "strictly increasing")]
fn error_set_schedule_violates_monotonicity_with_next() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due0 = future(&env, 100_000);
    let due1 = future(&env, 200_000);

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, due0)));
    scheds.push_back(Some(dated_schedule(&env, due1)));

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &two_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );

    // Try to set milestone 0's due date AFTER milestone 1's — should fail.
    let bad_sched = dated_schedule(&env, future(&env, 300_000)); // > due1
    client.set_milestone_schedule(&id, &0, &bad_sched);
}

/// An out-of-range milestone index is rejected.
#[test]
#[should_panic(expected = "milestone index out of range")]
fn error_set_schedule_out_of_range_index_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &Vec::new(&env),
    );

    let sched = dated_schedule(&env, future(&env, 10_000));
    client.set_milestone_schedule(&id, &99, &sched);
}

/// Schedules vector length mismatch is rejected.
#[test]
#[should_panic(expected = "schedules length must match milestone_amounts length")]
fn error_schedules_length_mismatch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    // 2 milestones but 1 schedule entry — mismatch.
    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, future(&env, 10_000))));

    client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &two_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

/// Full contract lifecycle with schedule metadata: create → deposit → approve
/// → release all milestones → verify schedules survive unchanged.
#[test]
fn integration_full_lifecycle_preserves_schedule_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due0 = future(&env, 100_000);
    let due1 = future(&env, 200_000);

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(full_schedule(&env, due0, "M1", "First milestone")));
    scheds.push_back(Some(full_schedule(&env, due1, "M2", "Second milestone")));

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &two_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );

    // Fund, approve, and release both milestones.
    client.deposit_funds(&id, &c, &300_i128);
    client.approve_milestone_release(&id, &c, &0);
    client.release_milestone(&id, &c, &0);
    client.approve_milestone_release(&id, &c, &1);
    client.release_milestone(&id, &c, &1);

    // Schedule metadata must still be readable after release.
    let s0 = client.get_milestone_schedule(&id, &0).unwrap();
    let s1 = client.get_milestone_schedule(&id, &1).unwrap();
    assert_eq!(s0.due_date, Some(due0));
    assert_eq!(s1.due_date, Some(due1));
}

/// Two independent contracts each carry their own isolated schedule state.
#[test]
fn integration_schedule_isolation_across_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due_a = future(&env, 100_000);
    let due_b = future(&env, 500_000);

    let mut scheds_a: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds_a.push_back(Some(dated_schedule(&env, due_a)));

    let mut scheds_b: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds_b.push_back(Some(dated_schedule(&env, due_b)));

    let id_a = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
        &scheds_a,
    );
    let id_b = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &vec![&env, 200_i128],
        &ReleaseAuthorization::ClientOnly,
        &scheds_b,
    );

    let sa = client.get_milestone_schedule(&id_a, &0).unwrap();
    let sb = client.get_milestone_schedule(&id_b, &0).unwrap();

    assert_eq!(sa.due_date, Some(due_a));
    assert_eq!(sb.due_date, Some(due_b));
    assert_ne!(sa.due_date, sb.due_date);
}

/// `set_milestone_schedule` correctly updates an existing entry without
/// disturbing other milestones in the same contract.
#[test]
fn integration_set_schedule_does_not_disturb_other_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = participants(&env);

    let due0 = future(&env, 100_000);
    let due1 = future(&env, 200_000);

    let mut scheds: Vec<Option<MilestoneSchedule>> = Vec::new(&env);
    scheds.push_back(Some(dated_schedule(&env, due0)));
    scheds.push_back(Some(dated_schedule(&env, due1)));

    let id = client.create_contract(
        &c,
        &f,
        &None::<Address>,
        &two_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
        &scheds,
    );

    // Update only milestone 0; milestone 1 must remain unchanged.
    let updated_due = future(&env, 150_000); // between due0 and due1
    client.set_milestone_schedule(&id, &0, &dated_schedule(&env, updated_due));

    let s0 = client.get_milestone_schedule(&id, &0).unwrap();
    let s1 = client.get_milestone_schedule(&id, &1).unwrap();

    assert_eq!(s0.due_date, Some(updated_due));
    assert_eq!(s1.due_date, Some(due1)); // untouched
}