#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    vec, Address, Env,
};

use crate::{DataKey, Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Create an initialized escrow client with mocked auth.
/// Returns (client, admin, contract_id).
fn setup(env: &Env) -> (EscrowClient<'_>, Address, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    // Advance ledger to sequence 1 so that LastFeeWithdrawalLedger is
    // stored as a non-zero value, enabling cooldown enforcement.
    env.ledger().set(LedgerInfo {
        sequence_number: 1,
        timestamp: 1000,
        ..env.ledger().get()
    });
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(env, &cid);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin, cid)
}

/// Creates a funded contract with accumulated protocol fees (100 stroops at 10 %).
fn setup_with_accumulated_fees(env: &Env) -> (EscrowClient<'_>, Address, Address, i128, Address) {
    let (client, admin, _cid) = setup(env);
    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);
    client.set_protocol_fee_bps(&1000u32, &1u64);

    let client_addr = Address::generate(env);
    let freelancer = Address::generate(env);
    let milestones = vec![env, 1_000_i128];

    let contract_id = client.create_contract(
        &client_addr,
        &freelancer,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Mint tokens to client before deposit
    let token_asset = soroban_sdk::token::StellarAssetClient::new(env, &token);
    token_asset.mint(&client_addr, &1_000_i128);

    client.deposit_funds(&contract_id, &client_addr, &1_000_i128);
    client.approve_milestone_release(&contract_id, &client_addr, &0);
    client.release_milestone(&contract_id, &client_addr, &0);

    let accumulated: i128 = 100;
    let destination = Address::generate(env);
    (client, admin, destination, accumulated, token)
}

/// Advance the ledger by `delta` sequence numbers and corresponding time.
fn advance_ledgers(env: &Env, delta: u32) {
    let info = env.ledger().get();
    env.ledger().set(LedgerInfo {
        sequence_number: info.sequence_number + delta,
        timestamp: info.timestamp + (delta as u64) * 5,
        protocol_version: info.protocol_version,
        network_id: info.network_id,
        base_reserve: info.base_reserve,
        min_temp_entry_ttl: info.min_temp_entry_ttl,
        min_persistent_entry_ttl: info.min_persistent_entry_ttl,
        max_entry_ttl: info.max_entry_ttl,
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
// Default values
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fee_withdrawal_cap_defaults_to_5000() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert_eq!(client.get_fee_withdrawal_cap(), 5_000u32);
}

#[test]
fn fee_withdrawal_cooldown_defaults_to_17280() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert_eq!(client.get_fee_withdrawal_cooldown(), 17_280u32);
}

#[test]
fn last_fee_withdrawal_ledger_defaults_to_zero() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert_eq!(client.get_last_fee_withdrawal_ledger(), 0u32);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Governance: set_fee_withdrawal_cap
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn set_fee_withdrawal_cap_accepts_zero() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert!(client.set_fee_withdrawal_cap(&0u32));
    assert_eq!(client.get_fee_withdrawal_cap(), 0);
}

#[test]
fn set_fee_withdrawal_cap_accepts_10000() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert!(client.set_fee_withdrawal_cap(&10_000u32));
    assert_eq!(client.get_fee_withdrawal_cap(), 10_000);
}

#[test]
fn set_fee_withdrawal_cap_rejects_10001() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    super::assert_contract_error(
        client.try_set_fee_withdrawal_cap(&10_001u32),
        Error::InvalidProtocolParameters,
    );
    assert_eq!(client.get_fee_withdrawal_cap(), 5_000u32);
}

#[test]
fn set_fee_withdrawal_cap_rejects_when_uninitialized() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    super::assert_contract_error(
        client.try_set_fee_withdrawal_cap(&1_000u32),
        Error::NotInitialized,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Governance: set_fee_withdrawal_cooldown
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn set_fee_withdrawal_cooldown_accepts_zero() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert!(client.set_fee_withdrawal_cooldown(&0u32));
    assert_eq!(client.get_fee_withdrawal_cooldown(), 0);
}

#[test]
fn set_fee_withdrawal_cooldown_accepts_max() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    assert!(client.set_fee_withdrawal_cooldown(&2_592_000u32));
    assert_eq!(client.get_fee_withdrawal_cooldown(), 2_592_000);
}

#[test]
fn set_fee_withdrawal_cooldown_rejects_over_max() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    super::assert_contract_error(
        client.try_set_fee_withdrawal_cooldown(&2_592_001u32),
        Error::InvalidProtocolParameters,
    );
    assert_eq!(client.get_fee_withdrawal_cooldown(), 17_280u32);
}

