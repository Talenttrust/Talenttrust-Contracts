//! # Milestone-level partial refund tests
//!
//! Covers `refund_milestone` and `get_refundable_balance`, verifying:
//! - Partial refunds (one or more milestones)
//! - Full refund (all milestones → status becomes `Refunded`)
//! - Mixed release + refund flows
//! - Accounting invariant: `total_deposited == released + refunded + available`
//! - All error paths (empty request, duplicate, already-released, already-refunded,
//!   insufficient balance)

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{ContractStatus, Escrow, EscrowClient};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    (env, client_addr, freelancer_addr)
}

fn register_client(env: &Env) -> EscrowClient {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

/// Create a 3-milestone contract (200 / 400 / 600 stroops = 1 200 total).
fn create_default_contract(
    env: &Env,
    client: &EscrowClient,
    client_addr: &Address,
    freelancer_addr: &Address,
) -> u32 {
    let milestones = vec![env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    client.create_contract(client_addr, freelancer_addr, &None, &milestones)
}

fn total_amount() -> i128 {
    200_0000000 + 400_0000000 + 600_0000000
}

// ─── Happy-path tests ─────────────────────────────────────────────────────────

/// Refunding a single unreleased milestone returns the correct amount and
/// preserves the accounting invariant.
#[test]
fn refund_single_milestone_updates_accounting() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));

    // Refund milestone 1 (400 stroops).
    let refunded = client.refund_milestone(&cid, &vec![&env, 1_u32]);
    assert_eq!(refunded, 400_0000000_i128);

    let record = client.get_contract(&cid);
    assert_eq!(record.refunded_amount, 400_0000000_i128);
    assert_eq!(record.released_amount, 0);
    assert_eq!(record.status, ContractStatus::Funded);

    // Invariant: deposited == released + refunded + available
    let available = client.get_refundable_balance(&cid);
    assert_eq!(available, total_amount() - 400_0000000_i128);
    assert_eq!(
        record.total_deposited,
        record.released_amount + record.refunded_amount + available
    );
}

/// Refunding multiple milestones in one call works correctly.
#[test]
fn refund_multiple_milestones_in_one_call() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));

    // Refund milestones 0 and 2 (200 + 600 = 800 stroops).
    let refunded = client.refund_milestone(&cid, &vec![&env, 0_u32, 2_u32]);
    assert_eq!(refunded, 200_0000000_i128 + 600_0000000_i128);

    let record = client.get_contract(&cid);
    assert_eq!(record.refunded_amount, 800_0000000_i128);
    assert_eq!(record.released_amount, 0);
    assert_eq!(record.status, ContractStatus::Funded); // milestone 1 still pending

    let available = client.get_refundable_balance(&cid);
    assert_eq!(available, 400_0000000_i128);
    assert_eq!(
        record.total_deposited,
        record.released_amount + record.refunded_amount + available
    );
}

/// When all milestones are refunded the contract status becomes `Refunded`.
#[test]
fn refunding_all_milestones_transitions_to_refunded_status() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));

    let refunded = client.refund_milestone(&cid, &vec![&env, 0_u32, 1_u32, 2_u32]);
    assert_eq!(refunded, total_amount());

    let record = client.get_contract(&cid);
    assert_eq!(record.status, ContractStatus::Refunded);
    assert_eq!(record.refunded_amount, total_amount());
    assert_eq!(record.released_amount, 0);
    assert_eq!(client.get_refundable_balance(&cid), 0);
}

