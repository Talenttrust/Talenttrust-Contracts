//! Tests for `simulate_deposit_funds` – a read-only preview of the deposit
//! outcome that runs the same validation as the real `deposit_funds` entrypoint
//! without executing the SAC transfer, writing storage, or emitting events.
//!
//! Coverage matrix:
//!
//! | Path                          | Positive cases | Negative cases |
//! |-------------------------------|---------------|----------------|
//! | `simulate_deposit_funds`      | matches real full deposit | unbound token rejected |
//! |                               | matches real partial deposit | non-client rejected |
//! |                               | idempotent (no state mutation) | non-positive amount rejected |
//! |                               | projected status correct | cancelled contract rejected |
//! |                               | — | refunded contract rejected |
//! |                               | — | invalid-state (Funded) rejected |
//! |                               | — | over-funding rejected |
//! |                               | — | not-initialized rejected |
//! |                               | — | paused rejected |
//! | State mutation                | simulation does not change contract state | — |
//! |                               | simulation does not move tokens | — |
//!
//! Run locally with `cargo test -p escrow --lib simulate_deposit`.

#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, Vec as SorobanVec,
};

use super::{
    assert_contract_error, total_milestone_amount, MILESTONE_ONE, MILESTONE_THREE, MILESTONE_TWO,
};
use crate::{ContractStatus, Error, EscrowError, ReleaseAuthorization};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Register the escrow contract, an SAC, initialize, bind settlement token.
fn setup_bound(env: &Env) -> (crate::EscrowClient<'_>, Address, Address) {
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);

    let sac = env.register_stellar_asset_contract(admin.clone());

    env.mock_all_auths_allowing_non_root_auth();
    client.initialize(&admin);
    client.bind_settlement_token(&admin, &sac);

    (client, sac, admin)
}

/// Mint `amount` SAC tokens to `holder` via the SAC admin client.
fn mint_to(env: &Env, sac: &Address, holder: &Address, amount: i128) {
    StellarAssetClient::new(env, sac).mint(holder, &amount);
}

/// Create a 3-milestone contract and return (client_addr, freelancer_addr, contract_id).
fn create_contract(env: &Env, client: &crate::EscrowClient<'_>) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = SorobanVec::from_slice(env, &[MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    (client_addr, freelancer_addr, id)
}

// ─── Positive cases ──────────────────────────────────────────────────────────

/// Simulating a full deposit must return the same projected outcome that a real
/// deposit produces (funded_amount and status).
#[test]
fn simulate_matches_real_full_deposit() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let total = total_milestone_amount();

    // Simulate the deposit first.
    let simulated = client.simulate_deposit_funds(&id, &client_addr, &total);
    assert_eq!(simulated.current_funded_amount, 0);
    assert_eq!(simulated.new_funded_amount, total);
    assert_eq!(simulated.projected_status, ContractStatus::Funded);
    assert_eq!(simulated.total_milestone_amount, total);

    // Now execute the real deposit.
    mint_to(&env, &sac, &client_addr, total);
    assert!(client.deposit_funds(&id, &client_addr, &total));

    // The real contract state must match the simulated projection.
    let contract = client.get_contract(&id);
    assert_eq!(contract.funded_amount, simulated.new_funded_amount);
    assert_eq!(contract.status, simulated.projected_status);
}

/// Simulating a partial deposit must return PartiallyFunded when the amount
/// is less than the total milestone sum.
#[test]
fn simulate_partial_deposit_returns_partially_funded() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let total = total_milestone_amount();
    let partial = total / 2;

    mint_to(&env, &sac, &client_addr, total);
    // Partially fund the contract so we're in PartiallyFunded state.
    assert!(client.deposit_funds(&id, &client_addr, &partial));

    // Simulate a second deposit that would bring it to full.
    let remainder = total - partial;
    let simulated = client.simulate_deposit_funds(&id, &client_addr, &remainder);
    assert_eq!(simulated.current_funded_amount, partial);
    assert_eq!(simulated.new_funded_amount, total);
    assert_eq!(simulated.projected_status, ContractStatus::Funded);
    assert_eq!(simulated.total_milestone_amount, total);

    // Execute the real remainder deposit and verify.
    assert!(client.deposit_funds(&id, &client_addr, &remainder));
    let contract = client.get_contract(&id);
    assert_eq!(contract.funded_amount, simulated.new_funded_amount);
    assert_eq!(contract.status, simulated.projected_status);
}

/// Simulating a deposit when already partially funded must project the correct
/// PartiallyFunded status if the new amount does not reach the total.
#[test]
fn simulate_from_partially_funded_stays_partial() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let total = total_milestone_amount();
    let partial = total / 2;
    let small_deposit = 100_0000000;

    mint_to(&env, &sac, &client_addr, total + small_deposit);
    assert!(client.deposit_funds(&id, &client_addr, &partial));

    let simulated = client.simulate_deposit_funds(&id, &client_addr, &small_deposit);
    assert_eq!(simulated.current_funded_amount, partial);
    assert_eq!(simulated.new_funded_amount, partial + small_deposit);
    assert_eq!(simulated.projected_status, ContractStatus::PartiallyFunded);
    assert_eq!(simulated.total_milestone_amount, total);
}

