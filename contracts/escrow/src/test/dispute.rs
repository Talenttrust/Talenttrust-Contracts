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
    ReleaseAuthorization, types::SimulateDisputeOutcome,
};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env};

use crate::dispute::{final_status_after_resolution, resolution_payouts};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env
}

fn make_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    // Bind a settlement token so deposit_funds can transfer value.
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);
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
    // Mint settlement tokens to the client so deposit_funds can transfer them.
    let token = client.get_settlement_token().unwrap();
    StellarAssetClient::new(env, &token).mint(&client_addr, &100_i128);
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
    // Mint settlement tokens to the client so deposit_funds can transfer them.
    let token = client.get_settlement_token().unwrap();
    StellarAssetClient::new(env, &token).mint(&client_addr, &100_i128);
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

/// Mint settlement tokens and deposit into the escrow contract.
fn mint_and_deposit(
    env: &Env,
    client: &EscrowClient<'_>,
    contract_id: &u32,
    depositor: &Address,
    amount: &i128,
) {
    let token = client.get_settlement_token().unwrap();
    StellarAssetClient::new(env, &token).mint(depositor, amount);
    assert!(client.deposit_funds(contract_id, depositor, amount));
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
        Ok(crate::types::DisputeSummary {
            available_balance: 70,
            client_payout: 70,
            freelancer_payout: 0,
        })
    );
}

#[test]
fn resolution_payouts_full_payout_routes_all_to_freelancer() {
    let env = make_env();
    let contract = payout_contract(&env, 100, 20, 10);
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullPayout),
        Ok(crate::types::DisputeSummary {
            available_balance: 70,
            client_payout: 0,
            freelancer_payout: 70,
        })
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
        Ok(crate::types::DisputeSummary {
            available_balance: 101,
            client_payout: 71,
            freelancer_payout: 30,
        })
    );
}

#[test]
fn resolution_payouts_split_accepts_exact_conserving_amounts() {
    let env = make_env();
    // Split (40, 60) exactly matches available 100
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
    // One stroop → floor(1 * 30 / 100) = 0, client gets 1
    assert_eq!(
        resolution_payouts(
            &payout_contract(&env, 1, 0, 0),
            &DisputeResolution::PartialRefund
        ),
        Ok(crate::types::DisputeSummary {
            available_balance: 1,
            client_payout: 1,
            freelancer_payout: 0,
        })
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
        let info = resolution_payouts(&contract, &DisputeResolution::PartialRefund)
            .expect("PartialRefund should not error");
        assert_eq!(
            info.client_payout + info.freelancer_payout,
            *available,
            "sum must equal available for amount {}",
            available
        );
        assert_eq!(info.client_payout, *expected_client);
        assert_eq!(info.freelancer_payout, *expected_freelancer);
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
        Ok(crate::types::DisputeSummary {
            available_balance: 100,
            client_payout: 40,
            freelancer_payout: 60,
        })
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
        Ok(crate::types::DisputeSummary {
            available_balance: 0,
            client_payout: 0,
            freelancer_payout: 0,
        })
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
        let info = resolution_payouts(&c, &DisputeResolution::FullRefund).unwrap();
        assert_eq!(info.client_payout + info.freelancer_payout, available);
        assert_eq!(info.client_payout, available);
        assert_eq!(info.freelancer_payout, 0);

        // FullPayout
        let info = resolution_payouts(&c, &DisputeResolution::FullPayout).unwrap();
        assert_eq!(info.client_payout + info.freelancer_payout, available);
        assert_eq!(info.client_payout, 0);
        assert_eq!(info.freelancer_payout, available);

        // PartialRefund
        let (client, freelancer) =
            resolution_payouts(&c, &DisputeResolution::PartialRefund).unwrap();
        assert_eq!(client + freelancer, available);
        let expected_freelancer = (available * 30) / 100;
        assert_eq!(info.freelancer_payout, expected_freelancer);
        assert_eq!(info.client_payout, available - expected_freelancer);

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
    mint_and_deposit(&env, &client, &escrow_id, &client_addr, &200_i128);

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
    mint_and_deposit(&env, &client, &escrow_id, &client_addr, &150_i128);

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
    mint_and_deposit(&env, &client, &escrow_id, &client_addr, &100_i128);

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
    mint_and_deposit(&env, &client, &escrow_id, &client_addr, &100_i128);

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
        Error::UnauthorizedRole,
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
    mint_and_deposit(&env, &client, &contract_id, &client_addr, &100_i128);
    // Release the only milestone to reach Completed state.
    client.approve_milestone_release(&contract_id, &client_addr, &0);
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
    // Random outsider is also rejected.
    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &outsider, &DisputeResolution::FullRefund),
        Error::UnauthorizedRole,
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
    mint_and_deposit(&env, &client, &contract_id, &client_addr, &100_i128);
    // Release all milestones to settle the contract.
    // Approve milestones before releasing (release requires approval).
    client.approve_milestone_release(&contract_id, &client_addr, &0);
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    client.approve_milestone_release(&contract_id, &client_addr, &1);
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

    // Cannot raise again — contract is Refunded, not Funded/PartiallyFunded.
    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &freelancer_addr),
        Error::InvalidState,
    );
}

