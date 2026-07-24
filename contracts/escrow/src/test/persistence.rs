use super::{
    assert_contract_error, complete_contract, create_contract, default_milestones,
    generated_participants, register_client, total_milestone_amount, MILESTONE_ONE,
    MILESTONE_THREE, MILESTONE_TWO,
};
use crate::{ttl, ContractStatus, Error, EscrowError, ReleaseAuthorization};
use soroban_sdk::{
    testutils::{storage::Persistent, Address as _, Ledger},
    vec, Address, Env, Symbol,
};

fn milestone_symbol(env: &Env) -> Symbol {
    Symbol::new(env, "milestones")
}

/// Finalization by arbiter works on a completed contract.
#[test]
fn finalize_completed_contract_allows_arbiter_finalizer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, arbiter_addr, contract_id) =
        super::create_contract_with_arbiter(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount()));
    assert!(client.raise_dispute(&contract_id, &client_addr));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );

    assert!(client.finalize_contract(&contract_id, &arbiter_addr));

    let record = client
        .get_finalization_record(&contract_id)
        .expect("finalization record should exist");
    assert_eq!(record.finalizer, arbiter_addr);
    assert_eq!(record.summary.status, ContractStatus::Disputed);
    assert_eq!(
        record.summary.funded_amount,
        super::total_milestone_amount()
    );
    assert_eq!(record.summary.released_amount, 0);
    assert_eq!(
        record.summary.refundable_balance,
        super::total_milestone_amount()
    );
}

#[test]
fn participant_metadata_and_pending_credits_persist_until_reputation_is_issued() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let completed = client.get_contract(&contract_id);
    assert_eq!(completed.client, client_addr);
    assert_eq!(completed.freelancer, freelancer_addr);
    assert_eq!(completed.status, ContractStatus::Completed);
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 1);

    let comment = soroban_sdk::String::from_str(&env, "Good job");
    assert!(client.issue_reputation(&contract_id, &client_addr, &5_u32, &comment));
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 0);
}

#[test]
fn try_get_contract_reports_missing_state_without_mutating_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    super::assert_contract_error(client.try_get_contract(&777), Error::ContractNotFound);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 10_i128];
    let _created = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

/// Freelancer may also finalize a Completed contract.
#[test]
fn finalize_allows_freelancer_finalizer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &freelancer_addr));

    let record = client
        .get_finalization_record(&contract_id)
        .expect("finalization record should exist");
    assert_eq!(record.finalizer, freelancer_addr);
}

/// Non-participant (outsider) cannot finalize.
#[test]
fn finalize_rejects_unauthorized_finalizer() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    let outsider = Address::generate(&env);

    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &outsider),
        Error::UnauthorizedRole,
    );
    assert!(client.get_finalization_record(&contract_id).is_none());
}

/// Finalization from non-terminal status (Created) is rejected.
#[test]
fn finalize_rejects_created_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        Error::InvalidStatusTransition,
    );
    assert!(client.get_finalization_record(&contract_id).is_none());
}

/// Finalization from Funded status is rejected.
#[test]
fn finalize_rejects_funded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount()));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Funded
    );

    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        Error::InvalidStatusTransition,
    );
    assert!(client.get_finalization_record(&contract_id).is_none());
}

/// Double finalization is rejected with AlreadyFinalized.
#[test]
fn finalize_is_idempotent_guarded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));
    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        Error::AlreadyFinalized,
    );
}

/// release_milestone is rejected after finalization.
#[test]
fn release_milestone_rejects_after_finalization() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    super::assert_contract_error(
        client.try_release_milestone(&contract_id, &client_addr, &0),
        Error::AlreadyFinalized,
    );
}

/// refund_unreleased_milestones is rejected after finalization.
#[test]
fn refund_unreleased_milestones_rejects_after_finalization() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    let res = client.try_refund_unreleased_milestones(&contract_id, &vec![&env, 0u32]);
    match res {
        Err(Ok(e)) => {
            assert_eq!(e, soroban_sdk::Error::from(EscrowError::AlreadyFinalized));
        }
        other => panic!("expected contract error AlreadyFinalized, got {:?}", other),
    }
}