#[test]
fn set_fee_withdrawal_cooldown_rejects_when_uninitialized() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    super::assert_contract_error(
        client.try_set_fee_withdrawal_cooldown(&3_600u32),
        Error::NotInitialized,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cap enforcement
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn withdraw_within_cap_succeeds() {
    let env = Env::default();
    let (client, _admin, destination, _acc, _tok) = setup_with_accumulated_fees(&env);
    // Default cap 50 % of 100 = 50
    assert!(client.withdraw_protocol_fees(&50_i128, &destination));
    assert_eq!(client.get_accumulated_protocol_fees(), 50);
}

#[test]
fn withdraw_exceeding_cap_rejected() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    // 51 > 50 % of 100
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&51_i128, &destination),
        EscrowError::FeeWithdrawalCapExceeded,
    );
    // Accumulated must be unchanged
    assert_eq!(client.get_accumulated_protocol_fees(), acc);
}

#[test]
fn withdraw_with_cap_disabled() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    client.set_fee_withdrawal_cap(&0u32);
    assert!(client.withdraw_protocol_fees(&acc, &destination));
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

#[test]
fn withdraw_with_cap_at_100_percent() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    client.set_fee_withdrawal_cap(&10_000u32);
    assert!(client.withdraw_protocol_fees(&acc, &destination));
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

#[test]
fn withdraw_cap_ceiling_division_2_passes() {
    // max_allowed = ceiling(100 * 50 / 10000) = ceiling(0.5) = 1
    let env = Env::default();
    let (client, _admin, _dest, _acc, _tok) = setup_with_accumulated_fees(&env);

    // Set cap to 50 bps (0.5%) to demonstrate ceiling division
    client.set_fee_withdrawal_cap(&50u32);

    // 1 ≤ ceiling(0.5) → passes
    let dest = Address::generate(&env);
    assert!(client.withdraw_protocol_fees(&1_i128, &dest));
}

#[test]
fn withdraw_cap_ceiling_division_3_fails() {
    // max_allowed = ceiling(100 * 50 / 10000) = ceiling(0.5) = 1
    // So 2 should fail with cap exceeded
    let env = Env::default();
    let (client, _admin, _dest, _acc, _tok) = setup_with_accumulated_fees(&env);

    // Set cap to 50 bps → max = 1
    client.set_fee_withdrawal_cap(&50u32);

    // Reset the last withdrawal ledger so cooldown doesn't interfere
    // (it was set by the first withdrawal, but setup_with_accumulated_fees doesn't withdraw)
    // 2 > ceiling(0.5) = 1 → fails
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&2_i128, &Address::generate(&env)),
        EscrowError::FeeWithdrawalCapExceeded,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cooldown enforcement
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn first_withdrawal_succeeds_no_cooldown() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    let amount = acc / 2;
    assert!(client.withdraw_protocol_fees(&amount, &destination));
    // Ledger starts at 1, so last withdrawal ledger should be 1
    assert_eq!(client.get_last_fee_withdrawal_ledger(), 1u32);
}

#[test]
fn second_withdrawal_within_cooldown_rejected() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    // Set small cooldown so default 17280 doesn't block first withdrawal check
    client.set_fee_withdrawal_cooldown(&100u32);

    // First withdrawal
    assert!(client.withdraw_protocol_fees(&(acc / 2), &destination));

    // Second within cooldown
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&1_i128, &destination),
        EscrowError::FeeWithdrawalCooldownActive,
    );
}

#[test]
fn withdrawal_after_cooldown_succeeds() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    client.set_fee_withdrawal_cooldown(&10u32);

    // First withdrawal
    let amount: i128 = acc / 2;
    assert!(client.withdraw_protocol_fees(&amount, &destination));

    // Should fail immediately
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&1_i128, &destination),
        EscrowError::FeeWithdrawalCooldownActive,
    );

    // Advance past cooldown
    advance_ledgers(&env, 11);
    assert!(client.withdraw_protocol_fees(&1_i128, &destination));
}

#[test]
fn withdrawal_with_cooldown_disabled() {
    let env = Env::default();
    let (client, _admin, destination, _acc, _tok) = setup_with_accumulated_fees(&env);
    client.set_fee_withdrawal_cooldown(&0u32);
    client.set_fee_withdrawal_cap(&10_000u32);

    assert!(client.withdraw_protocol_fees(&50_i128, &destination));
    assert!(client.withdraw_protocol_fees(&50_i128, &destination));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cooldown boundary edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn withdraw_exactly_at_cooldown_boundary() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    client.set_fee_withdrawal_cooldown(&10u32);
    assert!(client.withdraw_protocol_fees(&(acc / 2), &destination));

    // diff == cooldown, NOT < cooldown → succeeds
    advance_ledgers(&env, 10);
    assert!(client.withdraw_protocol_fees(&1_i128, &destination));
}