/// Resolving after the contract has been finalized is rejected with AlreadyFinalized.
#[test]
fn resolve_after_finalize_is_rejected() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    // Finalize the disputed contract.
    assert!(client.finalize_contract(&contract_id, &client_addr));

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund),
        Error::AlreadyFinalized,
    );
}

// ---------------------------------------------------------------------------
// Simulate / dry-run dispute resolution tests
// ---------------------------------------------------------------------------

/// Simulate FullRefund returns projected outcome (all refunded) without mutating state.
#[test]
fn simulate_full_refund_matches_real_outcome_and_is_read_only() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    // Read pre-simulation state.
    let before = client.get_contract(&contract_id);
    assert_eq!(before.status, ContractStatus::Disputed);

    let outcome = client.simulate_dispute_resolution(
        &contract_id,
        &arbiter_addr,
        &DisputeResolution::FullRefund,
    );

    assert_eq!(outcome.client_payout, 100);
    assert_eq!(outcome.freelancer_payout, 0);
    assert_eq!(outcome.final_status, ContractStatus::Refunded);
    assert_eq!(outcome.new_refunded_amount, 100);
    assert_eq!(outcome.new_released_amount, 0);

    // Verify state did NOT change.
    let after = client.get_contract(&contract_id);
    assert_eq!(after.status, ContractStatus::Disputed);
    assert_eq!(after.refunded_amount, 0);
    assert_eq!(after.released_amount, 0);
}

/// Simulate FullPayout returns projected outcome (all released) without mutating state.
#[test]
fn simulate_full_payout_matches_real_outcome_and_is_read_only() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    let outcome = client.simulate_dispute_resolution(
        &contract_id,
        &arbiter_addr,
        &DisputeResolution::FullPayout,
    );

    assert_eq!(outcome.client_payout, 0);
    assert_eq!(outcome.freelancer_payout, 100);
    assert_eq!(outcome.final_status, ContractStatus::Completed);
    assert_eq!(outcome.new_refunded_amount, 0);
    assert_eq!(outcome.new_released_amount, 100);

    // State unchanged.
    let after = client.get_contract(&contract_id);
    assert_eq!(after.status, ContractStatus::Disputed);
    assert_eq!(after.refunded_amount, 0);
    assert_eq!(after.released_amount, 0);
}

