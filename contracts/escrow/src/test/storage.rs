use super::{
    assert_contract_error, complete_contract, create_contract, default_milestones,
    generated_participants, register_client, total_milestone_amount, MILESTONE_ONE, MILESTONE_THREE,
    MILESTONE_TWO,
};
use crate::{ContractStatus, DataKey, EscrowError, ReadinessChecklist, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, Address, Env};

// ─── Initialized / Admin ──────────────────────────────────────────────────────

#[test]
fn initialized_written_on_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);

    assert!(client.initialize(&admin));

    env.as_contract(&client.address, || {
        let v: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Initialized)
            .unwrap();
        assert!(v);
    });
}

#[test]
fn admin_written_on_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    env.as_contract(&client.address, || {
        let stored: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        assert_eq!(stored, admin);
    });
}

#[test]
fn double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);
    assert_contract_error(
        client.try_initialize(&admin),
        EscrowError::AlreadyInitialized,
    );
}

// ─── Paused ───────────────────────────────────────────────────────────────────

#[test]
fn paused_written_by_pause_and_cleared_by_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.pause();
    env.as_contract(&client.address, || {
        let v: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        assert!(v);
    });

    client.unpause();
    env.as_contract(&client.address, || {
        let v: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        assert!(!v);
    });
}

#[test]
fn paused_blocks_create_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();

    let (c, f) = generated_participants(&env);
    assert_contract_error(
        client.try_create_contract(
            &c,
            &f,
            &None,
            &default_milestones(&env),
            &ReleaseAuthorization::ClientOnly,
        ),
        EscrowError::ContractPaused,
    );
}

#[test]
fn paused_blocks_deposit_funds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (client_addr, _, id) = create_contract(&env, &client);
    client.pause();

    assert_contract_error(
        client.try_deposit_funds(&id, &client_addr, &total_milestone_amount()),
        EscrowError::ContractPaused,
    );
}

#[test]
fn paused_blocks_release_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (client_addr, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &client_addr, &total_milestone_amount());
    client.pause();

    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        EscrowError::ContractPaused,
    );
}

#[test]
fn paused_blocks_cancel_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (client_addr, _, id) = create_contract(&env, &client);
    client.pause();

    assert_contract_error(
        client.try_cancel_contract(&id, &client_addr),
        EscrowError::ContractPaused,
    );
}

#[test]
fn read_only_queries_not_blocked_by_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let (_, _, id) = create_contract(&env, &client);
    client.pause();

    let record = client.get_contract(&id);
    assert_eq!(record.status, ContractStatus::Created);
    assert!(client.is_paused());
}

// ─── Emergency ────────────────────────────────────────────────────────────────

#[test]
fn emergency_written_by_activate_and_cleared_by_resolve() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.activate_emergency_pause();
    env.as_contract(&client.address, || {
        let v: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Emergency)
            .unwrap_or(false);
        assert!(v);
    });

    client.resolve_emergency();
    env.as_contract(&client.address, || {
        let v: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Emergency)
            .unwrap_or(false);
        assert!(!v);
    });
}

#[test]
fn unpause_blocked_while_emergency_active() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.activate_emergency_pause();
    assert_contract_error(client.try_unpause(), EscrowError::EmergencyActive);
}

// ─── Contract / NextContractId ────────────────────────────────────────────────

#[test]
fn contract_written_on_create_and_readable() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (c, f) = generated_participants(&env);

    let id = client.create_contract(
        &c,
        &f,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    let record = client.get_contract(&id);
    assert_eq!(record.client, c);
    assert_eq!(record.freelancer, f);
    assert_eq!(record.status, ContractStatus::Created);
}

#[test]
fn next_contract_id_increments_per_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, id1) = create_contract(&env, &client);
    let (_, _, id2) = create_contract(&env, &client);
    assert_eq!(id2, id1 + 1);
}

#[test]
fn get_contract_fails_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    assert_contract_error(
        client.try_get_contract(&9999),
        EscrowError::ContractNotFound,
    );
}

// ─── Milestone released flag (milestone vector) ───────────────────────────────

/// `release_milestone` sets `ms.released = true` in the persisted milestone
/// vector. There is no separate `DataKey::MilestoneReleased` storage key; the
/// vector is the single source of truth for released state.
#[test]
fn milestone_released_flag_set_in_vector_on_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &client_addr, &total_milestone_amount());
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);

    let milestones = client.get_milestones(&id);
    assert!(milestones.get(0).unwrap().released, "index 0 must be released");
    assert!(!milestones.get(1).unwrap().released, "index 1 must not be released");
    assert!(!milestones.get(2).unwrap().released, "index 2 must not be released");
}

#[test]
fn double_release_same_milestone_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &client_addr, &total_milestone_amount());
    client.release_milestone(&id, &client_addr, &0);

    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &0),
        EscrowError::AlreadyReleased,
    );
}

#[test]
fn release_out_of_bounds_milestone_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &client_addr, &total_milestone_amount());

    assert_contract_error(
        client.try_release_milestone(&id, &client_addr, &99),
        EscrowError::InvalidMilestone,
    );
}

