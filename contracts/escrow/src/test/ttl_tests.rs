//! Storage TTL tests for transient approval and migration entries.
//!
//! These tests exercise the TTL helpers in [`crate::ttl`] directly via
//! `env.as_contract`, advancing the ledger sequence to prove Soroban's
//! auto-eviction semantics for both approval and migration TTL constants.

#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{storage::Temporary as _, Address as _, Ledger as _},
    Env, Symbol,
};

use crate::{
    approvals,
    ttl::{
        compute_expiry, extend_if_below_threshold, has_transient, read_if_live, remove_transient,
        store_with_ttl, LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD,
        PENDING_APPROVAL_TTL_LEDGERS, PENDING_MIGRATION_BUMP_THRESHOLD,
        PENDING_MIGRATION_TTL_LEDGERS,
    },
    Error, Escrow, ReleaseAuthorization,
};

const INSTANCE_TTL: u32 = PENDING_MIGRATION_TTL_LEDGERS * 4;

fn setup() -> (Env, soroban_sdk::Address) {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.max_entry_ttl = INSTANCE_TTL;
        li.min_persistent_entry_ttl = INSTANCE_TTL;
        li.sequence_number = 1_000;
    });
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

fn advance(env: &Env, contract_id: &soroban_sdk::Address, by: u32) {
    env.ledger()
        .set_sequence_number(env.ledger().sequence().saturating_add(by));
    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    });
}

fn approval_key() -> Symbol {
    symbol_short!("appr")
}

fn migration_key() -> Symbol {
    symbol_short!("migr")
}

#[test]
fn compute_expiry_equals_sequence_plus_ttl() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let seq = env.ledger().sequence();
        assert_eq!(
            compute_expiry(&env, PENDING_APPROVAL_TTL_LEDGERS),
            seq + PENDING_APPROVAL_TTL_LEDGERS
        );
        assert_eq!(
            compute_expiry(&env, PENDING_MIGRATION_TTL_LEDGERS),
            seq + PENDING_MIGRATION_TTL_LEDGERS
        );
    });
}

#[test]
fn compute_expiry_saturates_on_overflow() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let seq = env.ledger().sequence();
        assert_eq!(compute_expiry(&env, u32::MAX - seq), u32::MAX);
        assert_eq!(compute_expiry(&env, u32::MAX), u32::MAX);
    });
}

#[test]
fn ledgers_per_day_constant_is_correct() {
    assert_eq!(LEDGERS_PER_DAY, 17_280);
    assert_eq!(PENDING_APPROVAL_TTL_LEDGERS, LEDGERS_PER_DAY * 7);
    assert_eq!(PENDING_MIGRATION_TTL_LEDGERS, LEDGERS_PER_DAY * 21);
    assert_eq!(PENDING_APPROVAL_BUMP_THRESHOLD, LEDGERS_PER_DAY);
    assert_eq!(PENDING_MIGRATION_BUMP_THRESHOLD, LEDGERS_PER_DAY * 3);
}

#[test]
fn approval_readable_before_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &42u32, PENDING_APPROVAL_TTL_LEDGERS);
    });

    advance(&env, &id, PENDING_APPROVAL_TTL_LEDGERS - 1);

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &approval_key());
        assert_eq!(val, Some(42u32), "entry must be live before TTL elapses");
    });
}

#[test]
fn approval_evicted_after_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &99u32, PENDING_APPROVAL_TTL_LEDGERS);
    });

    let expiry = env.as_contract(&id, || compute_expiry(&env, PENDING_APPROVAL_TTL_LEDGERS));
    env.ledger().set_sequence_number(expiry.saturating_add(1));
    env.as_contract(&id, || {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
    });

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &approval_key());
        assert!(val.is_none(), "entry must be evicted after TTL elapses");
        assert!(
            !has_transient(&env, &approval_key()),
            "evicted entries must not be reported as transient"
        );
    });
}

#[test]
fn migration_readable_before_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &migration_key(), &7u32, PENDING_MIGRATION_TTL_LEDGERS);
    });

    advance(&env, &id, PENDING_MIGRATION_TTL_LEDGERS - 1);

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &migration_key());
        assert_eq!(
            val,
            Some(7u32),
            "migration entry must be live before TTL elapses"
        );
    });
}

#[test]
fn migration_evicted_after_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &migration_key(), &7u32, PENDING_MIGRATION_TTL_LEDGERS);
    });

    advance(&env, &id, PENDING_MIGRATION_TTL_LEDGERS + 1);

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &migration_key());
        assert!(
            val.is_none(),
            "migration entry must be evicted after TTL elapses"
        );
    });
}

#[test]
fn extend_returns_false_for_absent_key() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let result = extend_if_below_threshold(
            &env,
            &approval_key(),
            PENDING_APPROVAL_BUMP_THRESHOLD,
            PENDING_APPROVAL_TTL_LEDGERS,
        );
        assert!(!result, "must return false when key is absent");
    });
}

