#![cfg(test)]

use crate::dispute::{final_status_after_resolution, resolution_payouts};
use crate::{
    ContractStatus, DisputeResolution, Escrow, EscrowClient, EscrowError, ReleaseAuthorization,
};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

fn setup_initialized() -> (Env, EscrowClient<'static>) {
    panic!(
        "setup_initialized returns a static lifetime which is unsound; use make_env_client instead"
    )
}

/// Create a fresh initialized escrow environment.
fn make_env_client() -> (Env, EscrowClient<'static>) {
    panic!(
        "Cannot return EscrowClient<'static>; restructure callers to not store client across env"
    )
}

fn new_client(env: &Env) -> EscrowClient<'_> {
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(env, &cid);
    let admin = Address::generate(env);
    client.initialize(&admin);
    client
}

fn create_funded_contract_with_arbiter(
    env: &Env,
    client: &EscrowClient,
    milestones: soroban_sdk::Vec<i128>,
    deposit_amount: i128,
) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&contract_id, &client_addr, &deposit_amount));

    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

/// Build a Contract value for unit-testing `resolution_payouts` / `final_status_after_resolution`
/// without going through the full escrow entrypoints.
fn payout_contract(env: &Env, funded: i128, released: i128, refunded: i128) -> crate::Contract {
    let dummy = Address::generate(env);
    crate::Contract {
        client: dummy.clone(),
        freelancer: dummy.clone(),
        arbiter: None,
        status: ContractStatus::Disputed,
        funded_amount: funded,
        released_amount: released,
        refunded_amount: refunded,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    }
}

/// Verifies FullRefund conserves all available balance for the client.
#[test]
fn resolution_payouts_full_refund_returns_available_to_client() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 20, 10);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullRefund),
        Ok((70, 0))
    );
}

/// Verifies FullPayout conserves all available balance for the freelancer.
#[test]
fn resolution_payouts_full_payout_returns_available_to_freelancer() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 20, 10);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullPayout),
        Ok((0, 70))
    );
}

/// Verifies PartialRefund applies the documented 70/30 split with floor rounding.
#[test]
fn resolution_payouts_partial_refund_uses_floor_rounded_70_30_split() {
    let env = Env::default();
    let contract = payout_contract(&env, 101, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::PartialRefund),
        Ok((71, 30))
    );
}

/// Verifies PartialRefund handles zero and one-stroop balances without creating value.
#[test]
fn resolution_payouts_partial_refund_handles_rounding_boundaries() {
    let env = Env::default();
    let zero_available = payout_contract(&env, 0, 0, 0);
    let one_stroop_available = payout_contract(&env, 1, 0, 0);

    assert_eq!(
        resolution_payouts(&zero_available, &DisputeResolution::PartialRefund),
        Ok((0, 0))
    );
    assert_eq!(
        resolution_payouts(&one_stroop_available, &DisputeResolution::PartialRefund),
        Ok((1, 0))
    );
}

/// Verifies Split rejects negative client or freelancer payouts.
#[test]
fn resolution_payouts_split_rejects_negative_legs() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(-1, 101)),
        Err(EscrowError::InvalidDisputeSplit)
    );
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(101, -1)),
        Err(EscrowError::InvalidDisputeSplit)
    );
}

/// Verifies Split rejects under-sized and oversized sums that do not equal available balance.
#[test]
fn resolution_payouts_split_rejects_non_conserving_sums() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(40, 59)),
        Err(EscrowError::InvalidDisputeSplit)
    );
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(40, 61)),
        Err(EscrowError::InvalidDisputeSplit)
    );
}

/// Verifies Split accepts exact conservation, including zero available balance.
#[test]
fn resolution_payouts_split_accepts_exact_splits() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 0, 0);
    let zero_available = payout_contract(&env, 0, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(40, 60)),
        Ok((40, 60))
    );
    assert_eq!(
        resolution_payouts(&zero_available, &DisputeResolution::Split(0, 0)),
        Ok((0, 0))
    );
}

/// Verifies Split uses checked addition and rejects overflowing payout sums.
#[test]
fn resolution_payouts_split_rejects_overflowing_sum() {
    let env = Env::default();
    let contract = payout_contract(&env, i128::MAX, 0, 0);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(i128::MAX, 1)),
        Err(EscrowError::PotentialOverflow)
    );
}

/// Verifies payout math fails closed when released and refunded amounts exceed deposits.
#[test]
fn resolution_payouts_rejects_accounting_invariant_violation() {
    let env = Env::default();
    let contract = payout_contract(&env, 100, 70, 31);

    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::FullRefund),
        Err(EscrowError::AccountingInvariantViolated)
    );
}