// ─── ReputationIssued / Reputation / PendingReputationCredits ─────────────────

#[test]
fn reputation_issued_written_and_reputation_updated() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (c, f, id) = complete_contract(&env, &client);
    client.issue_reputation(&id, &c, &f, &5);

    env.as_contract(&client.address, || {
        let issued: bool = env
            .storage()
            .persistent()
            .get(&DataKey::ReputationIssued(id))
            .unwrap_or(false);
        assert!(issued);
    });

    let rep = client.get_reputation(&f).unwrap();
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
    assert_eq!(rep.last_rating, 5);
}

#[test]
fn double_issue_reputation_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (c, f, id) = complete_contract(&env, &client);
    client.issue_reputation(&id, &c, &f, &4);

    assert_contract_error(
        client.try_issue_reputation(&id, &c, &f, &4),
        EscrowError::ReputationAlreadyIssued,
    );
}

#[test]
fn pending_reputation_credits_incremented_on_completion() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, f, _) = complete_contract(&env, &client);
    assert_eq!(client.get_pending_reputation_credits(&f), 1);
}

#[test]
fn pending_reputation_credits_decremented_on_issue() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (c, f, id) = complete_contract(&env, &client);
    assert_eq!(client.get_pending_reputation_credits(&f), 1);

    client.issue_reputation(&id, &c, &f, &3);
    assert_eq!(client.get_pending_reputation_credits(&f), 0);
}

#[test]
fn reputation_not_issuable_before_completion() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (c, f) = generated_participants(&env);
    let id = client.create_contract(
        &c,
        &f,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert_contract_error(
        client.try_issue_reputation(&id, &c, &f, &5),
        EscrowError::NotCompleted,
    );
}

#[test]
fn reputation_requires_client_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (c, f, id) = complete_contract(&env, &client);
    let stranger = Address::generate(&env);

    assert_contract_error(
        client.try_issue_reputation(&id, &stranger, &f, &5),
        EscrowError::UnauthorizedRole,
    );
}

// ─── ReadinessChecklist ───────────────────────────────────────────────────────

#[test]
fn readiness_checklist_initialized_flag_set_by_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    env.as_contract(&client.address, || {
        let checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap();
        assert!(checklist.initialized);
        assert!(!checklist.governed_params_set);
    });
}

#[test]
fn readiness_checklist_emergency_flag_set_by_activate() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.activate_emergency_pause();

    env.as_contract(&client.address, || {
        let checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap();
        assert!(checklist.emergency_controls_enabled);
    });
}

// ─── Accounting invariant ─────────────────────────────────────────────────────

#[test]
fn released_amount_tracks_milestone_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &client_addr, &total_milestone_amount());

    client.release_milestone(&id, &client_addr, &0);
    let r = client.get_contract(&id);
    assert_eq!(r.released_amount, MILESTONE_ONE);

    client.release_milestone(&id, &client_addr, &1);
    let r = client.get_contract(&id);
    assert_eq!(r.released_amount, MILESTONE_ONE + MILESTONE_TWO);

    client.release_milestone(&id, &client_addr, &2);
    let r = client.get_contract(&id);
    assert_eq!(r.released_amount, total_milestone_amount());
    assert_eq!(r.status, ContractStatus::Completed);
}

// ─── get_milestone single-index reader (issue #649) ───────────────────────────

#[test]
fn get_milestone_index_zero_returns_first_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    // Default contract has three milestones: ONE, TWO, THREE.
    let (_client_addr, _, id) = create_contract(&env, &client);

    let m = client
        .get_milestone(&id, &0u32)
        .expect("index 0 is in bounds");
    assert_eq!(m.amount, MILESTONE_ONE);
    // It must match the entry returned by the full-vector reader.
    assert_eq!(m, client.get_milestones(&id).get(0).unwrap());
}

#[test]
fn get_milestone_last_valid_index_returns_last_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _, id) = create_contract(&env, &client);

    let milestones = client.get_milestones(&id);
    let last = milestones.len() - 1;
    let m = client
        .get_milestone(&id, &last)
        .expect("last index is in bounds");
    assert_eq!(m.amount, MILESTONE_THREE);
    assert_eq!(m, milestones.get(last).unwrap());
}

#[test]
fn get_milestone_out_of_bounds_returns_none() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _, id) = create_contract(&env, &client);

    let len = client.get_milestones(&id).len();
    // One past the last valid index must return None, not panic.
    assert!(client.get_milestone(&id, &len).is_none());
    assert!(client.get_milestone(&id, &(len + 5)).is_none());
}

#[test]
fn get_milestone_unknown_contract_panics_contract_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    // No contract has been created; id 999 was never allocated.
    assert_contract_error(
        client.try_get_milestone(&999u32, &0u32),
        EscrowError::ContractNotFound,
    );
}

#[test]
fn deposit_exceeding_total_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _, id) = create_contract(&env, &client);
    assert_contract_error(
        client.try_deposit_funds(&id, &client_addr, &(total_milestone_amount() + 1)),
        EscrowError::ExactDepositRequired,
    );
}