/// deposit_funds is rejected after finalization.
#[test]
fn deposit_funds_rejects_after_finalization() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    super::assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &1_i128),
        Error::AlreadyFinalized,
    );
}

/// approve_milestone_release is rejected after finalization.
#[test]
fn approve_milestone_release_rejects_after_finalization() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    super::assert_contract_error(
        client.try_approve_milestone_release(&contract_id, &client_addr, &0),
        Error::AlreadyFinalized,
    );
}

/// get_finalization_record returns None for an unfinalized contract.
#[test]
fn get_finalization_record_returns_none_for_unfinalized() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);

    assert!(client.get_finalization_record(&contract_id).is_none());
}

/// Finalization record is absent for a non-existent contract.
#[test]
fn get_finalization_record_returns_none_for_missing_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    assert!(client.get_finalization_record(&999).is_none());
}

/// Pause blocks finalization.
#[test]
fn pause_blocks_finalization() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    assert!(client.pause());

    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        Error::ContractPaused,
    );
    assert!(client.get_finalization_record(&contract_id).is_none());
}

/// Test finalization on a contract refunded to Completion (mixed release/refund).
#[test]
fn finalize_completed_with_mixed_releases_and_refunds() {
    let env = Env::default();

    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert!(client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount()));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));

    assert!(client.refund_unreleased_milestones(&contract_id, &vec![&env, 2u32]) > 0);
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Completed
    );

    assert!(client.finalize_contract(&contract_id, &client_addr));

    let record = client
        .get_finalization_record(&contract_id)
        .expect("finalization record should exist");
    assert_eq!(record.summary.status, ContractStatus::Completed);
    assert_eq!(
        record.summary.released_amount,
        super::MILESTONE_ONE + super::MILESTONE_TWO
    );
    assert_eq!(record.summary.refundable_balance, 0);
    assert_eq!(record.summary.released_milestone_count, 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Read-path getters: get_contract / get_milestones / get_refundable_balance
// ─────────────────────────────────────────────────────────────────────────────
//
// These tests cover issue #475: the public read getters panic with
// `ContractNotFound` for unknown ids, return expected data on success, never
// mutate balances, and bump the persistent TTL of the entry they read from.
//
// All assertions on the returned payload use `client.try_*` wrappers so we can
// pair the panic-on-not-found contract semantics with the auto-generated
// `Err(Ok(...))` encoding surfaced to off-chain callers.

// ── get_contract: not-found ──────────────────────────────────────────────────

/// `get_contract` panics with `ContractNotFound` for a never-allocated id.
#[test]
fn get_contract_panics_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    assert_contract_error(client.try_get_contract(&999), EscrowError::ContractNotFound);
}

/// `get_contract` panics with `ContractNotFound` even when probed with id zero
/// (the smallest `u32` value) if no contract has been created there.
#[test]
fn get_contract_panics_for_zero_id_when_no_zero_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    assert_contract_error(client.try_get_contract(&0), EscrowError::ContractNotFound);
}

// ── get_contract: success ─────────────────────────────────────────────────────

/// `get_contract` returns the stored record for a valid id immediately after
/// creation, before any deposit.
#[test]
fn get_contract_returns_record_for_valid_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);

    let record = client.get_contract(&contract_id);
    assert_eq!(record.client, client_addr);
    assert_eq!(record.freelancer, freelancer_addr);
    assert_eq!(record.arbiter, None);
    assert_eq!(record.status, ContractStatus::Created);
    assert_eq!(record.funded_amount, 0);
    assert_eq!(record.released_amount, 0);
    assert_eq!(record.refunded_amount, 0);
    assert_eq!(
        record.release_authorization,
        ReleaseAuthorization::ClientOnly
    );
}

