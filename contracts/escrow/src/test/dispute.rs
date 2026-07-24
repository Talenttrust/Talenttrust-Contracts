//! Dispute resolution payout arithmetic tests.
//!
//! These tests verify the pure money-splitting logic in `resolution_payouts`:
//!
//!   - FullRefund: all available → client (available, 0)
//!   - FullPayout: all available → freelancer (0, available)
//!   - PartialRefund: 70/30 split with floor rounding on freelancer leg
//!   - Split: custom split requiring sum == available, no negative amounts
//!
//! Conservation invariant: client_payout + freelancer_payout == available.
//!
//! ## Lifecycle & auth tests
//!
//! The second section covers raise/resolve lifecycle and auth boundaries:
//!
//!   - Only a party to the contract may raise a dispute
//!   - Only the designated arbiter may resolve it
//!   - Resolving moves funds/state correctly
//!   - Raising in a terminal state is rejected with typed error
//!   - Non-party raise is rejected
//!   - Non-arbiter resolve is rejected
//!   - Double-resolve is rejected
//!   - Resolve-after-settle is rejected

#![cfg(test)]

use crate::{
    Contract, ContractStatus, DisputeResolution, DisputeSplit, Error, Escrow, EscrowClient,
    ReleaseAuthorization,
};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::dispute::{final_status_after_resolution, resolution_payouts};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    client
}

/// Build a bare `Contract` value with controlled accounting fields for unit tests
/// that call `resolution_payouts` / `final_status_after_resolution` directly.
///
/// `funded` is stored in both `total_deposited` and `funded_amount` so the
/// helper reflects a freshly-funded contract with no prior releases.
fn payout_contract(env: &Env, funded: i128, released: i128, refunded: i128) -> Contract {
    Contract {
        client: Address::generate(env),
        freelancer: Address::generate(env),
        arbiter: Some(Address::generate(env)),
        status: ContractStatus::Disputed,
        total_deposited: funded,
        funded_amount: funded,
        released_amount: released,
        refunded_amount: refunded,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    }
}

/// Helper: create a funded contract with an arbiter, ready for dispute.
/// Returns (client_addr, freelancer_addr, arbiter_addr, contract_id).
fn funded_contract_with_arbiter(
    env: &Env,
    client: &EscrowClient<'_>,
) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    let milestones = vec![env, 100_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

/// Helper: create a funded contract without an arbiter.
/// Returns (client_addr, freelancer_addr, contract_id).
fn funded_contract_no_arbiter(env: &Env, client: &EscrowClient<'_>) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 100_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    (client_addr, freelancer_addr, contract_id)
}

/// Helper: create a funded contract with arbiter and raise a dispute.
/// Returns (client_addr, freelancer_addr, arbiter_addr, contract_id).
fn disputed_contract(env: &Env, client: &EscrowClient<'_>) -> (Address, Address, Address, u32) {
    let (client_addr, freelancer_addr, arbiter_addr, contract_id) =
        funded_contract_with_arbiter(env, client);
    assert!(client.raise_dispute(&contract_id, &client_addr));
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

// ---------------------------------------------------------------------------
// Unit tests: resolution_payouts (pure arithmetic)
// ---------------------------------------------------------------------------

#[test]
fn resolution_payouts_full_refund_routes_all_to_client() {
    let env = make_env();
    // available = 100 - 20 - 10 = 70
    let contract = payout_contract(&env, 100, 20, 10);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullRefund),
        Ok((70, 0))
    );
}

#[test]
fn resolution_payouts_full_payout_routes_all_to_freelancer() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 20, 10);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullPayout),
        Ok((0, 70))
    );
}

/// PartialRefund applies the documented 70/30 split with floor rounding.
/// freelancer gets floor(available * 30 / 100), client gets remainder.
#[test]
fn resolution_payouts_partial_refund_applies_floor_rounded_30_pct_to_freelancer() {
    let env = make_env();
    // 101 available: freelancer = floor(101 * 30 / 100) = 30; client = 71
    let contract = payout_contract(&env, 101, 0, 0);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::PartialRefund),
        Ok((71, 30))
    );
}

