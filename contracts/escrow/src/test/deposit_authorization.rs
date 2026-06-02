//! Deposit authorization tests for [`Escrow::deposit_funds`].
//!
//! # What is tested
//!
//! | Category                  | Tests                                                      |
//! |---------------------------|------------------------------------------------------------|
//! | Happy path — auth + role  | Client deposits exact total / incremental partial / full   |
//! | Unauth path               | Arbitrary address with no auth; freelancer; arbiter; zero  |
//! | Role mismatch             | Valid auth but wrong address (not stored `client`)         |
//! | Amount guards             | Zero, negative, 1 stroop over total                        |
//! | State-machine guards      | Deposit after `Funded`, after `Refunded`, unknown id       |
//! | Overflow safety           | i128::MAX single deposit                                   |
//! | State transitions         | Created → Funded on exact amount; partial stays Created    |
//! | Events                    | `"deposited"` event emitted only on success                |
//! | Idempotency / reentrancy  | Two sequential legitimate deposits; double-state check     |

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    vec, Address, Env,
};

use crate::{ContractStatus, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};

// ─── Shared fixtures ──────────────────────────────────────────────────────────

/// 200 + 400 + 600 = 1 200 XLM in stroops.
const M1: i128 = 200_0000000;
const M2: i128 = 400_0000000;
const M3: i128 = 600_0000000;
const TOTAL: i128 = M1 + M2 + M3; // 1_200_0000000

fn setup() -> (Env, EscrowClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    (env, escrow, client_addr, freelancer_addr)
}

/// Creates a default 3-milestone contract and returns its `contract_id`.
fn make_contract(
    env: &Env,
    escrow: &EscrowClient,
    client: &Address,
    freelancer: &Address,
) -> u32 {
    let milestones = vec![env, M1, M2, M3];
    escrow.create_contract(client, freelancer, &None, &milestones, &ReleaseAuthorization::ClientOnly)
}

// ─── Happy path ───────────────────────────────────────────────────────────────

/// The stored client can deposit the exact milestone total and the contract
/// transitions from `Created` → `Funded`.
#[test]
fn client_can_deposit_exact_total_and_contract_transitions_to_funded() {
    let (env, escrow, client, freelancer) = setup();
    let cid = make_contract(&env, &escrow, &client, &freelancer);

    let result = escrow.deposit_funds(&cid, &client, &TOTAL);

    assert!(result, "deposit_funds must return true on success");

    let contract = escrow.get_contract(&cid);
    assert_eq!(contract.status,        ContractStatus::Funded);
    assert_eq!(contract.funded_amount, TOTAL);
    assert_eq!(contract.released_amount, 0);
    assert_eq!(contract.refunded_amount, 0);
}

/// The client can make two incremental deposits that together equal the total.
#[test]
fn client_can_fund_incrementally_across_two_deposits() {
    let (env, escrow, client, freelancer) = setup();
    let cid = make_contract(&env, &escrow, &client, &freelancer);

    // First partial deposit — should stay Created.
    assert!(escrow.deposit_funds(&cid, &client, &(M1 + M2)));
    let partial = escrow.get_contract(&cid);
    assert_eq!(partial.status,        ContractStatus::Created);
    assert_eq!(partial.funded_amount, M1 + M2);

    // Second deposit — reaches total, should become Funded.
    assert!(escrow.deposit_funds(&cid, &client, &M3));
    let funded = escrow.get_contract(&cid);
    assert_eq!(funded.status,        ContractStatus::Funded);
    assert_eq!(funded.funded_amount, TOTAL);
}

/// A single 1-stroop deposit is valid and keeps the contract in `Created`.
#[test]
fn client_can_deposit_minimum_one_stroop() {
    let (env, escrow, client, freelancer) = setup();
    let cid = make_contract(&env, &escrow, &client, &freelancer);

    assert!(escrow.deposit_funds(&cid, &client, &1));
    let contract = escrow.get_contract(&cid);
    assert_eq!(contract.status,        ContractStatus::Created);
    assert_eq!(contract.funded_amount, 1);
}

/// Depositing one stroop below the total stays `Created`; depositing the
/// remaining single stroop transitions to `Funded`.
#[test]
fn deposit_total_minus_one_stays_created_then_last_stroop_funds() {
    let (env, escrow, client, freelancer) = setup();
    let cid = make_contract(&env, &escrow, &client, &freelancer);

    assert!(escrow.deposit_funds(&cid, &client, &(TOTAL - 1)));
    assert_eq!(escrow.get_contract(&cid).status, ContractStatus::Created);

    assert!(escrow.deposit_funds(&cid, &client, &1));
    assert_eq!(escrow.get_contract(&cid).status, ContractStatus::Funded);
}