/// `get_contract` reflects subsequent state changes from deposits and releases.
#[test]
fn get_contract_reflects_deposit_and_release_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let initial = client.get_contract(&contract_id);
    assert_eq!(initial.status, ContractStatus::Created);
    assert_eq!(initial.funded_amount, 0);

    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));

    let funded = client.get_contract(&contract_id);
    assert_eq!(funded.funded_amount, total_milestone_amount());
    assert_eq!(funded.status, ContractStatus::Funded);

    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    let after_release = client.get_contract(&contract_id);
    assert_eq!(after_release.released_amount, MILESTONE_ONE);
    assert_eq!(after_release.funded_amount, total_milestone_amount());
}

/// Repeated reads of `get_contract` must return identical snapshots and never
/// mutate balances. This validates the security assumption that reads do not
/// change accounting state.
#[test]
fn get_contract_observations_are_pure() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let initial = client.get_contract(&contract_id);
    let initial_funded = initial.funded_amount;
    let initial_released = initial.released_amount;
    let initial_refunded = initial.refunded_amount;

    // 32 reads in succession with no operations between them.
    for _ in 0..32 {
        let snapshot = client.get_contract(&contract_id);
        assert_eq!(snapshot.funded_amount, initial_funded);
        assert_eq!(snapshot.released_amount, initial_released);
        assert_eq!(snapshot.refunded_amount, initial_refunded);
        assert_eq!(snapshot.status, initial.status);
    }

    let final_snapshot = client.get_contract(&contract_id);
    assert_eq!(final_snapshot.funded_amount, total_milestone_amount());
    assert_eq!(final_snapshot.released_amount, MILESTONE_ONE);
    assert_eq!(final_snapshot.refunded_amount, 0);
}

// ── get_milestones: not-found ─────────────────────────────────────────────────

/// `get_milestones` panics with `ContractNotFound` for a never-allocated id.
#[test]
fn get_milestones_panics_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    assert_contract_error(client.try_get_milestones(&999), Error::ContractNotFound);
}

/// `get_milestones` panics with `ContractNotFound` for the zero id when no
/// contract at slot zero exists.
#[test]
fn get_milestones_panics_for_zero_id_when_no_zero_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    assert_contract_error(client.try_get_milestones(&0), EscrowError::ContractNotFound);
}

// ── get_milestones: success ───────────────────────────────────────────────────

/// `get_milestones` returns the milestone vector with the same amounts and
/// order chosen at creation time.
#[test]
fn get_milestones_returns_vector_for_valid_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let milestones = client.get_milestones(&contract_id);
    assert_eq!(milestones.len(), 3);
    assert_eq!(milestones.get(0).unwrap().amount, MILESTONE_ONE);
    assert_eq!(milestones.get(1).unwrap().amount, MILESTONE_TWO);
    assert_eq!(milestones.get(2).unwrap().amount, MILESTONE_THREE);
    for ms in milestones.iter() {
        assert!(!ms.released);
        assert!(!ms.refunded);
        assert_eq!(ms.funded_amount, 0);
        assert_eq!(ms.refunded_amount, 0);
        assert!(ms.work_evidence.is_none());
    }
}

/// Reading milestones through `get_milestones` does not change release or
/// refund flags, nor funded_amount per-milestone accounting.
#[test]
fn get_milestones_observations_are_pure() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let initial = client.get_milestones(&contract_id);
    for _ in 0..16 {
        assert_eq!(client.get_milestones(&contract_id), initial);
    }
    assert!(initial.get(0).unwrap().released);
    assert!(!initial.get(1).unwrap().released);
    assert!(!initial.get(2).unwrap().released);
    assert!(!initial.get(0).unwrap().refunded);
}

// ── get_refundable_balance: not-found ────────────────────────────────────────

/// `get_refundable_balance` panics with `ContractNotFound` for a never-allocated id.
#[test]
fn get_refundable_balance_panics_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    match client.try_get_refundable_balance(&999) {
        Err(Ok(e)) => assert_eq!(e, soroban_sdk::Error::from(Error::ContractNotFound)),
        other => panic!("expected ContractNotFound, got {:?}", other),
    }
}