#[test]
fn resolution_payouts_split_accepts_exact_conserving_amounts() {
    let env = make_env();
    // Exact split amounts are accepted and returned verbatim.
    assert_eq!(
        resolution_payouts(
            &payout_contract(&env, 100, 0, 0),
            &DisputeResolution::Split(DisputeSplit {
                client_amount: 40,
                freelancer_amount: 60,
            })
        ),
        Ok((40, 60))
    );
    // One stroop → floor(1 * 30 / 100) = 0, client gets 1.
    assert_eq!(
        resolution_payouts(
            &payout_contract(&env, 1, 0, 0),
            &DisputeResolution::PartialRefund
        ),
        Ok((1, 0))
    );
}

/// Table-driven test covering PartialRefund rounding at odd amounts.
/// Verifies that floor truncation never creates value (sum == available).
#[test]
fn resolution_payouts_partial_refund_odd_amount_rounding() {
    let env = make_env();
    // (available, expected_client, expected_freelancer)
    let cases: &[(i128, i128, i128)] = &[
        (7, 5, 2),
        (10, 7, 3),
        (99, 70, 29),
        (100, 70, 30),
        (101, 71, 30),
        (102, 72, 30),
        (103, 73, 30),
    ];
    for (available, expected_client, expected_freelancer) in cases {
        let contract = payout_contract(&env, *available, 0, 0);
        let (client, freelancer) = resolution_payouts(&contract, &DisputeResolution::PartialRefund)
            .expect("PartialRefund should not error");
        assert_eq!(
            client + freelancer,
            *available,
            "sum must equal available for amount {}",
            available
        );
        assert_eq!(client, *expected_client);
        assert_eq!(freelancer, *expected_freelancer);
    }
}

/// Split rejects negative amounts.
#[test]
fn resolution_payouts_split_rejects_negative_legs() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: -1,
        freelancer_amount: 101,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
    let split = DisputeSplit {
        client_amount: 101,
        freelancer_amount: -1,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
}

#[test]
fn resolution_payouts_split_rejects_non_conserving_sum() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    // 40 + 59 = 99 ≠ 100
    let split = DisputeSplit {
        client_amount: 40,
        freelancer_amount: 59,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
    // 40 + 61 = 101 ≠ 100
    let split = DisputeSplit {
        client_amount: 40,
        freelancer_amount: 61,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
}

/// Split accepts any (a, b) where a + b == available and both are non-negative.
#[test]
fn resolution_payouts_split_accepts_exact_splits() {
    let env = make_env();
    let split = DisputeSplit {
        client_amount: 40,
        freelancer_amount: 60,
    };
    assert_eq!(
        resolution_payouts(
            &payout_contract(&env, 100, 0, 0),
            &DisputeResolution::Split(split)
        ),
        Ok((40, 60))
    );
    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: 0,
    };
    assert_eq!(
        resolution_payouts(
            &payout_contract(&env, 0, 0, 0),
            &DisputeResolution::Split(split)
        ),
        Ok((0, 0))
    );
}

/// Split uses checked addition and rejects overflow before the sum check.
#[test]
fn resolution_payouts_split_rejects_overflowing_sum() {
    let env = make_env();
    let contract = payout_contract(&env, i128::MAX, 0, 0);
    let split = DisputeSplit {
        client_amount: i128::MAX,
        freelancer_amount: 1,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::PotentialOverflow)
    );
}

/// Payout math fails closed when released + refunded already exceed funded_amount.
#[test]
fn resolution_payouts_rejects_corrupted_accounting_state() {
    let env = make_env();
    // released(70) + refunded(31) = 101 > funded(100) → available < 0
    let contract = payout_contract(&env, 100, 70, 31);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullRefund),
        Err(Error::AccountingInvariantViolated)
    );
}