/// Multiple simulate calls must return the same result because the simulation
/// never mutates state.
#[test]
fn simulate_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, _sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let total = total_milestone_amount();

    let first = client.simulate_deposit_funds(&id, &client_addr, &total);
    let second = client.simulate_deposit_funds(&id, &client_addr, &total);
    assert_eq!(first, second);

    // Verify no state change occurred.
    let contract = client.get_contract(&id);
    assert_eq!(contract.funded_amount, 0);
    assert_eq!(contract.status, ContractStatus::Created);
}

// ─── No state mutation ───────────────────────────────────────────────────────

/// After a simulate call, token balances and contract state must be untouched.
#[test]
fn simulate_does_not_mutate_state() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let total = total_milestone_amount();

    mint_to(&env, &sac, &client_addr, total);

    // Record balances before simulation.
    let token = TokenClient::new(&env, &sac);
    let before_client = token.balance(&client_addr);
    let before_escrow = token.balance(&client.address);
    let before_contract = client.get_contract(&id);

    // Run the simulation.
    let _simulated = client.simulate_deposit_funds(&id, &client_addr, &total);

    // Assert no tokens moved.
    assert_eq!(token.balance(&client_addr), before_client);
    assert_eq!(token.balance(&client.address), before_escrow);

    // Assert no contract state changed.
    let after_contract = client.get_contract(&id);
    assert_eq!(after_contract.funded_amount, before_contract.funded_amount);
    assert_eq!(after_contract.status, before_contract.status);
    assert_eq!(
        after_contract.total_deposited,
        before_contract.total_deposited
    );
}

// ─── Negative cases ─────────────────────────────────────────────────────────

/// Simulate must reject when no settlement token has been bound.
#[test]
fn simulate_rejects_unbound_token() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert_contract_error(
        client.try_simulate_deposit_funds(&id, &client_addr, &100_i128),
        crate::Error::SettlementTokenNotConfigured,
    );

    // State must be unchanged.
    let contract = client.get_contract(&id);
    assert_eq!(contract.funded_amount, 0);
    assert_eq!(contract.status, ContractStatus::Created);
}

/// Simulate must reject when the caller is not the contract's client.
#[test]
fn simulate_rejects_non_client() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, _sac, _admin) = setup_bound(&env);
    let (_client_addr, freelancer_addr, id) = create_contract(&env, &client);
    let total = total_milestone_amount();

    assert_contract_error(
        client.try_simulate_deposit_funds(&id, &freelancer_addr, &total),
        Error::UnauthorizedRole,
    );
}

/// Simulate must reject non-positive amounts (same as real deposit).
#[test]
fn simulate_rejects_non_positive_amounts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, _sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);

    for amount in [0_i128, -1_i128] {
        assert_contract_error(
            client.try_simulate_deposit_funds(&id, &client_addr, &amount),
            Error::AmountMustBePositive,
        );
    }
}

/// Simulate must reject deposits on a cancelled contract.
#[test]
fn simulate_rejects_cancelled_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, _sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);

    // Cancel the contract (needs to be in Created state, no funds).
    assert!(client.cancel_contract(&id, &client_addr));

    assert_contract_error(
        client.try_simulate_deposit_funds(&id, &client_addr, &100_i128),
        EscrowError::ContractCancelled,
    );
}

/// Simulate must reject deposits on a refunded contract.
#[test]
fn simulate_rejects_refunded_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let total = total_milestone_amount();

    // Fund and then refund.
    mint_to(&env, &sac, &client_addr, total);
    assert!(client.deposit_funds(&id, &client_addr, &total));
    let indices = SorobanVec::from_slice(&env, &[0u32, 1, 2]);
    assert_eq!(client.refund_unreleased_milestones(&id, &indices), total);

    assert_contract_error(
        client.try_simulate_deposit_funds(&id, &client_addr, &100_i128),
        EscrowError::ContractRefunded,
    );
}

/// Simulate must reject when the contract is already fully funded (Funded state).
#[test]
fn simulate_rejects_funded_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let total = total_milestone_amount();

    mint_to(&env, &sac, &client_addr, total);
    assert!(client.deposit_funds(&id, &client_addr, &total));

    assert_contract_error(
        client.try_simulate_deposit_funds(&id, &client_addr, &1_i128),
        Error::InvalidState,
    );
}

/// Simulate must reject deposits that would exceed the total milestone amount.
#[test]
fn simulate_rejects_overfunding() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, _sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);
    let total = total_milestone_amount();

    assert_contract_error(
        client.try_simulate_deposit_funds(&id, &client_addr, &(total + 1)),
        Error::InvalidDepositAmount,
    );
}

/// Simulate must reject when the contract has not been initialized.
#[test]
fn simulate_rejects_uninitialized() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let _sac = env.register_stellar_asset_contract(admin.clone());
    // Note: not calling initialize.

    assert_contract_error(
        client.try_simulate_deposit_funds(&0u32, &admin, &100_i128),
        crate::Error::NotInitialized,
    );
}

/// Simulate must reject when the contract is paused.
#[test]
fn simulate_rejects_paused() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let (client, _sac, _admin) = setup_bound(&env);
    let (client_addr, _freelancer_addr, id) = create_contract(&env, &client);

    // Pause the contract.
    assert!(client.pause());

    assert_contract_error(
        client.try_simulate_deposit_funds(&id, &client_addr, &100_i128),
        Error::ContractPaused,
    );
}
