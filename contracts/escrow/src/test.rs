//! Comprehensive test suite for the TalentTrust escrow contract.
//!
//! # Coverage strategy
//! Tests are grouped by functional area and cover:
//! - Happy paths for every public entry point.
//! - Checklist auto-update behaviour after each operation.
//! - Enforcement: `release_milestone` must panic when readiness gates are unmet.
//! - Edge cases: zero deposit, out-of-range milestone ID, duplicate release.
//! - Full end-to-end scenario exercising all six checklist items.

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient};

// ---------------------------------------------------------------------------
// Utility / smoke tests
// ---------------------------------------------------------------------------

#[test]
fn test_hello() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

// ---------------------------------------------------------------------------
// create_contract
// ---------------------------------------------------------------------------

#[test]
fn test_create_contract_returns_nonzero_id() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    assert!(id > 0, "contract ID must be positive");
}

#[test]
fn test_create_contract_ids_are_unique() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id1 = client.create_contract(&ca, &fa, &milestones);
    let id2 = client.create_contract(&ca, &fa, &milestones);
    assert_ne!(
        id1, id2,
        "each create_contract call must produce a unique ID"
    );
}

#[test]
fn test_checklist_initialized_on_create() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    let checklist = client.get_release_checklist(&id);

    // Deployment phase: contract_created Γ£ö, funds_deposited Γ£ÿ
    assert!(checklist.contract_created);
    assert!(!checklist.funds_deposited);
    // Verification phase: both true
    assert!(checklist.parties_authenticated);
    assert!(checklist.milestones_defined);
    // Post-deploy: both false
    assert!(!checklist.all_milestones_released);
    assert!(!checklist.reputation_issued);
}

#[test]
#[should_panic]
fn test_create_contract_panics_with_too_many_milestones() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    // 21 milestones ΓÇö exceeds MAX_MILESTONES (20).
    let mut amounts = vec![&env, 100_0000000_i128];
    for _ in 0..20 {
        amounts.push_back(100_0000000_i128);
    }
    client.create_contract(&ca, &fa, &amounts);
}

#[test]
#[should_panic]
fn test_create_contract_panics_with_zero_milestones() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let empty: soroban_sdk::Vec<i128> = vec![&env];
    client.create_contract(&ca, &fa, &empty);
}

// ---------------------------------------------------------------------------
// is_release_ready ΓÇö pre-deposit (false)
// ---------------------------------------------------------------------------

#[test]
fn test_is_release_ready_false_before_deposit() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    assert!(!client.is_release_ready(&id));
}

// ---------------------------------------------------------------------------
// deposit_funds
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_funds_sets_checklist_flag() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    let result = client.deposit_funds(&id, &500_0000000_i128);
    assert!(result);
    let checklist = client.get_release_checklist(&id);
    assert!(checklist.funds_deposited);
}

#[test]
fn test_is_release_ready_true_after_deposit() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    client.deposit_funds(&id, &500_0000000_i128);
    assert!(client.is_release_ready(&id));
}

#[test]
#[should_panic]
fn test_deposit_zero_panics() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    client.deposit_funds(&id, &0_i128);
}

#[test]
#[should_panic]
fn test_deposit_negative_panics() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    client.deposit_funds(&id, &-1_i128);
}

#[test]
#[should_panic]
fn test_deposit_unknown_contract_panics() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    client.deposit_funds(&999, &100_0000000_i128);
}

// ---------------------------------------------------------------------------
// release_milestone ΓÇö enforcement tests
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_release_blocked_before_deposit() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    // No deposit ΓÇö checklist incomplete ΓÇö must panic.
    client.release_milestone(&id, &0);
}

#[test]
fn test_release_milestone_succeeds_when_ready() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    client.deposit_funds(&id, &100_0000000_i128);
    let result = client.release_milestone(&id, &0);
    assert!(result);
}

#[test]
fn test_all_milestones_released_flag_set_after_last_release() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128, 300_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    client.deposit_funds(&id, &600_0000000_i128);

    client.release_milestone(&id, &0);
    client.release_milestone(&id, &1);
    let checklist = client.get_release_checklist(&id);
    assert!(
        !checklist.all_milestones_released,
        "should be false until ALL milestones released"
    );

    client.release_milestone(&id, &2);
    let checklist = client.get_release_checklist(&id);
    assert!(checklist.all_milestones_released);
}

#[test]
#[should_panic]
fn test_release_invalid_milestone_id_panics() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    client.deposit_funds(&id, &100_0000000_i128);
    // Index 1 is out of range for a single-milestone contract.
    client.release_milestone(&id, &1);
}

#[test]
#[should_panic]
fn test_release_duplicate_milestone_panics() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    client.deposit_funds(&id, &100_0000000_i128);
    client.release_milestone(&id, &0);
    // Second release of same milestone must panic.
    client.release_milestone(&id, &0);
}

// ---------------------------------------------------------------------------
// issue_reputation
// ---------------------------------------------------------------------------

#[test]
fn test_issue_reputation_updates_checklist() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);
    let fa_addr = Address::generate(&env);
    let result = client.issue_reputation(&id, &fa_addr, &5_i128);
    assert!(result);
    let checklist = client.get_release_checklist(&id);
    assert!(checklist.reputation_issued);
}

#[test]
#[should_panic]
fn test_issue_reputation_unknown_contract_panics() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let fa_addr = Address::generate(&env);
    client.issue_reputation(&999, &fa_addr, &5_i128);
}

// ---------------------------------------------------------------------------
// is_post_deploy_complete ΓÇö full end-to-end
// ---------------------------------------------------------------------------

#[test]
fn test_is_post_deploy_complete_full_lifecycle() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128, 200_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);

    assert!(!client.is_post_deploy_complete(&id));
    client.deposit_funds(&id, &300_0000000_i128);
    assert!(!client.is_post_deploy_complete(&id));
    client.release_milestone(&id, &0);
    client.release_milestone(&id, &1);
    assert!(!client.is_post_deploy_complete(&id)); // reputation still missing
    client.issue_reputation(&id, &fa, &5_i128);
    assert!(client.is_post_deploy_complete(&id));
}

// ---------------------------------------------------------------------------
// get_release_checklist ΓÇö state progression
// ---------------------------------------------------------------------------

#[test]
fn test_get_release_checklist_reflects_state_progression() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id = client.create_contract(&ca, &fa, &milestones);

    let count_true = |cl: crate::ReleaseChecklist| -> u32 {
        [
            cl.contract_created,
            cl.funds_deposited,
            cl.parties_authenticated,
            cl.milestones_defined,
            cl.all_milestones_released,
            cl.reputation_issued,
        ]
        .iter()
        .filter(|&&v| v)
        .count() as u32
    };

    assert_eq!(count_true(client.get_release_checklist(&id)), 3);
    client.deposit_funds(&id, &100_0000000_i128);
    assert_eq!(count_true(client.get_release_checklist(&id)), 4);
}

#[test]
#[should_panic]
fn test_get_release_checklist_unknown_contract_panics() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    client.get_release_checklist(&999);
}

// ---------------------------------------------------------------------------
// Isolation: multiple contracts don't share state
// ---------------------------------------------------------------------------

#[test]
fn test_independent_contracts_do_not_share_checklist_state() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let ca = Address::generate(&env);
    let fa = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let id1 = client.create_contract(&ca, &fa, &milestones);
    let id2 = client.create_contract(&ca, &fa, &milestones);
    client.deposit_funds(&id1, &100_0000000_i128);
    assert!(client.is_release_ready(&id1));
    assert!(!client.is_release_ready(&id2));
}