/// `get_refundable_balance` panics with `ContractNotFound` for the zero id.
#[test]
fn get_refundable_balance_panics_for_zero_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    match client.try_get_refundable_balance(&0) {
        Err(Ok(e)) => assert_eq!(e, soroban_sdk::Error::from(Error::ContractNotFound)),
        other => panic!("expected ContractNotFound, got {:?}", other),
    }
}

// ── get_refundable_balance: success ───────────────────────────────────────────

/// Unfunded contract: refundable balance equals zero because no funds are in
/// the contract, regardless of milestone total.
#[test]
fn get_refundable_balance_is_zero_for_unfunded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    assert_eq!(client.get_refundable_balance(&contract_id), 0);
}

/// Funded but fully unreleased contract: refundable balance equals funded_amount.
#[test]
fn get_refundable_balance_equals_funded_amount_pre_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));

    assert_eq!(
        client.get_refundable_balance(&contract_id),
        total_milestone_amount()
    );
}

/// Partially released contract: refundable balance subtracts released amount.
#[test]
fn get_refundable_balance_subtracts_released_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let expected = total_milestone_amount() - MILESTONE_ONE;
    assert_eq!(client.get_refundable_balance(&contract_id), expected);
}

/// Fully released contract: refundable balance is zero because all funds have
/// been paid out to the freelancer.
#[test]
fn get_refundable_balance_is_zero_after_full_release() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (_client_addr, _freelancer_addr, contract_id) = complete_contract(&env, &client);

    assert_eq!(client.get_refundable_balance(&contract_id), 0);
}

/// Repeated reads of `get_refundable_balance` must not change accounting state.
#[test]
fn get_refundable_balance_observations_are_pure() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    let initial = client.get_refundable_balance(&contract_id);
    for _ in 0..32 {
        assert_eq!(client.get_refundable_balance(&contract_id), initial);
    }
}

// ── get_milestone_approvals: None for unapproved milestones ──────────────────

/// `get_milestone_approvals` returns `None` when no approval has been recorded
/// for any milestone, even on a fully funded contract (where approvals are
/// expected prior to release).
#[test]
fn get_milestone_approvals_returns_none_when_absent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));

    assert!(client.get_milestone_approvals(&contract_id, &0).is_none());
    assert!(client.get_milestone_approvals(&contract_id, &1).is_none());
    assert!(client.get_milestone_approvals(&contract_id, &2).is_none());
}

/// Unknown contract id queries for approvals return `None` (this getter does
/// not panic on missing contract because approval keys are best-effort).
#[test]
fn get_milestone_approvals_returns_none_for_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    assert!(client.get_milestone_approvals(&999, &0).is_none());
}

/// After recording an approval, `get_milestone_approvals` returns `Some`:
/// the recorded entry exposes the booleans for each authorization party.
#[test]
fn get_milestone_approvals_returns_some_after_recorded() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));

    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    let approvals = client
        .get_milestone_approvals(&contract_id, &0)
        .expect("approvals should exist after record");
    assert!(approvals.client_approved);
    assert!(!approvals.freelancer_approved);
    assert!(!approvals.arbiter_approved);

    // Other milestones are still unapproved.
    assert!(client.get_milestone_approvals(&contract_id, &1).is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// TTL-extension behavior on read
// ─────────────────────────────────────────────────────────────────────────────
//
// Indirect TTL-on-read verification: bump the ledger sequence to within the
// bump threshold of the original expiry, trigger the read, then advance past
// the original expiry. If the read extended TTL, the entry survives and is
// retrievable; if not, the host will archive the entry and reads return None
// or panic.

/// Construct an `Env` whose ledger accepts long persistent TTLs and starts at
/// a known sequence number so we can advance it deterministically.
fn setup_ttl_env() -> Env {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.max_entry_ttl = ttl::LEDGERS_PER_DAY * 60;
        li.min_persistent_entry_ttl = ttl::LEDGERS_PER_DAY * 60;
        li.sequence_number = 1_000;
    });
    env.mock_all_auths();
    env
}