#[test]
fn extend_returns_true_and_entry_survives_past_original_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &1u32, PENDING_APPROVAL_TTL_LEDGERS);
    });

    advance(
        &env,
        &id,
        PENDING_APPROVAL_TTL_LEDGERS - PENDING_APPROVAL_BUMP_THRESHOLD + 1,
    );

    env.as_contract(&id, || {
        let bumped = extend_if_below_threshold(
            &env,
            &approval_key(),
            PENDING_APPROVAL_BUMP_THRESHOLD,
            PENDING_APPROVAL_TTL_LEDGERS,
        );
        assert!(bumped, "must return true for a live entry that is bumped");
    });

    advance(&env, &id, PENDING_APPROVAL_BUMP_THRESHOLD + 1);

    env.as_contract(&id, || {
        let val: Option<u32> = read_if_live(&env, &approval_key());
        assert!(
            val.is_some(),
            "entry must survive past original expiry after bump"
        );
    });
}

#[test]
fn extend_is_a_no_op_when_remaining_ttl_is_at_threshold() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &1u32, PENDING_APPROVAL_TTL_LEDGERS);
    });

    advance(
        &env,
        &id,
        PENDING_APPROVAL_TTL_LEDGERS - PENDING_APPROVAL_BUMP_THRESHOLD - 1,
    );

    env.as_contract(&id, || {
        let ttl_before = env.storage().temporary().get_ttl(&approval_key());
        assert_eq!(ttl_before, PENDING_APPROVAL_BUMP_THRESHOLD + 1);
        assert!(
            extend_if_below_threshold(
                &env,
                &approval_key(),
                PENDING_APPROVAL_BUMP_THRESHOLD,
                PENDING_APPROVAL_TTL_LEDGERS,
            ),
            "the key remains live even though its TTL should not be bumped"
        );
        assert_eq!(
            env.storage().temporary().get_ttl(&approval_key()),
            ttl_before,
            "a TTL at the threshold must not be extended"
        );
    });
}

#[test]
fn extend_migration_returns_false_for_absent_key() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        let result = extend_if_below_threshold(
            &env,
            &migration_key(),
            PENDING_MIGRATION_BUMP_THRESHOLD,
            PENDING_MIGRATION_TTL_LEDGERS,
        );
        assert!(!result);
    });
}

#[test]
fn remove_transient_clears_entry_immediately() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &5u32, PENDING_APPROVAL_TTL_LEDGERS);
        assert!(
            has_transient(&env, &approval_key()),
            "entry must exist before removal"
        );
        remove_transient(&env, &approval_key());
        assert!(
            !has_transient(&env, &approval_key()),
            "entry must be gone after removal"
        );
        let val: Option<u32> = read_if_live(&env, &approval_key());
        assert!(val.is_none(), "read_if_live must return None after removal");
    });
}

#[test]
fn remove_transient_is_idempotent() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        assert!(!has_transient(&env, &approval_key()));
        remove_transient(&env, &approval_key());
        assert!(!has_transient(&env, &approval_key()));
    });
}

#[test]
fn has_transient_false_before_store() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        assert!(!has_transient(&env, &approval_key()));
    });
}

#[test]
fn has_transient_true_after_store_false_after_expiry() {
    let (env, id) = setup();
    env.as_contract(&id, || {
        store_with_ttl(&env, &approval_key(), &1u32, PENDING_APPROVAL_TTL_LEDGERS);
        assert!(has_transient(&env, &approval_key()));
    });

    advance(&env, &id, PENDING_APPROVAL_TTL_LEDGERS + 1);

    env.as_contract(&id, || {
        assert!(
            !has_transient(&env, &approval_key()),
            "has_transient must be false after eviction"
        );
    });
}

#[test]
fn expiry_is_deterministic_across_independent_envs() {
    let (env_a, id_a) = setup();
    let (env_b, id_b) = setup();

    let expiry_a = env_a.as_contract(&id_a, || {
        compute_expiry(&env_a, PENDING_APPROVAL_TTL_LEDGERS)
    });
    let expiry_b = env_b.as_contract(&id_b, || {
        compute_expiry(&env_b, PENDING_APPROVAL_TTL_LEDGERS)
    });

    assert_eq!(
        expiry_a, expiry_b,
        "expiry must be deterministic given the same starting sequence"
    );
}

mod approval_ttl_integration {
    use super::*;
    use crate::{Contract, ContractStatus, DataKey, Milestone};

    fn setup_env_with_contract() -> (
        Env,
        soroban_sdk::Address,
        soroban_sdk::Address,
        soroban_sdk::Address,
        u32,
        soroban_sdk::Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| {
            li.max_entry_ttl = INSTANCE_TTL;
            li.min_persistent_entry_ttl = INSTANCE_TTL;
            li.sequence_number = 1_000;
        });