/// Mixed flow: release some milestones, refund the rest → status `Refunded`.
#[test]
fn mixed_release_and_refund_settles_contract() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));

    // Release milestone 0 (200 stroops).
    assert!(client.release_milestone(&cid, &0));

    // Refund milestones 1 and 2 (400 + 600 = 1 000 stroops).
    let refunded = client.refund_milestone(&cid, &vec![&env, 1_u32, 2_u32]);
    assert_eq!(refunded, 400_0000000_i128 + 600_0000000_i128);

    let record = client.get_contract(&cid);
    assert_eq!(record.status, ContractStatus::Refunded);
    assert_eq!(record.released_amount, 200_0000000_i128);
    assert_eq!(record.refunded_amount, 1_000_0000000_i128);
    assert_eq!(client.get_refundable_balance(&cid), 0);

    // Invariant holds.
    assert_eq!(
        record.total_deposited,
        record.released_amount + record.refunded_amount + client.get_refundable_balance(&cid)
    );
}

/// Refunding one milestone at a time across multiple calls accumulates correctly.
#[test]
fn sequential_single_milestone_refunds_accumulate() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));

    // Refund milestone 2 first.
    let r1 = client.refund_milestone(&cid, &vec![&env, 2_u32]);
    assert_eq!(r1, 600_0000000_i128);

    let record = client.get_contract(&cid);
    assert_eq!(record.refunded_amount, 600_0000000_i128);
    assert_eq!(record.status, ContractStatus::Funded);

    // Refund milestone 1 next.
    let r2 = client.refund_milestone(&cid, &vec![&env, 1_u32]);
    assert_eq!(r2, 400_0000000_i128);

    let record = client.get_contract(&cid);
    assert_eq!(record.refunded_amount, 1_000_0000000_i128);
    assert_eq!(record.status, ContractStatus::Funded); // milestone 0 still pending

    // Refund milestone 0 last → all settled.
    let r3 = client.refund_milestone(&cid, &vec![&env, 0_u32]);
    assert_eq!(r3, 200_0000000_i128);

    let record = client.get_contract(&cid);
    assert_eq!(record.refunded_amount, total_amount());
    assert_eq!(record.status, ContractStatus::Refunded);
    assert_eq!(client.get_refundable_balance(&cid), 0);
}

/// `get_refundable_balance` decreases correctly after each refund.
#[test]
fn refundable_balance_decreases_after_each_refund() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));
    assert_eq!(client.get_refundable_balance(&cid), total_amount());

    client.refund_milestone(&cid, &vec![&env, 0_u32]);
    assert_eq!(
        client.get_refundable_balance(&cid),
        total_amount() - 200_0000000_i128
    );

    client.refund_milestone(&cid, &vec![&env, 1_u32]);
    assert_eq!(
        client.get_refundable_balance(&cid),
        total_amount() - 200_0000000_i128 - 400_0000000_i128
    );

    client.refund_milestone(&cid, &vec![&env, 2_u32]);
    assert_eq!(client.get_refundable_balance(&cid), 0);
}

/// Milestone-level `refunded` flag is set after a refund.
#[test]
fn milestone_refunded_flag_is_set_after_refund() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));
    client.refund_milestone(&cid, &vec![&env, 1_u32]);

    let milestones = client.get_milestones(&cid);
    assert!(!milestones.get(0).unwrap().refunded);
    assert!(milestones.get(1).unwrap().refunded);
    assert!(!milestones.get(2).unwrap().refunded);
}

/// Milestone-level `refunded_amount` equals the milestone amount after refund.
#[test]
fn milestone_refunded_amount_equals_milestone_amount() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));
    client.refund_milestone(&cid, &vec![&env, 2_u32]);

    let milestones = client.get_milestones(&cid);
    let m2 = milestones.get(2).unwrap();
    assert_eq!(m2.refunded_amount, m2.amount);
    assert_eq!(m2.refunded_amount, 600_0000000_i128);
}

/// Partial deposit: only milestone 0 is funded; refunding milestone 0 succeeds.
#[test]
fn partial_deposit_allows_refund_of_funded_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit only enough for milestone 0.
    assert!(client.deposit_funds(&cid, &200_0000000_i128));

    let refunded = client.refund_milestone(&cid, &vec![&env, 0_u32]);
    assert_eq!(refunded, 200_0000000_i128);

    let record = client.get_contract(&cid);
    assert_eq!(record.refunded_amount, 200_0000000_i128);
    assert_eq!(client.get_refundable_balance(&cid), 0);
}