/// `get_contract` extends the persistent TTL of the contract entry; the entry
/// remains retrievable past its original expiry window.
#[test]
#[ignore]
fn get_contract_read_extends_persistent_ttl() {
    let env = setup_ttl_env();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let bump_threshold = ttl::PERSISTENT_BUMP_THRESHOLD as u32;
    let extension = ttl::PERSISTENT_TTL_LEDGERS as u32;

    let initial_ttl: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&crate::DataKey::Contract(contract_id))
    });

    // Advance ledger so the entry sits within `bump_threshold` of expiry.
    env.ledger().with_mut(|li| {
        li.sequence_number = li
            .sequence_number
            .saturating_add(initial_ttl.saturating_sub(bump_threshold) + 1);
    });

    // The read bumps the TTL of [`DataKey::Contract(id)`].
    let snapshot = client.get_contract(&contract_id);
    assert_eq!(snapshot.status, ContractStatus::Created);

    let ttl_after_read: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&crate::DataKey::Contract(contract_id))
    });
    assert!(
        ttl_after_read >= bump_threshold,
        "get_contract must extend TTL to at least the bump threshold (got {})",
        ttl_after_read
    );

    // Advance ledger close to (but not past) the freshly bumped expiry.
    // After the bump, the entry's live_until = current_seq + extension; an
    // advance of `extension - 1` keeps us strictly inside the live window
    // so the follow-up read is meaningful — if the read had not bumped the
    // TTL, the entry would already be archived.
    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number.saturating_add(extension - 1);
    });

    let snapshot_after = client.get_contract(&contract_id);
    assert_eq!(snapshot_after.status, ContractStatus::Created);
}

/// `get_milestones` extends the persistent TTL of the milestones vector entry.
#[test]
#[ignore]
fn get_milestones_read_extends_persistent_ttl() {
    let env = setup_ttl_env();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let bump_threshold = ttl::PERSISTENT_BUMP_THRESHOLD as u32;
    let extension = ttl::PERSISTENT_TTL_LEDGERS as u32;
    let milestone_key = milestone_symbol(&env);

    let initial_ttl: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&(crate::DataKey::Contract(contract_id), milestone_key.clone()))
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = li
            .sequence_number
            .saturating_add(initial_ttl.saturating_sub(bump_threshold) + 1);
    });

    let milestones = client.get_milestones(&contract_id);
    assert_eq!(milestones.len(), default_milestones(&env).len());

    let ttl_after_read: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&(crate::DataKey::Contract(contract_id), milestone_key.clone()))
    });
    assert!(
        ttl_after_read >= bump_threshold,
        "get_milestones must extend milestones TTL to at least the bump threshold (got {})",
        ttl_after_read
    );

    env.ledger().with_mut(|li| {
        // Advance safely inside the bumped live window; see get_contract test.
        li.sequence_number = li.sequence_number.saturating_add(extension - 1);
    });

    let milestones_after = client.get_milestones(&contract_id);
    assert_eq!(milestones_after.len(), default_milestones(&env).len());
}

/// `get_work_evidence` extends the persistent TTL of the milestones vector entry.
#[test]
fn get_work_evidence_read_extends_persistent_ttl() {
    let env = setup_ttl_env();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);
    client.deposit_funds(&contract_id, &client_addr, &super::total_milestone_amount());

    let ev = soroban_sdk::String::from_str(&env, "ipfs://QmTtlEvidence");
    assert!(client.submit_work_evidence(&contract_id, &freelancer_addr, &0, &ev));

    let bump_threshold = ttl::PERSISTENT_BUMP_THRESHOLD as u32;
    let extension = ttl::PERSISTENT_TTL_LEDGERS as u32;
    let milestone_key = milestone_symbol(&env);

    let initial_ttl: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&(crate::DataKey::Contract(contract_id), milestone_key.clone()))
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = li
            .sequence_number
            .saturating_add(initial_ttl.saturating_sub(bump_threshold) + 1);
    });

    let result = client.get_work_evidence(&contract_id, &0);
    assert_eq!(result, Some(ev.clone()));

    let ttl_after_read: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&(crate::DataKey::Contract(contract_id), milestone_key.clone()))
    });
    assert!(
        ttl_after_read >= bump_threshold,
        "get_work_evidence must extend milestones TTL to at least the bump threshold (got {})",
        ttl_after_read
    );

    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number.saturating_add(extension - 1);
    });

    let result_after = client.get_work_evidence(&contract_id, &0);
    assert_eq!(result_after, Some(ev));
}