/// Table-driven test verifying conservation invariant across all resolution variants.
#[test]
fn resolution_payouts_conserves_available_balance() {
    let env = make_env();
    let balances = &[0, 1, 2, 5, 10, 33, 99, 100, 101, 1000, 12345, 1000000];

    for &available in balances {
        let c = payout_contract(&env, available, 0, 0);

        // FullRefund
        let (client, freelancer) = resolution_payouts(&c, &DisputeResolution::FullRefund).unwrap();
        assert_eq!(client + freelancer, available);
        assert_eq!(client, available);
        assert_eq!(freelancer, 0);

        // FullPayout
        let (client, freelancer) = resolution_payouts(&c, &DisputeResolution::FullPayout).unwrap();
        assert_eq!(client + freelancer, available);
        assert_eq!(client, 0);
        assert_eq!(freelancer, available);

        // PartialRefund
        let (client, freelancer) =
            resolution_payouts(&c, &DisputeResolution::PartialRefund).unwrap();
        assert_eq!(client + freelancer, available);
        let expected_freelancer = (available * 30) / 100;
        assert_eq!(freelancer, expected_freelancer);
        assert_eq!(client, available - expected_freelancer);

        // Split (exact)
        let split_client = available / 2;
        let split_freelancer = available - split_client;
        let split = DisputeSplit {
            client_amount: split_client,
            freelancer_amount: split_freelancer,
        };
        let (client, freelancer) =
            resolution_payouts(&c, &DisputeResolution::Split(split)).unwrap();
        assert_eq!(client + freelancer, available);
        assert_eq!(client, split_client);
        assert_eq!(freelancer, split_freelancer);
    }
}

/// final_status returns Refunded only when the full deposit has been refunded.
#[test]
fn final_status_after_resolution_returns_refunded_only_when_fully_refunded() {
    let env = make_env();
    assert_eq!(
        final_status_after_resolution(&payout_contract(&env, 100, 0, 100)),
        ContractStatus::Refunded
    );
    assert_eq!(
        final_status_after_resolution(&payout_contract(&env, 100, 30, 70)),
        ContractStatus::Completed
    );
}

// ---------------------------------------------------------------------------
// Integration tests: resolution variants
// ---------------------------------------------------------------------------

/// Integration: FullRefund on a funded contract conserves balance and marks Refunded.
#[test]
fn resolve_full_refund_conserves_and_marks_refunded() {
    let env = make_env();
    let client = make_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 125_i128, 75_i128];
    let escrow_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&escrow_id, &client_addr, &200_i128);

    client.raise_dispute(&escrow_id, &client_addr);
    client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund);

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Refunded);
    assert_eq!(contract.released_amount, 0);
    assert_eq!(contract.refunded_amount, 200);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

/// Integration: FullPayout on a funded contract conserves balance and marks Completed.
#[test]
fn resolve_full_payout_conserves_and_marks_completed() {
    let env = make_env();
    let client = make_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 150_i128];
    let escrow_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&escrow_id, &client_addr, &150_i128);

    client.raise_dispute(&escrow_id, &client_addr);
    client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullPayout);

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, 150);
    assert_eq!(contract.refunded_amount, 0);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

/// Integration: PartialRefund applies 70/30 split and conserves balance.
#[test]
fn resolve_partial_refund_conserves_70_30_split() {
    let env = make_env();
    let client = make_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 100_i128];
    let escrow_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&escrow_id, &client_addr, &100_i128);

    client.raise_dispute(&escrow_id, &client_addr);
    client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::PartialRefund);

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

/// Integration: Split accepts valid custom amounts and conserves balance.
#[test]
fn resolve_split_conserves_custom_amounts() {
    let env = make_env();
    let client = make_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = soroban_sdk::vec![&env, 40_i128, 60_i128];
    let escrow_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&escrow_id, &client_addr, &100_i128);

    client.raise_dispute(&escrow_id, &client_addr);
    let split = DisputeSplit {
        client_amount: 35,
        freelancer_amount: 65,
    };
    client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::Split(split));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.refunded_amount, 35);
    assert_eq!(contract.released_amount, 65);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