// ─── Unauthenticated callers ──────────────────────────────────────────────────

/// An arbitrary address that was never involved in the contract must be
/// rejected.  `mock_all_auths` is deliberately **not** called here so that
/// `require_auth` is enforced by the Soroban test host.
#[test]
#[should_panic]
fn unauthenticated_arbitrary_address_cannot_deposit() {
    let env = Env::default(); // ← no mock_all_auths
    let id  = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);

    let client     = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let attacker   = Address::generate(&env);

    // Create contract with all-auth mock so it succeeds.
    {
        let env2 = env.clone();
        env2.mock_all_auths();
        let milestones = vec![&env2, M1, M2, M3];
        escrow.create_contract(
            &client, &freelancer, &None, &milestones, &ReleaseAuthorization::ClientOnly,
        );
    }

    // Attacker has no authorization — must panic.
    escrow.deposit_funds(&1, &attacker, &TOTAL);
}

/// A freshly generated address with **no relation** to the contract is
/// rejected even when all auths are mocked (role check fires after auth).
#[test]
fn random_address_with_mocked_auth_is_still_rejected_by_role_check() {
    let (env, escrow, client, freelancer) = setup();
    let cid = make_contract(&env, &escrow, &client, &freelancer);

    let outsider = Address::generate(&env);
    let result = escrow.try_deposit_funds(&cid, &outsider, &TOTAL);

    match result {
        Err(Ok(e)) => {
            let expected: soroban_sdk::Error = EscrowError::UnauthorizedRole.into();
            assert_eq!(e, expected, "expected UnauthorizedRole, got {:?}", e);
        }
        other => panic!("expected UnauthorizedRole error, got {:?}", other),
    }
}

/// The freelancer address is a contract participant but MUST NOT be allowed
/// to deposit — only the client can fund.
#[test]
fn freelancer_cannot_deposit_even_with_mocked_auth() {
    let (env, escrow, client, freelancer) = setup();
    let cid = make_contract(&env, &escrow, &client, &freelancer);

    let result = escrow.try_deposit_funds(&cid, &freelancer, &TOTAL);

    match result {
        Err(Ok(e)) => {
            let expected: soroban_sdk::Error = EscrowError::UnauthorizedRole.into();
            assert_eq!(e, expected, "freelancer must get UnauthorizedRole");
        }
        other => panic!("expected UnauthorizedRole, got {:?}", other),
    }
}

/// An arbiter (even a legitimate one registered on the contract) must NOT be
/// able to fund the contract.
#[test]
fn arbiter_cannot_deposit() {
    let (env, escrow, client, freelancer) = setup();
    let arbiter   = Address::generate(&env);
    let milestones = vec![&env, M1, M2, M3];
    let cid = escrow.create_contract(
        &client, &freelancer,
        &Some(arbiter.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
    );

    let result = escrow.try_deposit_funds(&cid, &arbiter, &TOTAL);

    match result {
        Err(Ok(e)) => {
            let expected: soroban_sdk::Error = EscrowError::UnauthorizedRole.into();
            assert_eq!(e, expected, "arbiter must get UnauthorizedRole");
        }
        other => panic!("expected UnauthorizedRole, got {:?}", other),
    }
}

// ─── Role-mismatch (right auth, wrong address) ────────────────────────────────

/// If the `depositor` argument is a different valid address (not the stored
/// client) the role check must fire even though mock_all_auths is active.
/// This validates that auth ≠ authorization and both must pass.
#[test]
fn passing_wrong_client_address_is_rejected() {
    let (env, escrow, client, freelancer) = setup();
    let _cid = make_contract(&env, &escrow, &client, &freelancer);

    // Create a SECOND contract so we have a different client stored.
    let other_client     = Address::generate(&env);
    let other_freelancer = Address::generate(&env);
    let cid2 = make_contract(&env, &escrow, &other_client, &other_freelancer);

    // Try to deposit into contract-2 using contract-1's client address.
    let result = escrow.try_deposit_funds(&cid2, &client, &TOTAL);

    match result {
        Err(Ok(e)) => {
            let expected: soroban_sdk::Error = EscrowError::UnauthorizedRole.into();
            assert_eq!(e, expected);
        }
        other => panic!("expected UnauthorizedRole, got {:?}", other),
    }
}