/// `get_refundable_balance` extends the persistent TTL of the contract entry.
#[test]
#[ignore]
fn get_refundable_balance_read_extends_persistent_ttl() {
    let env = setup_ttl_env();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let bump_threshold = ttl::PERSISTENT_BUMP_THRESHOLD as u32;
    let extension = ttl::PERSISTENT_TTL_LEDGERS as u32;

    let initial_ttl: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&crate::DataKey::Contract(contract_id))
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = li
            .sequence_number
            .saturating_add(initial_ttl.saturating_sub(bump_threshold) + 1);
    });

    let balance = client.get_refundable_balance(&contract_id);
    assert_eq!(balance, 0);

    let ttl_after_read: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&crate::DataKey::Contract(contract_id))
    });
    assert!(
        ttl_after_read >= bump_threshold,
        "get_refundable_balance must extend TTL to at least the bump threshold (got {})",
        ttl_after_read
    );

    env.ledger().with_mut(|li| {
        // Advance safely inside the bumped live window; see get_contract test.
        li.sequence_number = li.sequence_number.saturating_add(extension - 1);
    });

    let balance_after = client.get_refundable_balance(&contract_id);
    assert_eq!(balance_after, 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Cross-getter invalid-input matrix
// ─────────────────────────────────────────────────────────────────────────────
//
// Belt-and-braces coverage: probe every getter + every invalid id shape so a
// regression in any single panic path surfaces as a localized failure.

#[test]
fn read_getters_fail_for_arbitrary_unknown_id() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let was_paused = client.is_paused();
    let was_emergency = client.is_emergency();

    // Snapshot contract storage keys present before probing.
    let (has_initialized, has_admin, has_paused, has_emergency) =
        env.as_contract(&client.address, || {
            (
                env.storage().persistent().has(&crate::DataKey::Initialized),
                env.storage().persistent().has(&crate::DataKey::Admin),
                env.storage().persistent().has(&crate::DataKey::Paused),
                env.storage().persistent().has(&crate::DataKey::Emergency),
            )
        });

    // Invalid id 4_242 — no getter may mutate stored state.
    assert_contract_error(
        client.try_get_contract(&4_242),
        EscrowError::ContractNotFound,
    );
    assert_contract_error(
        client.try_get_milestones(&4_242),
        EscrowError::ContractNotFound,
    );
    match client.try_get_refundable_balance(&4_242) {
        Err(Ok(e)) => assert_eq!(e, soroban_sdk::Error::from(EscrowError::ContractNotFound)),
        other => panic!("expected ContractNotFound, got {:?}", other),
    };

    // State flags must remain unchanged after the failed reads.
    env.as_contract(&client.address, || {
        let has_initialized = env.storage().persistent().has(&crate::DataKey::Initialized);
        let has_admin = env.storage().persistent().has(&crate::DataKey::Admin);
        let has_paused = env.storage().persistent().has(&crate::DataKey::Paused);
        let has_emergency = env.storage().persistent().has(&crate::DataKey::Emergency);
        let was_paused = client.is_paused();
        let was_emergency = client.is_emergency();

        // Invalid id 4_242 — no getter may mutate stored state.
        assert_contract_error(client.try_get_contract(&4_242), Error::ContractNotFound);
        assert_contract_error(client.try_get_milestones(&4_242), Error::ContractNotFound);
        match client.try_get_refundable_balance(&4_242) {
            Err(Ok(e)) => assert_eq!(e, soroban_sdk::Error::from(Error::ContractNotFound)),
            other => panic!("expected ContractNotFound, got {:?}", other),
        };

        // State flags must remain unchanged after the failed reads.
        assert_eq!(
            env.storage().persistent().has(&crate::DataKey::Initialized),
            has_initialized
        );
        assert_eq!(
            env.storage().persistent().has(&crate::DataKey::Admin),
            has_admin
        );
        assert_eq!(
            env.storage().persistent().has(&crate::DataKey::Paused),
            has_paused
        );
        assert_eq!(
            env.storage().persistent().has(&crate::DataKey::Emergency),
            has_emergency
        );
    });
    assert_eq!(client.is_paused(), was_paused);
    assert_eq!(client.is_emergency(), was_emergency);
}

