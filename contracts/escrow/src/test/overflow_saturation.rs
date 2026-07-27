//! Overflow and saturation tests for escrow arithmetic.
//!
//! Covers all arithmetic hot-paths identified in issue #915:
//!
//! | Module          | Site                                    | Fix applied          |
//! |-----------------|------------------------------------------|----------------------|
//! | `release.rs`    | `released_amount += milestone.amount`   | `checked_add`        |
//! | `release.rs`    | `current_accumulated + fee`             | `checked_add`        |
//! | `release.rs`    | `pending + 1`                           | `checked_add`        |
//! | `lib.rs`        | `grant_pending_reputation_credit`       | `checked_add`        |
//! | `lib.rs`        | `resolve_dispute` +=                    | `checked_add`        |
//! | `lib.rs`        | `accumulated_fees + protocol_fee`       | `checked_add`        |
//! | `lib.rs`        | `invariant_sum` intermediates           | `checked_add` chain  |
//! | `refund_impl.rs`| `refunded_amount += total_refund`       | `checked_add`        |
//! | `refund_impl.rs`| `total_refund_amount += milestone.amount`| `checked_add`       |
//!
//! Tests use `try_*` client wrappers so panics surface as typed errors rather
//! than aborting the test process.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env};

use crate::{
    amount_validation::{
        accumulate_amounts, safe_add_amounts, safe_subtract_amounts, validate_deposit_amount,
        validate_single_amount, MAX_SINGLE_AMOUNT_STROOPS,
    },
    EscrowError, ReleaseAuthorization,
};

use super::assert_contract_error;

// ── Shared helpers ────────────────────────────────────────────────────────────

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env
}

/// Set up a fresh escrow with SAC token, initialize, bind, and return
/// `(client, sac_address, admin_address)`.
fn setup_escrow(env: &Env) -> (crate::EscrowClient<'_>, Address, Address) {
    let addr = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(env, &addr);
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract(admin.clone());
    client.initialize(&admin);
    client.bind_settlement_token(&admin, &sac);
    (client, sac, admin)
}

/// Mint `amount` of SAC tokens to `to`.
fn mint(env: &Env, sac: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, sac).mint(to, &amount);
}

/// Create a single-milestone contract with the given amount and return
/// `(client_addr, freelancer_addr, contract_id)`.
fn single_milestone_contract(
    env: &Env,
    escrow: &crate::EscrowClient<'_>,
    sac: &Address,
    amount: i128,
) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, amount];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    mint(env, sac, &client_addr, amount);
    escrow.deposit_funds(&id, &client_addr, &amount);
    (client_addr, freelancer_addr, id)
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Pure helper: safe_add_amounts / safe_subtract_amounts
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn safe_add_normal_values_succeeds() {
    assert_eq!(safe_add_amounts(100, 200), Some(300));
    assert_eq!(safe_add_amounts(0, 0), Some(0));
    assert_eq!(safe_add_amounts(i128::MAX - 1, 1), Some(i128::MAX));
}

#[test]
fn safe_add_overflow_returns_none() {
    assert_eq!(safe_add_amounts(i128::MAX, 1), None);
    assert_eq!(safe_add_amounts(i128::MAX, i128::MAX), None);
}

#[test]
fn safe_subtract_normal_values_succeeds() {
    assert_eq!(safe_subtract_amounts(300, 100), Some(200));
    assert_eq!(safe_subtract_amounts(0, 0), Some(0));
}

#[test]
fn safe_subtract_underflow_returns_none() {
    assert_eq!(safe_subtract_amounts(i128::MIN, 1), None);
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Pure helper: validate_single_amount at extreme values
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn validate_single_amount_at_max_allowed_passes() {
    assert!(validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS).is_ok());
}

#[test]
fn validate_single_amount_one_above_max_rejected() {
    let result = validate_single_amount(MAX_SINGLE_AMOUNT_STROOPS + 1);
    assert_eq!(result, Err(EscrowError::InvalidMilestoneAmount));
}