// ---------------------------------------------------------------------------
// Lifecycle & auth tests: raise/resolve boundaries
// ---------------------------------------------------------------------------

/// Client can raise a dispute on a funded contract with an arbiter.
#[test]
fn raise_dispute_by_client_succeeds() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);

    assert!(client.raise_dispute(&contract_id, &client_addr));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// Freelancer can also raise a dispute on a funded contract with an arbiter.
#[test]
fn raise_dispute_by_freelancer_succeeds() {
    let env = make_env();
    let client = make_client(&env);
    let (_, freelancer_addr, _, contract_id) = funded_contract_with_arbiter(&env, &client);

    assert!(client.raise_dispute(&contract_id, &freelancer_addr));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// A non-party (random address) cannot raise a dispute.
#[test]
fn raise_dispute_by_non_party_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);
    let outsider = Address::generate(&env);

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &outsider),
        Error::PartyNotAuthorized,
    );
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Funded
    );
}

/// Raising a dispute on a contract without an arbiter is rejected.
#[test]
fn raise_dispute_without_arbiter_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, contract_id) = funded_contract_no_arbiter(&env, &client);

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::ArbiterRequired,
    );
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Funded
    );
}

/// Raising a dispute on a contract in the Completed (terminal) state is rejected
/// with a typed error.
#[test]
fn raise_dispute_on_completed_contract_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let milestones = vec![&env, 100_i128];
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    // Release the only milestone to reach Completed state.
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Completed
    );

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::InvalidState,
    );
}

/// The designated arbiter can resolve a dispute.
#[test]
fn resolve_dispute_by_arbiter_succeeds() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund,));
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Refunded);
    assert_eq!(contract.refunded_amount, 100);
}

/// A non-arbiter address cannot resolve a dispute.
#[test]
fn resolve_dispute_by_non_arbiter_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, _, contract_id) = disputed_contract(&env, &client);
    let outsider = Address::generate(&env);

    // Client is a party but not the arbiter — must be rejected.
    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &client_addr, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
    // Random outsider is rejected by require_party.
    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &outsider, &DisputeResolution::FullRefund),
        Error::PartyNotAuthorized,
    );
    // Contract remains in Disputed state.
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// After resolving a dispute, a second resolve is rejected (double-resolve).
#[test]
fn double_resolve_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    // First resolution succeeds.
    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund,));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Refunded
    );

    // Second resolution must fail — contract is no longer Disputed.
    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullPayout),
        Error::InvalidStatusTransition,
    );
}

/// After all milestones are released (settled), raise_dispute is rejected
/// because the contract is in Completed state, not Funded/PartiallyFunded.
#[test]
fn raise_dispute_after_settle_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let milestones = vec![&env, 50_i128, 50_i128];
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    // Release all milestones to settle the contract.
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &1));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Completed
    );

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::InvalidState,
    );
}

/// The assigned arbiter cannot resolve a dispute when the contract is not in
/// `Disputed` state; the status guard must fire first and return
/// `InvalidStatusTransition`.
#[test]
fn resolve_dispute_by_arbiter_when_not_disputed_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let milestones = vec![&env, 100_i128];
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Funded
    );

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

/// Raising a dispute on a contract that is already `Disputed` is rejected with
/// `InvalidState`. The first successful raise must emit the `dispute opened`
/// event; the second attempt must fail without changing state.
#[test]
fn raise_dispute_when_already_disputed_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);

    assert!(client.raise_dispute(&contract_id, &client_addr));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );

    let events = env.events().all();
    assert!(
        events.iter().any(|event| event.0 == symbol_short!("dispute")),
        "expected dispute opened event after first raise"
    );

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::InvalidState,
    );
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// Resolving a dispute moves funds: FullPayout sets released_amount and marks
/// Completed; FullRefund sets refunded_amount and marks Refunded. Verify that
/// the accounting is correct after each resolution type.
#[test]
fn resolve_moves_funds_correctly_full_payout() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _freelancer_addr, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullPayout,));
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, 100);
    assert_eq!(contract.refunded_amount, 0);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