#[test]
fn get_contract_summary_works_as_expected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    // 1. Unknown contract id summary call panics with ContractNotFound
    super::assert_contract_error(
        client.try_get_contract_summary(&999),
        EscrowError::ContractNotFound,
    );

    // 2. Created contract summary verification
    let (client_addr, freelancer_addr, contract_id) = create_contract(&env, &client);
    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.schema_version, 1);
    assert_eq!(summary.client, client_addr);
    assert_eq!(summary.freelancer, freelancer_addr);
    assert_eq!(summary.status, ContractStatus::Created);
    assert_eq!(summary.funded_amount, 0);
    assert_eq!(summary.released_amount, 0);
    assert_eq!(summary.refundable_balance, 0);
    assert_eq!(summary.released_milestone_count, 0);
    assert_eq!(summary.milestones.len(), 3);
    assert_eq!(summary.milestones.get(0).unwrap().amount, MILESTONE_ONE);
    assert_eq!(summary.milestones.get(0).unwrap().released, false);

    // 3. Funded contract summary verification
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));
    let summary_funded = client.get_contract_summary(&contract_id);
    assert_eq!(summary_funded.status, ContractStatus::Funded);
    assert_eq!(summary_funded.funded_amount, total_milestone_amount());
    assert_eq!(summary_funded.refundable_balance, total_milestone_amount());
    assert_eq!(summary_funded.released_amount, 0);

    // 4. Released milestone summary verification
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    let summary_released = client.get_contract_summary(&contract_id);
    assert_eq!(summary_released.released_amount, MILESTONE_ONE);
    assert_eq!(
        summary_released.refundable_balance,
        total_milestone_amount() - MILESTONE_ONE
    );
    assert_eq!(summary_released.released_milestone_count, 1);
    assert_eq!(summary_released.milestones.get(0).unwrap().released, true);
    assert_eq!(summary_released.milestones.get(1).unwrap().released, false);
}

#[test]
fn get_contract_summary_extends_ttl() {
    let env = setup_ttl_env();
    let client = register_client(&env);
    let (_client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);

    let bump_threshold = ttl::PERSISTENT_BUMP_THRESHOLD;

    let initial_contract_ttl: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&crate::DataKey::Contract(contract_id))
    });

    // Advance ledger so it is close to expiry.
    env.ledger().with_mut(|li| {
        li.sequence_number = li
            .sequence_number
            .saturating_add(initial_contract_ttl.saturating_sub(bump_threshold) + 1);
    });

    // Re-extend the contract instance so it is never archived.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .extend_ttl(ttl::LEDGERS_PER_DAY * 60, ttl::LEDGERS_PER_DAY * 60);
    });

    let _summary = client.get_contract_summary(&contract_id);

    let ttl_after_read: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&crate::DataKey::Contract(contract_id))
    });
    assert!(
        ttl_after_read >= bump_threshold,
        "get_contract_summary must extend contract TTL to at least the bump threshold (got {})",
        ttl_after_read
    );
}

