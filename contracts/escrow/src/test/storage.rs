use super::{
    assert_contract_error, complete_contract, create_contract, default_milestones,
    generated_participants, register_client, total_milestone_amount, MILESTONE_ONE, MILESTONE_TWO,
};
use crate::{ContractStatus, DataKey, EscrowError, ReadinessChecklist};
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
            &default_milestones(&env),
            &crate::types::DepositMode::ExactTotal,
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

    let (_, _, id) = create_contract(&env, &client);
    client.pause();

    assert_contract_error(
        client.try_deposit_funds(&id, &total_milestone_amount()),
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

    let (_, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &total_milestone_amount());
    client.pause();

    assert_contract_error(
        client.try_release_milestone(&id, &0),
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

    let (c, _, id) = create_contract(&env, &client);
    client.pause();

    assert_contract_error(
        client.try_cancel_contract(&id, &c),
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
        &default_milestones(&env),
        &crate::types::DepositMode::ExactTotal,
    );

    let record = client.get_contract(&id);
    assert_eq!(record.client, c);
    assert_eq!(record.freelancer, f);
    assert_eq!(record.status, ContractStatus::Created);
    assert_eq!(record.total_deposited, 0);
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

// ─── MilestoneReleased ────────────────────────────────────────────────────────

#[test]
fn milestone_released_written_on_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &total_milestone_amount());
    client.release_milestone(&id, &0);

    env.as_contract(&client.address, || {
        let v: bool = env
            .storage()
            .persistent()
            .get(&DataKey::MilestoneReleased(id, 0))
            .unwrap_or(false);
        assert!(v);
        let v1: bool = env
            .storage()
            .persistent()
            .get(&DataKey::MilestoneReleased(id, 1))
            .unwrap_or(false);
        assert!(!v1);
    });
}

#[test]
fn double_release_same_milestone_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &total_milestone_amount());
    client.release_milestone(&id, &0);

    assert_contract_error(
        client.try_release_milestone(&id, &0),
        EscrowError::AlreadyReleased,
    );
}

#[test]
fn release_out_of_bounds_milestone_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &total_milestone_amount());

    assert_contract_error(
        client.try_release_milestone(&id, &99),
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
        &default_milestones(&env),
        &crate::types::DepositMode::ExactTotal,
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

    let (_, _, id) = create_contract(&env, &client);
    client.deposit_funds(&id, &total_milestone_amount());

    client.release_milestone(&id, &0);
    let r = client.get_contract(&id);
    assert_eq!(r.released_amount, MILESTONE_ONE);

    client.release_milestone(&id, &1);
    let r = client.get_contract(&id);
    assert_eq!(r.released_amount, MILESTONE_ONE + MILESTONE_TWO);

    client.release_milestone(&id, &2);
    let r = client.get_contract(&id);
    assert_eq!(r.released_amount, total_milestone_amount());
    assert_eq!(r.status, ContractStatus::Completed);
}

#[test]
fn deposit_exceeding_total_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_, _, id) = create_contract(&env, &client);
    assert_contract_error(
        client.try_deposit_funds(&id, &(total_milestone_amount() + 1)),
        EscrowError::ExactDepositRequired,
    );
}

// ─── NextContractId overflow / collision ─────────────────────────────────────

fn assert_error<T>(
    result: Result<
        Result<T, soroban_sdk::ConversionError>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
    expected: crate::Error,
) {
    match result {
        Err(Ok(e)) => {
            let expected_err: soroban_sdk::Error = expected.into();
            assert_eq!(e, expected_err);
        }
        other => panic!("expected {:?}, got {:?}", expected, other),
    }
}

#[test]
fn next_contract_id_overflow_at_u32_max() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client_addr, freelancer_addr, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &u32::MAX);
    });

    let result = escrow.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    assert_error(result, crate::Error::ContractIdOverflow);

    env.as_contract(&escrow.address, || {
        let next: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap();
        assert_eq!(next, u32::MAX);
    });
}

#[test]
fn next_contract_id_rejects_occupied_slot() {
    let env = Env::default();
    env.mock_all_auths();
    let escrow = register_client(&env);
    let (client_addr, freelancer_addr, _) = generated_participants(&env);
    let milestones = default_milestones(&env);

    let existing_id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );

    env.as_contract(&escrow.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &existing_id);
    });

    let intruder = Address::generate(&env);
    let result = escrow.try_create_contract(
        &intruder,
        &freelancer_addr,
        &None,
        &milestones,
        &crate::ReleaseAuthorization::ClientOnly,
    );
    assert_error(result, crate::Error::ContractIdCollision);
}