// ─── Error-path tests ─────────────────────────────────────────────────────────

/// An empty milestone list is rejected.
#[test]
#[should_panic]
fn rejects_empty_refund_request() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));
    client.refund_milestone(&cid, &vec![&env]);
}

/// Duplicate milestone indices in a single call are rejected.
#[test]
#[should_panic]
fn rejects_duplicate_milestone_indices() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));
    client.refund_milestone(&cid, &vec![&env, 1_u32, 1_u32]);
}

/// Attempting to refund an already-released milestone is rejected.
#[test]
#[should_panic]
fn rejects_refund_of_released_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));
    assert!(client.release_milestone(&cid, &0));

    // Milestone 0 is released; refunding it must fail.
    client.refund_milestone(&cid, &vec![&env, 0_u32]);
}

/// Attempting to refund an already-refunded milestone is rejected (double-refund guard).
#[test]
#[should_panic]
fn rejects_double_refund_of_same_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));
    client.refund_milestone(&cid, &vec![&env, 2_u32]);

    // Second refund of the same milestone must fail.
    client.refund_milestone(&cid, &vec![&env, 2_u32]);
}

/// Refund is rejected when the escrow balance is insufficient.
#[test]
#[should_panic]
fn rejects_refund_when_balance_insufficient() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    // Deposit only 200 stroops but try to refund milestone 1 (400 stroops).
    assert!(client.deposit_funds(&cid, &200_0000000_i128));
    client.refund_milestone(&cid, &vec![&env, 1_u32]);
}

/// An out-of-bounds milestone index is rejected.
#[test]
#[should_panic]
fn rejects_out_of_bounds_milestone_index() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));
    client.refund_milestone(&cid, &vec![&env, 99_u32]);
}

/// Releasing a milestone that was previously refunded is rejected.
#[test]
#[should_panic]
fn rejects_release_of_refunded_milestone() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));
    client.refund_milestone(&cid, &vec![&env, 1_u32]);

    // Milestone 1 is refunded; releasing it must fail.
    client.release_milestone(&cid, &1);
}

// ─── Invariant stress tests ───────────────────────────────────────────────────

/// Interleaved releases and refunds always preserve the accounting invariant.
#[test]
fn accounting_invariant_holds_across_interleaved_operations() {
    let (env, client_addr, freelancer_addr) = setup();
    let client = register_client(&env);
    let cid = create_default_contract(&env, &client, &client_addr, &freelancer_addr);

    assert!(client.deposit_funds(&cid, &total_amount()));

    let check_invariant = |cid: &u32| {
        let r = client.get_contract(cid);
        let avail = client.get_refundable_balance(cid);
        assert_eq!(r.total_deposited, r.released_amount + r.refunded_amount + avail);
    };

    check_invariant(&cid);

    client.release_milestone(&cid, &0);
    check_invariant(&cid);

    client.refund_milestone(&cid, &vec![&env, 1_u32]);
    check_invariant(&cid);

    client.refund_milestone(&cid, &vec![&env, 2_u32]);
    check_invariant(&cid);

    let record = client.get_contract(&cid);
    assert_eq!(record.status, ContractStatus::Refunded);
}

/// A contract with a single milestone that is refunded reaches `Refunded` status.
#[test]
fn single_milestone_contract_reaches_refunded_status() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 500_0000000_i128];
    let cid = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);

    assert!(client.deposit_funds(&cid, &500_0000000_i128));
    let refunded = client.refund_milestone(&cid, &vec![&env, 0_u32]);
    assert_eq!(refunded, 500_0000000_i128);

    let record = client.get_contract(&cid);
    assert_eq!(record.status, ContractStatus::Refunded);
    assert_eq!(record.refunded_amount, 500_0000000_i128);
    assert_eq!(client.get_refundable_balance(&cid), 0);
}