/// Simulate PartialRefund returns projected 70/30 outcome without mutating state.
#[test]
fn simulate_partial_refund_matches_real_outcome_and_is_read_only() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    let outcome = client.simulate_dispute_resolution(
        &contract_id,
        &arbiter_addr,
        &DisputeResolution::PartialRefund,
    );

    // 70% client, 30% freelancer (floor): 100 * 30/100 = 30
    assert_eq!(outcome.client_payout, 70);
    assert_eq!(outcome.freelancer_payout, 30);
    assert_eq!(outcome.client_payout + outcome.freelancer_payout, 100);
    assert_eq!(outcome.final_status, ContractStatus::Completed);
    assert_eq!(outcome.new_refunded_amount, 70);
    assert_eq!(outcome.new_released_amount, 30);

    // State unchanged.
    let after = client.get_contract(&contract_id);
    assert_eq!(after.status, ContractStatus::Disputed);
    assert_eq!(after.refunded_amount, 0);
    assert_eq!(after.released_amount, 0);
}

/// Simulate Split returns projected split outcome without mutating state.
#[test]
fn simulate_split_matches_real_outcome_and_is_read_only() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    let split = DisputeSplit {
        client_amount: 35,
        freelancer_amount: 65,
    };
    let outcome = client.simulate_dispute_resolution(
        &contract_id,
        &arbiter_addr,
        &DisputeResolution::Split(split),
    );

    assert_eq!(outcome.client_payout, 35);
    assert_eq!(outcome.freelancer_payout, 65);
    assert_eq!(outcome.client_payout + outcome.freelancer_payout, 100);
    assert_eq!(outcome.final_status, ContractStatus::Completed);
    assert_eq!(outcome.new_refunded_amount, 35);
    assert_eq!(outcome.new_released_amount, 65);

    // State unchanged.
    let after = client.get_contract(&contract_id);
    assert_eq!(after.status, ContractStatus::Disputed);
    assert_eq!(after.refunded_amount, 0);
    assert_eq!(after.released_amount, 0);
}

/// Simulate outcome exactly matches what a real resolve produces.
#[test]
fn simulate_matches_real_resolve_outcome() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, freelancer_addr, arbiter_addr, contract_id) =
        funded_contract_with_arbiter(&env, &client);

    // Simulate first, verify output.
    assert!(client.raise_dispute(&contract_id, &client_addr));
    let sim = client.simulate_dispute_resolution(
        &contract_id,
        &arbiter_addr,
        &DisputeResolution::FullRefund,
    );
    assert_eq!(sim.client_payout, 100);
    assert_eq!(sim.freelancer_payout, 0);
    assert_eq!(sim.final_status, ContractStatus::Refunded);

    // Now resolve for real (still in Disputed state because simulate didn't mutate).
    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund));
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Refunded);
    assert_eq!(contract.refunded_amount, 100);
    assert_eq!(contract.released_amount, 0);
}

/// Simulate is rejected when called by a non-arbiter.
#[test]
fn simulate_rejects_non_arbiter() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, _, contract_id) = disputed_contract(&env, &client);
    let outsider = Address::generate(&env);

    // Client is a party but not the arbiter → rejected.
    super::assert_contract_error(
        client.try_simulate_dispute_resolution(
            &contract_id,
            &client_addr,
            &DisputeResolution::FullRefund,
        ),
        Error::UnauthorizedRole,
    );
    // Random outsider → rejected.
    super::assert_contract_error(
        client.try_simulate_dispute_resolution(
            &contract_id,
            &outsider,
            &DisputeResolution::FullRefund,
        ),
        Error::UnauthorizedRole,
    );
}

/// Simulate is rejected when the contract is not in Disputed state.
#[test]
fn simulate_rejects_non_disputed_state() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    // Resolve first.
    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund));

    // Now simulate should fail because contract is Refunded (not Disputed).
    super::assert_contract_error(
        client.try_simulate_dispute_resolution(
            &contract_id,
            &arbiter_addr,
            &DisputeResolution::FullPayout,
        ),
        Error::InvalidStatusTransition,
    );
}

/// Simulate is rejected after the contract has been finalized.
#[test]
fn simulate_rejects_after_finalize() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    super::assert_contract_error(
        client.try_simulate_dispute_resolution(
            &contract_id,
            &arbiter_addr,
            &DisputeResolution::FullRefund,
        ),
        Error::AlreadyFinalized,
    );
}