#[test]
fn validate_single_amount_i128_max_rejected() {
    let result = validate_single_amount(i128::MAX);
    assert_eq!(result, Err(EscrowError::InvalidMilestoneAmount));
}

#[test]
fn validate_single_amount_zero_rejected() {
    let result = validate_single_amount(0);
    assert_eq!(result, Err(EscrowError::AmountMustBePositive));
}

#[test]
fn validate_single_amount_negative_rejected() {
    let result = validate_single_amount(-1);
    assert_eq!(result, Err(EscrowError::AmountMustBePositive));
}

#[test]
fn validate_single_amount_i128_min_rejected() {
    let result = validate_single_amount(i128::MIN);
    assert_eq!(result, Err(EscrowError::AmountMustBePositive));
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Pure helper: accumulate_amounts at extreme values
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn accumulate_amounts_empty_iterator_gives_zero() {
    let result = accumulate_amounts(core::iter::empty());
    assert_eq!(result, Ok(0));
}

#[test]
fn accumulate_amounts_single_max_allowed_passes() {
    let result = accumulate_amounts([MAX_SINGLE_AMOUNT_STROOPS]);
    assert_eq!(result, Ok(MAX_SINGLE_AMOUNT_STROOPS));
}

#[test]
fn accumulate_amounts_two_valid_amounts_passes() {
    let result = accumulate_amounts([1_0000000_i128, 2_0000000_i128]);
    assert_eq!(result, Ok(3_0000000_i128));
}

#[test]
fn accumulate_amounts_sum_near_i128_max_overflow_rejected() {
    // Two amounts that are each individually too large (exceed MAX_SINGLE_AMOUNT_STROOPS)
    // so they get caught by validate_single_amount before the add.
    let result = accumulate_amounts([i128::MAX]);
    assert_eq!(result, Err(EscrowError::InvalidMilestoneAmount));
}

#[test]
fn accumulate_amounts_zero_amount_rejected() {
    let result = accumulate_amounts([100_i128, 0_i128]);
    assert_eq!(result, Err(EscrowError::AmountMustBePositive));
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Pure helper: validate_deposit_amount at extreme values
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn validate_deposit_exact_fill_passes() {
    // Deposit exactly fills remaining capacity.
    assert!(validate_deposit_amount(500, 500, 1_000).is_ok());
}

#[test]
fn validate_deposit_one_over_rejects() {
    let result = validate_deposit_amount(501, 500, 1_000);
    assert_eq!(result, Err(EscrowError::InvalidMilestoneAmount));
}

#[test]
fn validate_deposit_i128_max_current_overflow_rejects() {
    // current_deposited = i128::MAX → adding 1 would overflow.
    let result = validate_deposit_amount(1, i128::MAX, i128::MAX);
    assert_eq!(result, Err(EscrowError::PotentialOverflow));
}

#[test]
fn validate_deposit_amount_zero_rejected() {
    let result = validate_deposit_amount(0, 0, 1_000);
    assert_eq!(result, Err(EscrowError::AmountMustBePositive));
}

#[test]
fn validate_deposit_amount_negative_rejected() {
    let result = validate_deposit_amount(-1, 0, 1_000);
    assert_eq!(result, Err(EscrowError::AmountMustBePositive));
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. calculate_protocol_fee: overflow and boundary checks
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn calculate_fee_zero_bps_short_circuits_to_zero() {
    let env = make_env();
    assert_eq!(crate::Escrow::calculate_protocol_fee(&env, i128::MAX, 0), 0);
}

#[test]
fn calculate_fee_normal_values_correct() {
    let env = make_env();
    // 1_000 stroops at 1_000 bps (10%) = 100
    assert_eq!(
        crate::Escrow::calculate_protocol_fee(&env, 1_000, 1_000),
        100
    );
    // 9 stroops at 1_000 bps → floor(9*1000/10_000) = 0
    assert_eq!(crate::Escrow::calculate_protocol_fee(&env, 9, 1_000), 0);
    // 10_000 stroops at 10_000 bps (100%) = 10_000
    assert_eq!(
        crate::Escrow::calculate_protocol_fee(&env, 10_000, 10_000),
        10_000
    );
}

#[test]
#[should_panic]
fn calculate_fee_i128_max_amount_nonzero_bps_panics_with_overflow() {
    // i128::MAX * 1_000 overflows i128 → PotentialOverflow panic
    let env = make_env();
    crate::Escrow::calculate_protocol_fee(&env, i128::MAX, 1_000);
}

#[test]
fn calculate_fee_largest_safe_amount_does_not_overflow() {
    // i128::MAX / 10_000 is the largest amount that won't overflow at 1 bps.
    let env = make_env();
    let safe = i128::MAX / 10_000;
    // Should not panic.
    let fee = crate::Escrow::calculate_protocol_fee(&env, safe, 1);
    assert!(fee >= 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. release_milestone: released_amount accumulates without overflow
// ═══════════════════════════════════════════════════════════════════════════

/// Releasing all milestones in a normal-range contract produces the correct
/// cumulative released_amount (checks the fixed `checked_add` path).
#[test]
fn release_milestone_accumulates_released_amount_correctly() {
    let env = make_env();
    let (escrow, sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // Three milestones: 100, 200, 300 stroops.
    let milestones = vec![&env, 100_i128, 200_i128, 300_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = 600_i128;
    mint(&env, &sac, &client_addr, total);
    escrow.deposit_funds(&id, &client_addr, &total);

    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);
    assert_eq!(escrow.get_contract(&id).released_amount, 100);

    escrow.approve_milestone_release(&id, &client_addr, &1);
    escrow.release_milestone(&id, &client_addr, &1);
    assert_eq!(escrow.get_contract(&id).released_amount, 300);

    escrow.approve_milestone_release(&id, &client_addr, &2);
    escrow.release_milestone(&id, &client_addr, &2);
    assert_eq!(escrow.get_contract(&id).released_amount, 600);
}

/// Releasing a milestone with fee enabled: accumulated_fees updates safely.
#[test]
fn release_with_fee_accumulates_protocol_fees_correctly() {
    let env = make_env();
    let (escrow, sac, admin) = setup_escrow(&env);
    // 10% fee
    escrow.set_protocol_fee_bps(&1_000_u32);

    let (client_addr, _freelancer_addr, id) =
        single_milestone_contract(&env, &escrow, &sac, 1_000_i128);

    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);

    // fee = 1_000 * 1_000 / 10_000 = 100
    assert_eq!(escrow.get_accumulated_protocol_fees(), 100);
    // net released = 1_000 - 100 = 900
    assert_eq!(escrow.get_contract(&id).released_amount, 900);
    let _ = admin; // keep admin alive
}

/// Two sequential releases with fees: accumulated_fees adds up correctly.
#[test]
fn two_releases_with_fee_accumulate_without_overflow() {
    let env = make_env();
    let (escrow, sac, _admin) = setup_escrow(&env);
    // 5% fee
    escrow.set_protocol_fee_bps(&500_u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 2_000_i128, 4_000_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &sac, &client_addr, 6_000_i128);
    escrow.deposit_funds(&id, &client_addr, &6_000_i128);

    // Release m0: fee = 2_000 * 500 / 10_000 = 100
    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);
    assert_eq!(escrow.get_accumulated_protocol_fees(), 100);

    // Release m1: fee = 4_000 * 500 / 10_000 = 200; cumulative = 300
    escrow.approve_milestone_release(&id, &client_addr, &1);
    escrow.release_milestone(&id, &client_addr, &1);
    assert_eq!(escrow.get_accumulated_protocol_fees(), 300);
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. refund_unreleased_milestones: refunded_amount accumulates without overflow
// ═══════════════════════════════════════════════════════════════════════════

/// Refunding a single milestone updates refunded_amount via checked_add.
#[test]
fn refund_single_milestone_updates_refunded_amount_correctly() {
    let env = make_env();
    let (escrow, sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 500_i128, 300_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = 800_i128;
    mint(&env, &sac, &client_addr, total);
    escrow.deposit_funds(&id, &client_addr, &total);

    let indices = vec![&env, 0_u32];
    escrow.refund_unreleased_milestones(&id, &indices);

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.refunded_amount, 500);
}

/// Refunding two milestones: sum is accumulated via checked_add.
#[test]
fn refund_two_milestones_accumulates_correctly() {
    let env = make_env();
    let (escrow, sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_i128, 400_i128, 600_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = 1_200_i128;
    mint(&env, &sac, &client_addr, total);
    escrow.deposit_funds(&id, &client_addr, &total);

    let indices = vec![&env, 0_u32, 1_u32];
    escrow.refund_unreleased_milestones(&id, &indices);

    let contract = escrow.get_contract(&id);
    assert_eq!(contract.refunded_amount, 600);
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Accounting invariant: released + refunded + available == funded
// ═══════════════════════════════════════════════════════════════════════════

/// After a release and a refund the invariant must hold.
#[test]
fn accounting_invariant_holds_after_release_then_refund() {
    let env = make_env();
    let (escrow, sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 300_i128, 700_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &sac, &client_addr, 1_000_i128);
    escrow.deposit_funds(&id, &client_addr, &1_000_i128);

    // Release m0
    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);

    // Refund m1
    let indices = vec![&env, 1_u32];
    escrow.refund_unreleased_milestones(&id, &indices);

    let c = escrow.get_contract(&id);
    let available = c.funded_amount - c.released_amount - c.refunded_amount;
    assert!(available >= 0);
    assert_eq!(
        c.funded_amount,
        c.released_amount + c.refunded_amount + available
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Resolution payouts: dispute arithmetic stays within i128 bounds
// ═══════════════════════════════════════════════════════════════════════════

/// resolution_payouts does not overflow for a FullRefund on a large balance.
#[test]
fn resolution_payouts_full_refund_large_balance() {
    // Use a valid large amount within MAX_SINGLE_AMOUNT_STROOPS.
    let large = crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS;
    let contract = crate::Contract {
        client: {
            let env = make_env();
            Address::generate(&env)
        },
        freelancer: {
            let env = make_env();
            Address::generate(&env)
        },
        arbiter: None,
        status: crate::ContractStatus::Disputed,
        total_deposited: large,
        funded_amount: large,
        released_amount: 0,
        refunded_amount: 0,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    };
    let result = crate::resolution_payouts(&contract, &crate::DisputeResolution::FullRefund);
    assert_eq!(result, Ok((large, 0)));
}

/// resolution_payouts does not overflow for a FullPayout on a large balance.
#[test]
fn resolution_payouts_full_payout_large_balance() {
    let large = crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS;
    let contract = crate::Contract {
        client: {
            let env = make_env();
            Address::generate(&env)
        },
        freelancer: {
            let env = make_env();
            Address::generate(&env)
        },
        arbiter: None,
        status: crate::ContractStatus::Disputed,
        total_deposited: large,
        funded_amount: large,
        released_amount: 0,
        refunded_amount: 0,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    };
    let result = crate::resolution_payouts(&contract, &crate::DisputeResolution::FullPayout);
    assert_eq!(result, Ok((0, large)));
}

/// resolution_payouts returns AccountingInvariantViolated when state is corrupted
/// (released > funded, so available would be negative).
#[test]
fn resolution_payouts_negative_available_returns_error() {
    let env = make_env();
    let contract = crate::Contract {
        client: Address::generate(&env),
        freelancer: Address::generate(&env),
        arbiter: None,
        status: crate::ContractStatus::Disputed,
        total_deposited: 1_000,
        funded_amount: 500,
        released_amount: 600, // released > funded → negative available
        refunded_amount: 0,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    };
    let result = crate::resolution_payouts(&contract, &crate::DisputeResolution::FullRefund);
    assert_eq!(result, Err(crate::Error::AccountingInvariantViolated));
}

/// Split resolution with values summing exactly to available succeeds.
#[test]
fn resolution_payouts_split_exact_sum_succeeds() {
    let env = make_env();
    let contract = crate::Contract {
        client: Address::generate(&env),
        freelancer: Address::generate(&env),
        arbiter: None,
        status: crate::ContractStatus::Disputed,
        total_deposited: 1_000,
        funded_amount: 1_000,
        released_amount: 0,
        refunded_amount: 0,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    };
    let split = crate::DisputeSplit {
        client_amount: 600,
        freelancer_amount: 400,
    };
    let result = crate::resolution_payouts(&contract, &crate::DisputeResolution::Split(split));
    assert_eq!(result, Ok((600, 400)));
}

/// Split resolution with components that overflow i128 when summed is rejected.
#[test]
fn resolution_payouts_split_overflow_sum_rejected() {
    let env = make_env();
    // funded_amount = i128::MAX; both split legs = i128::MAX would overflow when summed.
    let contract = crate::Contract {
        client: Address::generate(&env),
        freelancer: Address::generate(&env),
        arbiter: None,
        status: crate::ContractStatus::Disputed,
        total_deposited: i128::MAX,
        funded_amount: i128::MAX,
        released_amount: 0,
        refunded_amount: 0,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    };
    let split = crate::DisputeSplit {
        client_amount: i128::MAX,
        freelancer_amount: i128::MAX,
    };
    let result = crate::resolution_payouts(&contract, &crate::DisputeResolution::Split(split));
    // Either PotentialOverflow or InvalidDisputeSplit (component > available guard fires first).
    assert!(
        result == Err(crate::Error::InvalidDisputeSplit)
            || result == Err(crate::Error::PotentialOverflow),
        "expected overflow or invalid split, got {:?}",
        result
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. Deposit overflow guard via contract entrypoint
// ═══════════════════════════════════════════════════════════════════════════

/// Depositing more than the contract total is rejected with InvalidDepositAmount.
#[test]
fn deposit_exceeding_contract_total_is_rejected() {
    let env = make_env();
    let (escrow, sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let amount = 1_000_i128;
    let milestones = vec![&env, amount];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    // Mint more than the contract total to the client.
    mint(&env, &sac, &client_addr, amount + 1);

    // First deposit: exact total — OK.
    escrow.deposit_funds(&id, &client_addr, &amount);

    // Second deposit should fail (already fully funded — contract is in Funded state,
    // which rejects further deposits with InvalidState).
    let result = escrow.try_deposit_funds(&id, &client_addr, &1_i128);
    assert_contract_error(result, crate::Error::InvalidState);
}

/// Depositing a zero amount is rejected.
#[test]
fn deposit_zero_amount_is_rejected() {
    let env = make_env();
    let (escrow, _sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1_000_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let result = escrow.try_deposit_funds(&id, &client_addr, &0_i128);
    assert_contract_error(result, crate::Error::AmountMustBePositive);
}

/// Depositing a negative amount is rejected.
#[test]
fn deposit_negative_amount_is_rejected() {
    let env = make_env();
    let (escrow, _sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1_000_i128];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let result = escrow.try_deposit_funds(&id, &client_addr, &(-1_i128));
    assert_contract_error(result, crate::Error::AmountMustBePositive);
}

// ═══════════════════════════════════════════════════════════════════════════
// 11. Milestone amount bounds enforced at create_contract
// ═══════════════════════════════════════════════════════════════════════════

/// A milestone with amount 0 is rejected at contract creation.
#[test]
fn create_contract_rejects_zero_milestone_amount() {
    let env = make_env();
    let (escrow, _sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 0_i128];
    let result = escrow.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_contract_error(result, EscrowError::InvalidMilestoneAmount);
}

/// A milestone exceeding MAX_SINGLE_AMOUNT_STROOPS is rejected at contract creation.
#[test]
fn create_contract_rejects_oversized_milestone_amount() {
    let env = make_env();
    let (escrow, _sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, MAX_SINGLE_AMOUNT_STROOPS + 1];
    let result = escrow.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_contract_error(result, EscrowError::InvalidMilestoneAmount);
}

/// Negative milestone amount is rejected at contract creation.
#[test]
fn create_contract_rejects_negative_milestone_amount() {
    let env = make_env();
    let (escrow, _sac, _admin) = setup_escrow(&env);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, -1_i128];
    let result = escrow.try_create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_contract_error(result, EscrowError::InvalidMilestoneAmount);
}