#[test]
fn resolve_moves_funds_correctly_full_refund() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund,));
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Refunded);
    assert_eq!(contract.released_amount, 0);
    assert_eq!(contract.refunded_amount, 100);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

/// Raising a dispute on a contract in the Refunded (terminal) state is rejected.
#[test]
fn raise_dispute_on_refunded_contract_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr, contract_id) =
        funded_contract_with_arbiter(&env, &client);

    // Resolve as FullRefund to reach Refunded terminal state.
    assert!(client.raise_dispute(&contract_id, &client_addr));
    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund,));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Refunded
    );

    // Cannot raise again.
    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &freelancer_addr),
        Error::AlreadyFinalized,
    );
}

/// Resolving after the contract has been finalized is rejected with AlreadyFinalized.
#[test]
fn resolve_after_finalize_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::AlreadyFinalized,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
//  RAISE_DISPUTE — FULL BOUNDARY AND REJECTION MATRIX
// ═══════════════════════════════════════════════════════════════════════════════

// ── Guard 1: NotInitialized ──────────────────────────────────────────────────

#[test]
fn raise_dispute_rejects_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_id);
    let caller = Address::generate(&env);

    super::assert_contract_error(
        client.try_raise_dispute(&0_u32, &caller),
        Error::NotInitialized,
    );
}

// ── Guard 2: Pause / Emergency rails ─────────────────────────────────────────

#[test]
fn raise_dispute_rejects_when_paused() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (client_addr, _, arbiter_addr, contract_id) = funded_contract_with_arbiter(&env, &client);

    client.pause();

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::ContractPaused,
    );
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Funded
    );
}

#[test]
fn raise_dispute_rejects_when_emergency_active() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (client_addr, _, arbiter_addr, contract_id) = funded_contract_with_arbiter(&env, &client);

    client.activate_emergency_pause();

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::EmergencyActive,
    );
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Funded
    );
}

// ── Guard 3: ContractNotFound ────────────────────────────────────────────────

#[test]
fn raise_dispute_rejects_unknown_contract_id() {
    let env = make_env();
    let client = make_client(&env);
    let caller = Address::generate(&env);

    super::assert_contract_error(
        client.try_raise_dispute(&999_u32, &caller),
        Error::ContractNotFound,
    );
}

// ── Guard 4: AlreadyFinalized ────────────────────────────────────────────────

fn set_finalized(env: &Env, escrow_addr: &Address, contract_id: u32) {
    use crate::types::DataKey;
    env.as_contract(escrow_addr, || {
        env.storage()
            .persistent()
            .set(&DataKey::Finalization(contract_id), &true);
    });
}

#[test]
fn raise_dispute_rejects_when_finalized() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);

    set_finalized(&env, &escrow, contract_id);

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::AlreadyFinalized,
    );
}

// ── Guard 5: UnauthorizedRole (not client nor freelancer) ────────────────────

#[test]
fn raise_dispute_rejects_arbiter_as_caller() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = funded_contract_with_arbiter(&env, &client);

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &arbiter_addr),
        Error::UnauthorizedRole,
    );
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Funded
    );
}

#[test]
fn raise_dispute_rejects_random_stranger_caller() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);
    let stranger = Address::generate(&env);

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &stranger),
        Error::UnauthorizedRole,
    );
}

// ── Guard 6: ArbiterRequired ─────────────────────────────────────────────────
// (covered by raise_dispute_without_arbiter_is_rejected, retained for matrix)

// ── Guard 7: InvalidState for every non-disputable status ────────────────────

fn force_status(env: &Env, escrow_addr: &Address, contract_id: u32, status: ContractStatus) {
    use crate::types::DataKey;
    env.as_contract(escrow_addr, || {
        let key = DataKey::Contract(contract_id);
        let mut contract: Contract = env.storage().persistent().get(&key).unwrap();
        contract.status = status;
        env.storage().persistent().set(&key, &contract);
    });
}