/// Verifies final status is Refunded only when the full deposit has been refunded.
#[test]
fn final_status_after_resolution_marks_refunded_only_for_full_refund() {
    let env = Env::default();
    let fully_refunded = payout_contract(&env, 100, 0, 100);
    let partially_refunded = payout_contract(&env, 100, 30, 70);

    assert_eq!(
        final_status_after_resolution(&fully_refunded),
        ContractStatus::Refunded
    );
    assert_eq!(
        final_status_after_resolution(&partially_refunded),
        ContractStatus::Completed
    );
}

#[test]
fn client_can_raise_dispute_on_funded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = new_client(&env);
    let (client_addr, _, _, escrow_id) = create_funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128, 200_i128],
        300_i128,
    );

    assert!(client.raise_dispute(&escrow_id, &client_addr));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Disputed);
}

#[test]
fn freelancer_can_raise_dispute_on_funded_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = new_client(&env);
    let (_, freelancer_addr, _, escrow_id) = create_funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 100_i128, 200_i128],
        300_i128,
    );

    assert!(client.raise_dispute(&escrow_id, &freelancer_addr));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Disputed);
}

#[test]
fn raise_dispute_requires_contract_party() {
    let (env, _contract_id, client) = setup_initialized();
    let (_, _, _, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    let outsider = Address::generate(&env);
    super::assert_contract_error(
        client.try_raise_dispute(&escrow_id, &outsider),
        EscrowError::UnauthorizedRole,
    );
}

#[test]
fn raise_dispute_requires_assigned_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let client = new_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create contract WITHOUT arbiter
    let escrow_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    assert!(client.deposit_funds(&escrow_id, &client_addr, &100_i128));

    super::assert_contract_error(
        client.try_raise_dispute(&escrow_id, &client_addr),
        EscrowError::ArbiterRequired,
    );
}

#[test]
fn raise_dispute_rejects_completed_contract() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, _, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    // Release milestone and complete
    assert!(client.approve_milestone_release(&escrow_id, &client_addr, &0));
    assert!(client.release_milestone(&escrow_id, &client_addr, &0));

    super::assert_contract_error(
        client.try_raise_dispute(&escrow_id, &client_addr),
        EscrowError::InvalidState,
    );
}

#[test]
fn resolve_full_refund_marks_refunded_and_closes_accounting() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 125_i128, 75_i128], 200_i128);

    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Refunded);
    assert_eq!(contract.released_amount, 0);
    assert_eq!(contract.refunded_amount, 200);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

#[test]
fn resolve_full_payout_marks_completed_and_closes_accounting() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 150_i128], 150_i128);

    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullPayout));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, 150);
    assert_eq!(contract.refunded_amount, 0);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

#[test]
fn resolve_partial_refund_applies_70_30_split() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::PartialRefund));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    // 70% refund to client, 30% release to freelancer
    assert_eq!(contract.refunded_amount, 70);
    assert_eq!(contract.released_amount, 30);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

#[test]
fn resolve_partial_refund_applies_to_remaining_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let client = new_client(&env);
    let (client_addr, _, arbiter_addr, escrow_id) = create_funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 101_i128, 100_i128],
        201_i128,
    );

    // Release first milestone
    assert!(client.approve_milestone_release(&escrow_id, &client_addr, &0));
    assert!(client.release_milestone(&escrow_id, &client_addr, &0));

    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::PartialRefund));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    // Initial release: 101
    // Remaining: 100 → 70% refund (70), 30% release (30)
    assert_eq!(contract.released_amount, 131); // 101 + 30
    assert_eq!(contract.refunded_amount, 70);
    assert_eq!(
        contract.released_amount + contract.refunded_amount,
        contract.funded_amount
    );
}

#[test]
fn resolve_split_accepts_custom_amounts_that_match_available_balance() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 40_i128, 60_i128], 100_i128);

    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::Split(35, 65)));

    let contract = client.get_contract(&escrow_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.refunded_amount, 35);
    assert_eq!(contract.released_amount, 65);
}

#[test]
fn resolve_split_rejects_invalid_totals() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    assert!(client.raise_dispute(&escrow_id, &client_addr));

    // Split doesn't match available balance
    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::Split(30, 50)),
        EscrowError::InvalidDisputeSplit,
    );
}

#[test]
fn resolve_split_rejects_negative_amounts() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    assert!(client.raise_dispute(&escrow_id, &client_addr));

    super::assert_contract_error(
        client.try_resolve_dispute(
            &escrow_id,
            &arbiter_addr,
            &DisputeResolution::Split(-10, 110),
        ),
        EscrowError::InvalidDisputeSplit,
    );
}

#[test]
fn resolve_dispute_requires_assigned_arbiter() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, _, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    assert!(client.raise_dispute(&escrow_id, &client_addr));

    let outsider = Address::generate(&env);
    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &outsider, &DisputeResolution::FullPayout),
        EscrowError::UnauthorizedRole,
    );
}

#[test]
fn resolve_dispute_rejects_non_disputed_contract() {
    let (env, _contract_id, client) = setup_initialized();
    let (_, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    // Try to resolve without raising dispute first
    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund),
        EscrowError::InvalidStatusTransition,
    );
}