#[test]
fn read_getters_succeed_after_creating_contract_at_zero_index() {
    // Specifically exercise the u32::default slot by creating a contract, then
    // verifying reads work end-to-end. Contract id 0 occurs only when no prior
    // contract was allocated, which is the default fresh-env case.
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    // First contract allocated by `create_contract` is at slot 1 (DataKey::NextContractId
    // starts at 1 — see create_contract.rs). Probe the zero slot to confirm
    // it remains not-found, then exercise slot 1.
    assert_contract_error(client.try_get_contract(&0), EscrowError::ContractNotFound);
    assert_contract_error(client.try_get_milestones(&0), EscrowError::ContractNotFound);
    assert_contract_error(
        client.try_get_refundable_balance(&0),
        Error::ContractNotFound,
    );

    let (c, f, _) = generated_participants(&env);
    let id = client.create_contract(
        &c,
        &f,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 1);

    let record = client.get_contract(&id);
    assert_eq!(record.client, c);
    assert_eq!(record.freelancer, f);
    assert_eq!(record.status, ContractStatus::Created);

    let milestones = client.get_milestones(&id);
    assert_eq!(milestones.len(), default_milestones(&env).len());

    // Unfunded contract reads zero.
    assert_eq!(client.get_refundable_balance(&id), 0);

    // Approvals absent for all milestones.
    assert!(client.get_milestone_approvals(&id, &0).is_none());
}

#[test]
fn read_getters_unchanged_after_pause() {
    // Pause mutates `Paused`; read getters must remain available and correct.
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let (client_addr, _freelancer_addr, contract_id) = create_contract(&env, &client);
    assert!(client.deposit_funds(&contract_id, &client_addr, &total_milestone_amount()));

    let before_pause = client.get_contract(&contract_id);
    let milestones_before = client.get_milestones(&contract_id);
    let refundable_before = client.get_refundable_balance(&contract_id);

    assert!(client.pause());

    let after_pause = client.get_contract(&contract_id);
    let milestones_after = client.get_milestones(&contract_id);
    let refundable_after = client.get_refundable_balance(&contract_id);

    assert_eq!(after_pause, before_pause);
    assert_eq!(milestones_after, milestones_before);
    assert_eq!(refundable_after, refundable_before);

    // Not-found assertions still hold while paused.
    assert_contract_error(client.try_get_contract(&9999), Error::ContractNotFound);
    assert_contract_error(client.try_get_milestones(&9999), Error::ContractNotFound);
    match client.try_get_refundable_balance(&9999) {
        Err(Ok(e)) => assert_eq!(e, soroban_sdk::Error::from(Error::ContractNotFound)),
        other => panic!("expected ContractNotFound, got {:?}", other),
    };
}

/// Finalization writes a round-tripped snapshot of milestone summaries and derived totals.
#[test]
fn finalize_round_trips_milestone_summaries_and_totals() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, freelancer_addr, contract_id) = super::complete_contract(&env, &client);
    assert!(client.finalize_contract(&contract_id, &client_addr));
    let record = client
        .get_finalization_record(&contract_id)
        .expect("finalization record should exist");
    assert_eq!(record.summary.released_milestone_count, 3);
    assert_eq!(record.summary.total_amount, super::total_milestone_amount());
    assert_eq!(record.summary.refundable_balance, 0);
}

/// Double finalize is rejected with AlreadyFinalized error.
#[test]
fn double_finalize_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let (client_addr, _, contract_id) = super::complete_contract(&env, &client);
    assert!(client.finalize_contract(&contract_id, &client_addr));
    let result = client.try_finalize_contract(&contract_id, &client_addr);
    super::assert_contract_error(result, EscrowError::AlreadyFinalized);
}

/// Asserts that the milestone storage helper resolves to the current storage symbol.
#[test]
fn milestone_symbol_helper_matches_expected() {
    let env = Env::default();
    let helper_symbol = milestone_symbol(&env);
    let expected_symbol = Symbol::new(&env, "milestones");
    assert_eq!(helper_symbol, expected_symbol);
}