#[test]
fn raise_dispute_rejects_created_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::InvalidState,
    );
}

#[test]
fn raise_dispute_rejects_accepted_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (client_addr, _, arbiter_addr, contract_id) = funded_contract_with_arbiter(&env, &client);

    force_status(&env, &escrow, contract_id, ContractStatus::Accepted);

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::InvalidState,
    );
}

#[test]
fn raise_dispute_rejects_cancelled_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);

    force_status(&env, &escrow, contract_id, ContractStatus::Cancelled);

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        Error::InvalidState,
    );
}

#[test]
fn raise_dispute_accepts_partially_funded_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 50_i128, 50_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &client_addr, &50_i128);
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::PartiallyFunded
    );

    assert!(client.raise_dispute(&contract_id, &client_addr));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

// ── Event emission: exact topic pair + payload ───────────────────────────────

fn has_dispute_event_exact(env: &Env, sub_topic: &str, expected_id: u32) -> bool {
    use soroban_sdk::{Symbol, TryFromVal};
    let sub = Symbol::new(env, sub_topic);
    env.events().all().iter().any(|event| {
        let topics = &event.1;
        if topics.len() < 2 {
            return false;
        }
        let t0 = Symbol::try_from_val(env, &topics.get(0).unwrap()).ok();
        let t1 = Symbol::try_from_val(env, &topics.get(1).unwrap()).ok();
        if t0.as_ref() != Some(&symbol_short!("dispute")) || t1.as_ref() != Some(&sub) {
            return false;
        }
        let data = &event.2;
        if data.len() < 2 {
            return false;
        }
        let got_id: Option<u32> = data.get(0).and_then(|v| v.try_into_val(env).ok());
        got_id == Some(expected_id)
    })
}

#[test]
fn raise_dispute_emits_dispute_opened_event_with_correct_payload() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, _, contract_id) = funded_contract_with_arbiter(&env, &client);

    assert!(client.raise_dispute(&contract_id, &client_addr));

    assert!(
        has_dispute_event_exact(&env, "opened", contract_id),
        "expected (dispute, opened) event with matching contract_id"
    );
}

#[test]
fn raise_dispute_rejection_does_not_emit_event() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = funded_contract_with_arbiter(&env, &client);

    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &arbiter_addr),
        Error::UnauthorizedRole,
    );

    let dispute_topic = symbol_short!("dispute");
    let any_dispute = env.events().all().iter().any(|event| {
        let topics = &event.1;
        !topics.is_empty()
            && soroban_sdk::Symbol::try_from_val(&env, &topics.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&dispute_topic)
    });
    assert!(!any_dispute, "rejected raise must not publish any dispute event");
}

// ═══════════════════════════════════════════════════════════════════════════════
//  RESOLVE_DISPUTE — FULL BOUNDARY AND REJECTION MATRIX
// ═══════════════════════════════════════════════════════════════════════════════

// ── Guard 1: NotInitialized ──────────────────────────────────────────────────

#[test]
fn resolve_dispute_rejects_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let escrow_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow_id);
    let arbiter = Address::generate(&env);

    super::assert_contract_error(
        client.try_resolve_dispute(&0_u32, &arbiter, &DisputeResolution::FullRefund),
        Error::NotInitialized,
    );
}

// ── Guard 2: Pause / Emergency rails ─────────────────────────────────────────

#[test]
fn resolve_dispute_rejects_when_paused() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    client.pause();

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::ContractPaused,
    );
}

#[test]
fn resolve_dispute_rejects_when_emergency_active() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    client.activate_emergency_pause();

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::EmergencyActive,
    );
}

// ── Guard 3: ContractNotFound ────────────────────────────────────────────────

#[test]
fn resolve_dispute_rejects_unknown_contract_id() {
    let env = make_env();
    let client = make_client(&env);
    let arbiter = Address::generate(&env);

    super::assert_contract_error(
        client.try_resolve_dispute(&999_u32, &arbiter, &DisputeResolution::FullRefund),
        Error::ContractNotFound,
    );
}