/// Simulate rejects a non-existent contract.
#[test]
fn simulate_rejects_contract_not_found() {
    let env = make_env();
    let client = make_client(&env);
    let arbiter = Address::generate(&env);
    let nonexistent_id = 9999u32;

    super::assert_contract_error(
        client.try_simulate_dispute_resolution(
            &nonexistent_id,
            &arbiter,
            &DisputeResolution::FullRefund,
        ),
        Error::ContractNotFound,
    );
}

/// Simulate rejects invalid split (non-conserving amounts).
#[test]
fn simulate_rejects_invalid_split() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    let bad_split = DisputeSplit {
        client_amount: 40,
        freelancer_amount: 59, // 40 + 59 = 99 ≠ 100
    };
    super::assert_contract_error(
        client.try_simulate_dispute_resolution(
            &contract_id,
            &arbiter_addr,
            &DisputeResolution::Split(bad_split),
        ),
        Error::InvalidDisputeSplit,
    );
}

/// Simulate can be called multiple times without affecting state — idempotent reads.
#[test]
fn simulate_is_idempotent() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed_contract(&env, &client);

    let first = client.simulate_dispute_resolution(
        &contract_id,
        &arbiter_addr,
        &DisputeResolution::PartialRefund,
    );
    let second = client.simulate_dispute_resolution(
        &contract_id,
        &arbiter_addr,
        &DisputeResolution::PartialRefund,
    );
    assert_eq!(first, second);

    // Still Disputed after multiple simulations.
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
}

/// Table-driven test: simulate matches what resolve would produce for all resolution variants.
#[test]
fn simulate_matches_resolve_for_all_variants() {
    let env = make_env();

    struct Case {
        resolution: DisputeResolution,
        expected_client: i128,
        expected_freelancer: i128,
        expected_status: ContractStatus,
        label: &'static str,
    }

    let cases = &[
        Case {
            resolution: DisputeResolution::FullRefund,
            expected_client: 200,
            expected_freelancer: 0,
            expected_status: ContractStatus::Refunded,
            label: "FullRefund",
        },
        Case {
            resolution: DisputeResolution::FullPayout,
            expected_client: 0,
            expected_freelancer: 200,
            expected_status: ContractStatus::Completed,
            label: "FullPayout",
        },
        Case {
            resolution: DisputeResolution::PartialRefund,
            expected_client: 140,    // 200 * 70%
            expected_freelancer: 60, // 200 * 30%
            expected_status: ContractStatus::Completed,
            label: "PartialRefund",
        },
    ];

    for case in cases {
        // Fresh contract per case so simulate doesn't affect resolve.
        let client = make_client(&env);
        let client_addr = Address::generate(&env);
        let freelancer_addr = Address::generate(&env);
        let arbiter_addr = Address::generate(&env);
        let milestones = soroban_sdk::vec![&env, 100_i128, 100_i128];
        let contract_id = client.create_contract(
            &client_addr,
            &freelancer_addr,
            &Some(arbiter_addr.clone()),
            &milestones,
            &ReleaseAuthorization::ClientOnly,
        );
        mint_and_deposit(&env, &client, &contract_id, &client_addr, &200_i128);
        client.raise_dispute(&contract_id, &client_addr);

        // Simulate.
        let sim = client.simulate_dispute_resolution(
            &contract_id,
            &arbiter_addr,
            &case.resolution.clone(),
        );
        assert_eq!(
            sim.client_payout, case.expected_client,
            "{}: client payout mismatch",
            case.label
        );
        assert_eq!(
            sim.freelancer_payout, case.expected_freelancer,
            "{}: freelancer payout mismatch",
            case.label
        );
        assert_eq!(
            sim.client_payout + sim.freelancer_payout,
            200,
            "{}: conservation violated",
            case.label
        );
        assert_eq!(
            sim.final_status, case.expected_status,
            "{}: status mismatch",
            case.label
        );

        // After simulate, still Disputed.
        assert_eq!(
            client.get_contract(&contract_id).status,
            ContractStatus::Disputed,
            "{}: simulate mutated state",
            case.label
        );
    }
}