        let escrow_id = env.register(Escrow, ());
        let client_addr = soroban_sdk::Address::generate(&env);
        let freelancer_addr = soroban_sdk::Address::generate(&env);
        let arbiter_addr = soroban_sdk::Address::generate(&env);

        let contract = Contract {
            client: client_addr.clone(),
            freelancer: freelancer_addr.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 6000_0000000_i128,
            funded_amount: 6000_0000000_i128,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };

        env.as_contract(&escrow_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(1), &contract);
            let milestones = soroban_sdk::Vec::from_array(
                &env,
                [Milestone {
                    amount: 6000_0000000_i128,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let milestone_key = Symbol::new(&env, "milestones");
            env.storage()
                .persistent()
                .set(&(DataKey::Contract(1), milestone_key), &milestones);
        });

        (
            env,
            client_addr,
            freelancer_addr,
            arbiter_addr,
            1,
            escrow_id,
        )
    }

    #[test]
    fn check_approvals_returns_insufficient_after_ttl_elapses() {
        let (env, client, _freelancer, _arbiter, contract_id, escrow_id) =
            setup_env_with_contract();

        env.as_contract(&escrow_id, || {
            approvals::approve_milestone(&env, contract_id, 0, &client).unwrap();
        });

        let check_before = env.as_contract(&escrow_id, || {
            let contract: Contract = env
                .storage()
                .persistent()
                .get(&DataKey::Contract(contract_id))
                .unwrap();
            approvals::check_approvals(&env, &contract, contract_id, 0)
        });
        assert!(check_before.is_ok(), "approval should be valid before TTL");

        advance(&env, &escrow_id, PENDING_APPROVAL_TTL_LEDGERS + 1);

        let check_after = env.as_contract(&escrow_id, || {
            let contract: Contract = env
                .storage()
                .persistent()
                .get(&DataKey::Contract(contract_id))
                .unwrap();
            approvals::check_approvals(&env, &contract, contract_id, 0)
        });
        assert_eq!(
            check_after,
            Err(Error::InsufficientApprovals),
            "check_approvals must return InsufficientApprovals after TTL expires"
        );
    }

    #[test]
    fn check_approvals_valid_within_ttl() {
        let (env, client, _freelancer, _arbiter, contract_id, escrow_id) =
            setup_env_with_contract();

        env.as_contract(&escrow_id, || {
            approvals::approve_milestone(&env, contract_id, 0, &client).unwrap();
        });

        advance(&env, &escrow_id, PENDING_APPROVAL_TTL_LEDGERS / 2);

        let check = env.as_contract(&escrow_id, || {
            let contract: Contract = env
                .storage()
                .persistent()
                .get(&DataKey::Contract(contract_id))
                .unwrap();
            approvals::check_approvals(&env, &contract, contract_id, 0)
        });
        assert!(check.is_ok(), "approval should be valid within TTL");
    }

    #[test]
    fn check_approvals_multisig_both_required() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| {
            li.max_entry_ttl = INSTANCE_TTL;
            li.min_persistent_entry_ttl = INSTANCE_TTL;
            li.sequence_number = 1_000;
        });

        let escrow_id = env.register(Escrow, ());
        let client_addr = soroban_sdk::Address::generate(&env);
        let freelancer_addr = soroban_sdk::Address::generate(&env);

        let contract = Contract {
            client: client_addr.clone(),
            freelancer: freelancer_addr.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 6000_0000000_i128,
            funded_amount: 6000_0000000_i128,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::MultiSig,
            reputation_issued: false,
        };

        env.as_contract(&escrow_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(1), &contract);
            let milestones = soroban_sdk::Vec::from_array(
                &env,
                [Milestone {
                    amount: 6000_0000000_i128,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let milestone_key = Symbol::new(&env, "milestones");
            env.storage()
                .persistent()
                .set(&(DataKey::Contract(1), milestone_key), &milestones);
        });

        env.as_contract(&escrow_id, || {
            approvals::approve_milestone(&env, 1, 0, &client_addr).unwrap();
        });

        let check = env.as_contract(&escrow_id, || {
            let contract: Contract = env
                .storage()
                .persistent()
                .get(&DataKey::Contract(1))
                .unwrap();
            approvals::check_approvals(&env, &contract, 1, 0)
        });
        assert_eq!(
            check,
            Err(Error::InsufficientApprovals),
            "MultiSig requires both approvals"
        );

        env.as_contract(&escrow_id, || {
            approvals::approve_milestone(&env, 1, 0, &freelancer_addr).unwrap();
        });

        let check2 = env.as_contract(&escrow_id, || {
            let contract: Contract = env
                .storage()
                .persistent()
                .get(&DataKey::Contract(1))
                .unwrap();
            approvals::check_approvals(&env, &contract, 1, 0)
        });
        assert!(check2.is_ok(), "MultiSig passes with both approvals");
    }
}