// ── Guard 4: AlreadyFinalized ────────────────────────────────────────────────

#[test]
fn resolve_dispute_rejects_when_finalized() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    set_finalized(&env, &escrow, contract_id);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::AlreadyFinalized,
    );
}

// ── Guard 5: InvalidStatusTransition for every non-Disputed status ───────────

#[test]
fn resolve_dispute_rejects_created_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 100_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

#[test]
fn resolve_dispute_rejects_accepted_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    force_status(&env, &escrow, contract_id, ContractStatus::Accepted);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

#[test]
fn resolve_dispute_rejects_funded_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (_, _, arbiter_addr, contract_id) = funded_contract_with_arbiter(&env, &client);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

#[test]
fn resolve_dispute_rejects_partially_funded_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 50_i128, 50_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&contract_id, &client_addr, &50_i128);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

#[test]
fn resolve_dispute_rejects_completed_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    force_status(&env, &escrow, contract_id, ContractStatus::Completed);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

#[test]
fn resolve_dispute_rejects_cancelled_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    force_status(&env, &escrow, contract_id, ContractStatus::Cancelled);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

#[test]
fn resolve_dispute_rejects_refunded_status() {
    let env = make_env();
    let escrow = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &escrow);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    force_status(&env, &escrow, contract_id, ContractStatus::Refunded);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::InvalidStatusTransition,
    );
}

// ── Guard 6: UnauthorizedRole (caller is not the assigned arbiter) ──────────

#[test]
fn resolve_dispute_rejects_client_caller() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, _, contract_id) = disputed_contract(&env, &client);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &client_addr, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
}

#[test]
fn resolve_dispute_rejects_freelancer_caller() {
    let env = make_env();
    let client = make_client(&env);
    let (_, freelancer_addr, _, contract_id) = disputed_contract(&env, &client);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &freelancer_addr, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
}

#[test]
fn resolve_dispute_rejects_stranger_caller() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, _, contract_id) = disputed_contract(&env, &client);
    let stranger = Address::generate(&env);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &stranger, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
}

#[test]
fn resolve_dispute_rejects_different_arbiter_address() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, _, contract_id) = disputed_contract(&env, &client);
    let other_arbiter = Address::generate(&env);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &other_arbiter, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );
}

// ── Event emission: resolved event with exact code ───────────────────────────

fn has_resolved_event_with_code(env: &Env, expected_id: u32, expected_code: u32) -> bool {
    use soroban_sdk::{Symbol, TryFromVal};
    let sub = Symbol::new(env, "resolved");
    env.events().all().iter().any(|event| {
        let topics = &event.1;
        if topics.len() < 2 {
            return false;
        }
        let t0 = Symbol::try_from_val(env, &topics.get(0).unwrap()).ok();
        let t1 = Symbol::try_from_val(env, &topics.get(1).unwrap()).ok();
        if t0.as_ref() != Some(&symbol_short!("dispute")) || t1.as_ref() != Some(&sub) {
            return false;
        }
        let data = &event.2;
        if data.len() < 2 {
            return false;
        }
        let got_id: Option<u32> = data.get(0).and_then(|v| v.try_into_val(env).ok());
        let got_code: Option<u32> = data.get(1).and_then(|v| v.try_into_val(env).ok());
        got_id == Some(expected_id) && got_code == Some(expected_code)
    })
}

#[test]
fn resolve_dispute_emits_resolved_event_full_refund_code_0() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund));
    assert!(
        has_resolved_event_with_code(&env, contract_id, 0),
        "expected resolved event with code 0 (FullRefund)"
    );
}

#[test]
fn resolve_dispute_emits_resolved_event_partial_refund_code_1() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::PartialRefund));
    assert!(
        has_resolved_event_with_code(&env, contract_id, 1),
        "expected resolved event with code 1 (PartialRefund)"
    );
}

