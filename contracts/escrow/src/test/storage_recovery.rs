//! Deterministic failure recovery for `crate::storage` (#1529).
//!
//! Every persistent read has three outcomes — present, missing, malformed —
//! and each must map to a fixed contract error without destroying the stored
//! bytes. Safety flags and the admin nonce must fail closed when malformed.
//!
//! These tests register `Escrow` directly and drive the helpers inside
//! `env.as_contract`, so they depend only on `crate::storage` and the types.
//!
//! ```sh
//! cargo test -p escrow --lib storage_recovery
//! ```

use crate::storage::{self, StorageRead};
use crate::{
    Contract, ContractStatus, DataKey, Error, Escrow, Milestone, PauseScope, PauseTarget,
    ReleaseAuthorization,
};
use soroban_sdk::{
    symbol_short, testutils::Address as _, vec, Address, Env, IntoVal, String, Symbol, Val, Vec,
};

fn setup() -> (Env, Address) {
    let env = Env::default();
    let id = env.register(Escrow, ());
    (env, id)
}

fn sample_contract(env: &Env) -> Contract {
    Contract {
        client: Address::generate(env),
        freelancer: Address::generate(env),
        arbiter: None,
        status: ContractStatus::Funded,
        total_deposited: 300,
        funded_amount: 300,
        released_amount: 0,
        refunded_amount: 0,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    }
}

fn milestone(amount: i128) -> Milestone {
    Milestone {
        amount,
        funded_amount: amount,
        released: false,
        refunded: false,
        work_evidence: None,
        refunded_amount: 0,
        deadline: None,
    }
}

fn milestone_key(env: &Env, contract_id: u32) -> (DataKey, Symbol) {
    (
        DataKey::Contract(contract_id),
        Symbol::new(env, "milestones"),
    )
}

// ── read_persistent: the three outcomes ──────────────────────────────────────

#[test]
fn read_persistent_distinguishes_present_missing_and_corrupt() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        assert_eq!(
            storage::read_persistent::<_, u64>(&env, &DataKey::AdminNonce),
            StorageRead::Missing
        );

        env.storage().persistent().set(&DataKey::AdminNonce, &7u64);
        assert_eq!(
            storage::read_persistent::<_, u64>(&env, &DataKey::AdminNonce),
            StorageRead::Present(7)
        );

        env.storage()
            .persistent()
            .set(&DataKey::AdminNonce, &symbol_short!("junk"));
        assert_eq!(
            storage::read_persistent::<_, u64>(&env, &DataKey::AdminNonce),
            StorageRead::Corrupt
        );
    });
}

#[test]
fn malformed_entry_is_never_reported_as_default() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage().persistent().set(&DataKey::Paused, &5u32);
        assert_eq!(
            storage::read_persistent::<_, bool>(&env, &DataKey::Paused).or_default(false),
            Err(Error::StorageInvariantViolated)
        );
    });
}

// ── Contract loading ─────────────────────────────────────────────────────────

#[test]
fn try_load_contract_success() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let contract = sample_contract(&env);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(1), &contract);
        assert_eq!(storage::try_load_contract(&env, 1), Ok(contract));
    });
}

#[test]
fn try_load_contract_rejects_zero_id_before_storage() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        assert_eq!(
            storage::try_load_contract(&env, 0),
            Err(Error::InvalidContractId)
        );
    });
}

#[test]
fn try_load_contract_missing_is_contract_not_found() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        assert_eq!(
            storage::try_load_contract(&env, 9),
            Err(Error::ContractNotFound)
        );
    });
}

#[test]
fn try_load_contract_malformed_is_typed_and_preserves_bytes() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(1), &42u32);

        assert_eq!(
            storage::try_load_contract(&env, 1),
            Err(Error::StorageInvariantViolated)
        );
        // Repeated reads give the same verdict: no retry state, no repair.
        assert_eq!(
            storage::try_load_contract(&env, 1),
            Err(Error::StorageInvariantViolated)
        );
        // The malformed entry is left exactly as it was, for a migration to inspect.
        let raw: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(1))
            .unwrap();
        assert_eq!(raw, 42);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #87)")]
fn load_contract_panics_with_storage_invariant_on_malformed_entry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(1), &42u32);
        storage::load_contract(&env, 1);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn load_contract_panics_with_not_found_on_missing_entry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        storage::load_contract(&env, 1);
    });
}

// ── Milestone loading ────────────────────────────────────────────────────────

#[test]
fn try_load_milestones_success() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let milestones = vec![&env, milestone(100), milestone(200)];
        env.storage()
            .persistent()
            .set(&milestone_key(&env, 1), &milestones);
        assert_eq!(storage::try_load_milestones(&env, 1), Ok(milestones));
    });
}

#[test]
fn try_load_milestones_boundaries() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        assert_eq!(
            storage::try_load_milestones(&env, 0),
            Err(Error::InvalidContractId)
        );
        assert_eq!(
            storage::try_load_milestones(&env, 1),
            Err(Error::ContractNotFound)
        );

        // An empty vector is valid data, not a missing entry.
        let empty: Vec<Milestone> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&milestone_key(&env, 2), &empty);
        assert_eq!(storage::try_load_milestones(&env, 2), Ok(empty));
    });
}