#[test]
fn resolve_dispute_cannot_be_called_twice() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund));

    // Try to resolve again
    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullPayout),
        EscrowError::InvalidStatusTransition,
    );
}

#[test]
fn pause_blocks_raise_dispute() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, _, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    assert!(client.pause());

    super::assert_contract_error(
        client.try_raise_dispute(&escrow_id, &client_addr),
        EscrowError::ContractPaused,
    );
}

#[test]
fn pause_blocks_resolve_dispute() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert!(client.pause());

    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund),
        EscrowError::ContractPaused,
    );
}

#[test]
fn emergency_blocks_raise_and_resolve_dispute() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    assert!(client.activate_emergency_pause());

    super::assert_contract_error(
        client.try_raise_dispute(&escrow_id, &client_addr),
        EscrowError::EmergencyActive,
    );

    // Resolve emergency, raise dispute, then emergency again
    assert!(client.resolve_emergency());
    assert!(client.raise_dispute(&escrow_id, &client_addr));
    assert!(client.activate_emergency_pause());

    super::assert_contract_error(
        client.try_resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund),
        EscrowError::EmergencyActive,
    );
}

#[test]
fn dispute_accounting_invariants_hold() {
    let env = Env::default();
    env.mock_all_auths();
    let client = new_client(&env);
    let (client_addr, _, arbiter_addr, escrow_id) = create_funded_contract_with_arbiter(
        &env,
        &client,
        vec![&env, 50_i128, 30_i128, 20_i128],
        100_i128,
    );

    // Release one milestone
    assert!(client.approve_milestone_release(&escrow_id, &client_addr, &0));
    assert!(client.release_milestone(&escrow_id, &client_addr, &0));

    let before_dispute = client.get_contract(&escrow_id);
    assert_eq!(before_dispute.released_amount, 50);
    assert_eq!(before_dispute.refunded_amount, 0);

    // Raise dispute
    assert!(client.raise_dispute(&escrow_id, &client_addr));

    // Resolve with split: remaining 50 → 20 refund, 30 release
    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::Split(20, 30)));

    let after_dispute = client.get_contract(&escrow_id);
    assert_eq!(after_dispute.released_amount, 80); // 50 + 30
    assert_eq!(after_dispute.refunded_amount, 20);
    assert_eq!(after_dispute.funded_amount, 100);
    assert_eq!(
        after_dispute.released_amount + after_dispute.refunded_amount,
        after_dispute.funded_amount
    );
}

#[test]
fn multiple_disputes_on_different_contracts() {
    let (env, _contract_id, client) = setup_initialized();

    // Create two contracts
    let (client1, _, arbiter1, escrow1) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    let (client2, _, arbiter2, escrow2) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 200_i128], 200_i128);

    // Raise and resolve disputes independently
    assert!(client.raise_dispute(&escrow1, &client1));
    assert!(client.raise_dispute(&escrow2, &client2));

    assert!(client.resolve_dispute(&escrow1, &arbiter1, &DisputeResolution::FullRefund));
    assert!(client.resolve_dispute(&escrow2, &arbiter2, &DisputeResolution::FullPayout));

    let contract1 = client.get_contract(&escrow1);
    let contract2 = client.get_contract(&escrow2);

    assert_eq!(contract1.status, ContractStatus::Refunded);
    assert_eq!(contract1.refunded_amount, 100);

    assert_eq!(contract2.status, ContractStatus::Completed);
    assert_eq!(contract2.released_amount, 200);
}

#[test]
fn dispute_events_are_emitted() {
    let (env, _contract_id, client) = setup_initialized();
    let (client_addr, _, arbiter_addr, escrow_id) =
        create_funded_contract_with_arbiter(&env, &client, vec![&env, 100_i128], 100_i128);

    // Raise dispute
    assert!(client.raise_dispute(&escrow_id, &client_addr));

    // Check for dispute opened event
    let events = env.events().all();
    let dispute_opened = events.iter().any(|e| {
        if let Some((topics, _data)) = e.try_into() {
            topics
                == (
                    soroban_sdk::symbol_short!("dispute"),
                    soroban_sdk::symbol_short!("opened"),
                )
        } else {
            false
        }
    });
    assert!(dispute_opened, "dispute opened event not found");

    // Resolve dispute
    assert!(client.resolve_dispute(&escrow_id, &arbiter_addr, &DisputeResolution::FullRefund));

    // Check for dispute resolved event
    let events = env.events().all();
    let dispute_resolved = events.iter().any(|e| {
        if let Some((topics, _data)) = e.try_into() {
            topics
                == (
                    soroban_sdk::symbol_short!("dispute"),
                    soroban_sdk::symbol_short!("resolved"),
                )
        } else {
            false
        }
    });
    assert!(dispute_resolved, "dispute resolved event not found");
}