#[test]
fn resolve_dispute_emits_resolved_event_full_payout_code_2() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullPayout));
    assert!(
        has_resolved_event_with_code(&env, contract_id, 2),
        "expected resolved event with code 2 (FullPayout)"
    );
}

#[test]
fn resolve_dispute_emits_resolved_event_split_code_3() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);
    let split = DisputeSplit {
        client_amount: 40,
        freelancer_amount: 60,
    };

    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::Split(split)));
    assert!(
        has_resolved_event_with_code(&env, contract_id, 3),
        "expected resolved event with code 3 (Split)"
    );
}

#[test]
fn resolve_dispute_rejection_does_not_emit_event() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, _, contract_id) = disputed_contract(&env, &client);

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &client_addr, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
    );

    let resolved_topic = soroban_sdk::Symbol::new(&env, "resolved");
    let any_resolved = env.events().all().iter().any(|event| {
        let topics = &event.1;
        topics.len() >= 2
            && soroban_sdk::Symbol::try_from_val(&env, &topics.get(1).unwrap())
                .ok()
                .as_ref()
                == Some(&resolved_topic)
    });
    assert!(!any_resolved, "rejected resolve must not publish resolved event");
}

// ═══════════════════════════════════════════════════════════════════════════════
//  SPLIT RESOLUTION — EXACT ARITHMETIC BOUNDARIES
// ═══════════════════════════════════════════════════════════════════════════════

// ── Exactly-at-boundary cases: accept ────────────────────────────────────────

#[test]
fn split_accepts_boundary_client_equals_available_freelancer_zero() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 100,
        freelancer_amount: 0,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Ok((100, 0))
    );
}

#[test]
fn split_accepts_boundary_client_zero_freelancer_equals_available() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: 100,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Ok((0, 100))
    );
}

#[test]
fn split_accepts_boundary_minimal_available_one_stroop() {
    let env = make_env();
    let contract = payout_contract(&env, 1, 0, 0);
    let split = DisputeSplit {
        client_amount: 1,
        freelancer_amount: 0,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Ok((1, 0))
    );
    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: 1,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Ok((0, 1))
    );
}

#[test]
fn split_accepts_degenerate_zero_available_both_legs_zero() {
    let env = make_env();
    let contract = payout_contract(&env, 0, 0, 0);
    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: 0,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Ok((0, 0))
    );
}

// ── One-over-boundary cases: reject with InvalidDisputeSplit ─────────────────

#[test]
fn split_rejects_client_one_over_available() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 101,
        freelancer_amount: 0,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
}

#[test]
fn split_rejects_freelancer_one_over_available() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: 101,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
}

#[test]
fn split_rejects_each_leg_at_available_sum_double_available() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 100,
        freelancer_amount: 100,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
}

#[test]
fn split_rejects_sum_one_under_available() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 50,
        freelancer_amount: 49,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
}

#[test]
fn split_rejects_sum_one_over_available() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 50,
        freelancer_amount: 51,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
}

// ── PartialRefund boundaries at extreme amounts ──────────────────────────────

#[test]
fn partial_refund_zero_available_returns_zero_zero() {
    let env = make_env();
    let contract = payout_contract(&env, 0, 0, 0);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::PartialRefund),
        Ok((0, 0))
    );
}

#[test]
fn full_refund_zero_available_returns_zero_zero() {
    let env = make_env();
    let contract = payout_contract(&env, 0, 0, 0);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullRefund),
        Ok((0, 0))
    );
}

#[test]
fn full_payout_zero_available_returns_zero_zero() {
    let env = make_env();
    let contract = payout_contract(&env, 0, 0, 0);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullPayout),
        Ok((0, 0))
    );
}

#[test]
fn split_rejects_negative_one_client_even_if_sum_matches() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: -1,
        freelancer_amount: 101,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
}

#[test]
fn split_rejects_negative_one_freelancer_even_if_sum_matches() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 0, 0);
    let split = DisputeSplit {
        client_amount: 101,
        freelancer_amount: -1,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(split)),
        Err(Error::InvalidDisputeSplit)
    );
}