#[test]
fn try_load_milestones_malformed_vector_is_typed() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage()
            .persistent()
            .set(&milestone_key(&env, 1), &true);
        assert_eq!(
            storage::try_load_milestones(&env, 1),
            Err(Error::StorageInvariantViolated)
        );
    });
}

#[test]
fn try_load_milestones_malformed_element_is_detected_up_front() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        // A valid first element followed by a value that is not a Milestone.
        let mixed: Vec<Val> = vec![
            &env,
            milestone(100).into_val(&env),
            symbol_short!("junk").into_val(&env),
        ];
        env.storage()
            .persistent()
            .set(&milestone_key(&env, 1), &mixed);

        assert_eq!(
            storage::try_load_milestones(&env, 1),
            Err(Error::StorageInvariantViolated)
        );
        let raw: Vec<Val> = env
            .storage()
            .persistent()
            .get(&milestone_key(&env, 1))
            .unwrap();
        assert_eq!(raw.len(), 2);
    });
}

// ── Pause and emergency flags fail closed ────────────────────────────────────

#[test]
fn check_not_paused_matrix() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        assert_eq!(storage::check_not_paused(&env), Ok(()));

        env.storage().persistent().set(&DataKey::Paused, &false);
        env.storage().persistent().set(&DataKey::Emergency, &false);
        assert_eq!(storage::check_not_paused(&env), Ok(()));

        env.storage().persistent().set(&DataKey::Emergency, &true);
        assert_eq!(storage::check_not_paused(&env), Err(Error::EmergencyActive));

        // Legacy pause takes precedence over emergency.
        env.storage().persistent().set(&DataKey::Paused, &true);
        assert_eq!(storage::check_not_paused(&env), Err(Error::ContractPaused));
    });
}

#[test]
fn malformed_pause_flag_fails_closed() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage().persistent().set(&DataKey::Paused, &1u32);
        assert_eq!(
            storage::check_not_paused(&env),
            Err(Error::StorageInvariantViolated)
        );
    });
}

#[test]
fn malformed_emergency_flag_fails_closed() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Emergency, &symbol_short!("on"));
        assert_eq!(
            storage::check_not_paused(&env),
            Err(Error::StorageInvariantViolated)
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #87)")]
fn require_pause_scope_rejects_malformed_scope() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage().persistent().set(&DataKey::PauseScope, &7u32);
        storage::require_pause_scope(&env, &PauseTarget::Payout);
    });
}

#[test]
fn require_pause_scope_allows_non_overlapping_scope() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let scope = PauseScope {
            target: PauseTarget::Dispute,
            reason: String::from_str(&env, "maintenance"),
            paused_at: 0,
        };
        env.storage().persistent().set(&DataKey::PauseScope, &scope);
        storage::require_pause_scope(&env, &PauseTarget::Payout);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #74)")]
fn require_pause_scope_blocks_overlapping_scope() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let scope = PauseScope {
            target: PauseTarget::Payout,
            reason: String::from_str(&env, "incident"),
            paused_at: 0,
        };
        env.storage().persistent().set(&DataKey::PauseScope, &scope);
        storage::require_pause_scope(&env, &PauseTarget::Payout);
    });
}

// ── Initialization flag ──────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #87)")]
fn require_initialized_rejects_malformed_flag() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage().persistent().set(&DataKey::Initialized, &1u32);
        storage::require_initialized(&env);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #34)")]
fn malformed_initialized_flag_cannot_be_used_to_reinitialize() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage().persistent().set(&DataKey::Initialized, &1u32);
        storage::require_not_initialized(&env);
    });
}

// ── Admin nonce ──────────────────────────────────────────────────────────────

#[test]
fn admin_nonce_is_strictly_sequential() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        storage::consume_admin_nonce(&env, 1);
        storage::consume_admin_nonce(&env, 2);
        let stored: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::AdminNonce)
            .unwrap();
        assert_eq!(stored, 2);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #76)")]
fn admin_nonce_replay_is_rejected() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        storage::consume_admin_nonce(&env, 1);
        storage::consume_admin_nonce(&env, 1);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #45)")]
fn admin_nonce_at_u64_max_fails_closed() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage()
            .persistent()
            .set(&DataKey::AdminNonce, &u64::MAX);
        storage::consume_admin_nonce(&env, 0);
    });
}

/// A malformed counter must not restart the sequence at 0, which would make
/// nonce `1` (and every later one) acceptable a second time.
#[test]
#[should_panic(expected = "Error(Contract, #87)")]
fn malformed_admin_nonce_fails_closed_instead_of_restarting() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage()
            .persistent()
            .set(&DataKey::AdminNonce, &symbol_short!("junk"));
        storage::consume_admin_nonce(&env, 1);
    });
}

// ── Finalization ─────────────────────────────────────────────────────────────

#[test]
fn finalization_guards() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        assert!(!storage::is_finalized(&env, 1));
        assert!(storage::require_not_finalized(&env, 1));
        env.storage()
            .persistent()
            .set(&DataKey::Finalization(1), &true);
        assert!(storage::is_finalized(&env, 1));
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn finalization_rejects_zero_id() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        storage::is_finalized(&env, 0);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn load_contract_checked_reports_not_found_before_finalized() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Finalization(3), &true);
        storage::load_contract_checked(&env, 3, true, true);
    });
}