#[test]
fn withdraw_one_ledger_before_cooldown_boundary() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    client.set_fee_withdrawal_cooldown(&10u32);
    assert!(client.withdraw_protocol_fees(&(acc / 2), &destination));

    advance_ledgers(&env, 9);
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&1_i128, &destination),
        EscrowError::FeeWithdrawalCooldownActive,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Combined cap + cooldown
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn cap_and_cooldown_work_together() {
    let env = Env::default();
    let (client, _admin, destination, _acc, _tok) = setup_with_accumulated_fees(&env);
    client.set_fee_withdrawal_cooldown(&10u32);

    // First withdrawal: 50 (at 50 % cap of 100)
    assert!(client.withdraw_protocol_fees(&50_i128, &destination));
    assert_eq!(client.get_accumulated_protocol_fees(), 50);

    // Second withdrawal within cooldown → cooldown error
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&25_i128, &destination),
        EscrowError::FeeWithdrawalCooldownActive,
    );

    // Advance past cooldown
    advance_ledgers(&env, 11);

    // Now cap on remaining 50: max = 25. Try 26 → cap error
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&26_i128, &destination),
        EscrowError::FeeWithdrawalCapExceeded,
    );

    // Within both limits → succeeds
    assert!(client.withdraw_protocol_fees(&25_i128, &destination));
    assert_eq!(client.get_accumulated_protocol_fees(), 25);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Exact accounting
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn partial_withdrawal_keeps_exact_accounting() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    // Use small cooldown for fast testing
    client.set_fee_withdrawal_cooldown(&10u32);

    let first: i128 = 50;
    assert!(client.withdraw_protocol_fees(&first, &destination));
    assert_eq!(client.get_accumulated_protocol_fees(), acc - first);

    // Advance past cooldown
    advance_ledgers(&env, 11);

    // To withdraw the rest, disable cap (test is about exact accounting, not cap)
    client.set_fee_withdrawal_cap(&10_000u32);
    let second: i128 = acc - first;
    assert!(client.withdraw_protocol_fees(&second, &destination));
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Pause enforcement
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn withdraw_rejected_when_paused() {
    let env = Env::default();
    let (client, _admin, destination, _acc, _tok) = setup_with_accumulated_fees(&env);
    client.pause(&1u64);
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&1_i128, &destination),
        EscrowError::ContractPaused,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Insufficient accumulated fees
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn withdraw_rejects_more_than_accumulated() {
    let env = Env::default();
    let (client, _admin, _dest, acc, _tok) = setup_with_accumulated_fees(&env);
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&(acc + 1), &Address::generate(&env)),
        EscrowError::InsufficientAccumulatedFees,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Amount validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn withdraw_rejects_zero_amount() {
    let env = Env::default();
    let (client, _admin, _dest, _acc, _tok) = setup_with_accumulated_fees(&env);
    super::assert_contract_error(
        client.try_withdraw_protocol_fees(&0_i128, &Address::generate(&env)),
        EscrowError::AmountMustBePositive,
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Getter consistency
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rate_limit_getters_consistent() {
    let env = Env::default();
    let (client, _, _) = setup(&env);

    assert_eq!(client.get_fee_withdrawal_cap(), 5_000);
    assert_eq!(client.get_fee_withdrawal_cooldown(), 17_280);
    assert_eq!(client.get_last_fee_withdrawal_ledger(), 0);

    client.set_fee_withdrawal_cap(&2_500u32);
    client.set_fee_withdrawal_cooldown(&3_600u32);

    assert_eq!(client.get_fee_withdrawal_cap(), 2_500);
    assert_eq!(client.get_fee_withdrawal_cooldown(), 3_600);
    assert_eq!(client.get_last_fee_withdrawal_ledger(), 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Workflow: multiple withdrawal cycles
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn multiple_withdrawals_cycle_with_cooldown() {
    let env = Env::default();
    let (client, _admin, destination, acc, _tok) = setup_with_accumulated_fees(&env);
    client.set_fee_withdrawal_cooldown(&5u32);

    let mut remaining = acc;

    // First cycle: 50 (at 50% cap), remaining = 50
    assert!(client.withdraw_protocol_fees(&50_i128, &destination));
    remaining -= 50;
    advance_ledgers(&env, 6);

    // Second cycle: cap on 50 = 25, withdraw 25, remaining = 25
    assert!(client.withdraw_protocol_fees(&25_i128, &destination));
    remaining -= 25;
    advance_ledgers(&env, 6);

    // Third cycle: disable cap to drain remaining 25
    client.set_fee_withdrawal_cap(&10_000u32);
    assert!(client.withdraw_protocol_fees(&25_i128, &destination));
    remaining -= 25;

    assert_eq!(remaining, 0);
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}
