//! Storage entrypoint bounds validation tests (issue #899).
//!
//! Covers every storage-mutating entrypoint that accepts numeric or
//! length-bounded inputs, verifying:
//!   - values at the exact boundary are accepted
//!   - values one above/below the boundary are rejected with the correct typed error
//!   - zero / negative inputs are rejected where applicable
//!   - contract_id = 0 is rejected for all entrypoints that use it
//!   - existing valid inputs continue to be accepted (regression)
//!
//! Entrypoints covered:
//!   - `set_governed_params`        — max_escrow_total_stroops > 0
//!   - `set_reputation_config`      — min_rating, max_rating, max_comment_bytes
//!   - `set_protocol_fee_bps`       — 0..=10_000
//!   - `propose_client_migration`   — contract_id != 0
//!   - `accept_client_migration`    — contract_id != 0
//!   - `rollback_dispute`           — contract_id != 0
//!   - `deposit_funds`              — amount > 0
//!   - `create_contract`            — milestone count in [1, MAX_MILESTONES]

#![cfg(test)]

#[allow(deprecated)]
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env};

use crate::{
    Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization, MAX_FEE_BPS, MAX_MILESTONES,
    MAX_SINGLE_AMOUNT_STROOPS, MAX_TOTAL_ESCROW_STROOPS,
};

// ── Fixture helpers ──────────────────────────────────────────────────────────

/// Minimal fixture: initialized escrow, no settlement token.
fn setup_no_token(env: &Env) -> (EscrowClient<'_>, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(env, &addr);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Full fixture: initialized escrow + bound SAC token + minted client balance.
#[allow(deprecated)]
fn setup_with_token(env: &Env) -> (EscrowClient<'_>, Address, Address, Address) {
    env.mock_all_auths_allowing_non_root_auth();
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(env, &addr);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &token);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);

    StellarAssetClient::new(env, &token).mint(&client_addr, &(MAX_TOTAL_ESCROW_STROOPS * 10));

    (client, client_addr, freelancer_addr, admin)
}

/// Create a funded 1-milestone contract; returns contract_id.
fn funded_contract(
    env: &Env,
    escrow: &EscrowClient<'_>,
    client_addr: &Address,
    freelancer_addr: &Address,
    amount: i128,
) -> u32 {
    let milestones = vec![env, amount];
    let id = escrow.create_contract(
        client_addr,
        freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&id, client_addr, &amount);
    id
}

/// Build a Soroban Vec of `count` identical amounts.
fn milestone_vec(env: &Env, count: u32, amount: i128) -> soroban_sdk::Vec<i128> {
    let mut v = soroban_sdk::Vec::new(env);
    for _ in 0..count {
        v.push_back(amount);
    }
    v
}

// ── set_governed_params — max_escrow_total_stroops bounds ─────────────────────

/// Boundary success: exactly 1 stroop must be accepted.
#[test]
fn set_governed_params_accepts_1_stroop() {
    let env = Env::default();
    let (escrow, admin) = setup_no_token(&env);
    assert!(escrow.set_governed_params(&admin, &0_u32, &1_i128));
    let params = escrow.get_governed_parameters().unwrap();
    assert_eq!(params.max_escrow_total_stroops, 1);
}

/// Boundary success: i128::MAX must be accepted.
#[test]
fn set_governed_params_accepts_i128_max() {
    let env = Env::default();
    let (escrow, admin) = setup_no_token(&env);
    assert!(escrow.set_governed_params(&admin, &0_u32, &i128::MAX));
    let params = escrow.get_governed_parameters().unwrap();
    assert_eq!(params.max_escrow_total_stroops, i128::MAX);
}

/// Zero cap must be rejected with InvalidProtocolParameters.
#[test]
fn set_governed_params_rejects_zero_cap() {
    let env = Env::default();
    let (escrow, admin) = setup_no_token(&env);
    let result = escrow.try_set_governed_params(&admin, &0_u32, &0_i128);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(e, want, "expected InvalidProtocolParameters for zero cap");
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// Negative cap must be rejected with InvalidProtocolParameters.
#[test]
fn set_governed_params_rejects_negative_cap() {
    let env = Env::default();
    let (escrow, admin) = setup_no_token(&env);
    let result = escrow.try_set_governed_params(&admin, &0_u32, &(-1_i128));
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(
                e, want,
                "expected InvalidProtocolParameters for negative cap"
            );
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// i128::MIN must be rejected with InvalidProtocolParameters.
#[test]
fn set_governed_params_rejects_i128_min() {
    let env = Env::default();
    let (escrow, admin) = setup_no_token(&env);
    let result = escrow.try_set_governed_params(&admin, &0_u32, &i128::MIN);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(e, want, "expected InvalidProtocolParameters for i128::MIN");
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// fee_bps > MAX_FEE_BPS must still be rejected (existing validation preserved).
#[test]
fn set_governed_params_rejects_fee_over_max() {
    let env = Env::default();
    let (escrow, admin) = setup_no_token(&env);
    let result = escrow.try_set_governed_params(&admin, &(MAX_FEE_BPS + 1), &100_i128);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(
                e, want,
                "expected InvalidProtocolParameters for fee over max"
            );
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// Rejected calls must not mutate the stored parameters.
#[test]
fn set_governed_params_rejected_leaves_params_unchanged() {
    let env = Env::default();
    let (escrow, admin) = setup_no_token(&env);
    escrow.set_governed_params(&admin, &500_u32, &1_000_000_i128);
    let _ = escrow.try_set_governed_params(&admin, &500_u32, &0_i128);
    let params = escrow.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 500);
    assert_eq!(params.max_escrow_total_stroops, 1_000_000);
}

// ── set_reputation_config — rating and comment bounds ─────────────────────────

/// Default config (1, 5, 200) must be accepted.
#[test]
fn set_reputation_config_accepts_default() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_reputation_config(&1_u32, &5_u32, &200_u32));
    let cfg = escrow.get_reputation_config();
    assert_eq!(cfg.min_rating, 1);
    assert_eq!(cfg.max_rating, 5);
    assert_eq!(cfg.max_comment_bytes, 200);
}

/// min_rating == max_rating (degenerate range) must be accepted.
#[test]
fn set_reputation_config_accepts_equal_min_max() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_reputation_config(&3_u32, &3_u32, &1_u32));
    let cfg = escrow.get_reputation_config();
    assert_eq!(cfg.min_rating, 3);
    assert_eq!(cfg.max_rating, 3);
}

/// max_comment_bytes = 1_000 (maximum) must be accepted.
#[test]
fn set_reputation_config_accepts_max_comment_1000() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_reputation_config(&1_u32, &10_u32, &1_000_u32));
    let cfg = escrow.get_reputation_config();
    assert_eq!(cfg.max_comment_bytes, 1_000);
}

/// max_comment_bytes = 1_001 must be rejected.
#[test]
fn set_reputation_config_rejects_comment_over_1000() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_set_reputation_config(&1_u32, &5_u32, &1_001_u32);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(
                e, want,
                "expected InvalidProtocolParameters for comment > 1000"
            );
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// max_comment_bytes = 0 must be rejected.
#[test]
fn set_reputation_config_rejects_zero_comment_bytes() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_set_reputation_config(&1_u32, &5_u32, &0_u32);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(
                e, want,
                "expected InvalidProtocolParameters for 0 comment bytes"
            );
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// min_rating = 0 must be rejected.
#[test]
fn set_reputation_config_rejects_zero_min_rating() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_set_reputation_config(&0_u32, &5_u32, &200_u32);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(
                e, want,
                "expected InvalidProtocolParameters for min_rating=0"
            );
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// max_rating < min_rating must be rejected.
#[test]
fn set_reputation_config_rejects_max_below_min() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_set_reputation_config(&5_u32, &3_u32, &200_u32);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(e, want, "expected InvalidProtocolParameters for max < min");
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// max_rating > 10 must be rejected.
#[test]
fn set_reputation_config_rejects_max_rating_over_10() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_set_reputation_config(&1_u32, &11_u32, &200_u32);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(
                e, want,
                "expected InvalidProtocolParameters for max_rating=11"
            );
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// Rejected calls must not mutate the stored reputation config.
#[test]
fn set_reputation_config_rejected_leaves_config_unchanged() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    escrow.set_reputation_config(&2_u32, &8_u32, &150_u32);
    let _ = escrow.try_set_reputation_config(&2_u32, &8_u32, &0_u32);
    let cfg = escrow.get_reputation_config();
    assert_eq!(cfg.min_rating, 2);
    assert_eq!(cfg.max_rating, 8);
    assert_eq!(cfg.max_comment_bytes, 150);
}

// ── set_protocol_fee_bps — bounds validation (centralized) ────────────────────

/// 0 bps (no fee) must be accepted.
#[test]
fn set_protocol_fee_bps_accepts_zero() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_protocol_fee_bps(&0_u32));
    assert_eq!(escrow.get_protocol_fee_bps(), 0);
}

/// Exactly MAX_FEE_BPS (10_000) must be accepted.
#[test]
fn set_protocol_fee_bps_accepts_exactly_max() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_protocol_fee_bps(&MAX_FEE_BPS));
    assert_eq!(escrow.get_protocol_fee_bps(), MAX_FEE_BPS);
}

/// MAX_FEE_BPS + 1 must be rejected.
#[test]
fn set_protocol_fee_bps_rejects_over_max() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_set_protocol_fee_bps(&(MAX_FEE_BPS + 1));
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(e, want, "expected InvalidProtocolParameters for 10001 bps");
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

/// u32::MAX must be rejected.
#[test]
fn set_protocol_fee_bps_rejects_u32_max() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_set_protocol_fee_bps(&u32::MAX);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = Error::InvalidProtocolParameters.into();
            assert_eq!(e, want, "expected InvalidProtocolParameters for u32::MAX");
        }
        other => panic!("expected InvalidProtocolParameters, got {:?}", other),
    }
}

// ── contract_id = 0 rejection for migration entrypoints ───────────────────────

/// propose_client_migration with contract_id = 0 must be rejected.
#[test]
fn propose_client_migration_rejects_zero_contract_id() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 100_0000000_i128];
    let _id = escrow.create_contract(
        &c,
        &f,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let new_client = Address::generate(&env);
    let result = escrow.try_propose_client_migration(&0_u32, &c, &new_client);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = EscrowError::ContractNotFound.into();
            assert_eq!(e, want, "expected ContractNotFound for contract_id=0");
        }
        other => panic!("expected ContractNotFound, got {:?}", other),
    }
}

/// accept_client_migration with contract_id = 0 must be rejected.
#[test]
fn accept_client_migration_rejects_zero_contract_id() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let new_client = Address::generate(&env);
    let result = escrow.try_accept_client_migration(&0_u32, &new_client);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = EscrowError::ContractNotFound.into();
            assert_eq!(e, want, "expected ContractNotFound for contract_id=0");
        }
        other => panic!("expected ContractNotFound, got {:?}", other),
    }
}

/// rollback_dispute with contract_id = 0 must be rejected.
#[test]
fn rollback_dispute_rejects_zero_contract_id() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let result = escrow.try_rollback_dispute(&0_u32);
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = EscrowError::ContractNotFound.into();
            assert_eq!(e, want, "expected ContractNotFound for contract_id=0");
        }
        other => panic!("expected ContractNotFound, got {:?}", other),
    }
}

// ── deposit_funds — amount bounds (additional edge cases) ─────────────────────

/// Deposit of i128::MAX must be rejected (exceeds MAX_SINGLE_AMOUNT_STROOPS).
#[test]
fn deposit_funds_rejects_i128_max_amount() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let milestones = vec![&env, MAX_SINGLE_AMOUNT_STROOPS];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let result = escrow.try_deposit_funds(&id, &client_addr, &i128::MAX);
    assert!(result.is_err(), "deposit of i128::MAX must be rejected");
}

/// Deposit of MAX_SINGLE_AMOUNT_STROOPS + 1 must be rejected.
#[test]
fn deposit_funds_rejects_amount_over_single_max() {
    let env = Env::default();
    let (escrow, client_addr, freelancer_addr, _admin) = setup_with_token(&env);
    let milestones = vec![&env, MAX_SINGLE_AMOUNT_STROOPS];
    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let result = escrow.try_deposit_funds(&id, &client_addr, &(MAX_SINGLE_AMOUNT_STROOPS + 1));
    assert!(
        result.is_err(),
        "deposit over MAX_SINGLE_AMOUNT_STROOPS must be rejected"
    );
}

// ── create_contract — milestone count bounds (additional edge cases) ──────────

/// Exactly MAX_MILESTONES milestones must be accepted.
#[test]
fn create_contract_accepts_exactly_max_milestones() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = milestone_vec(&env, MAX_MILESTONES, 1_i128);
    let result = escrow.try_create_contract(
        &c,
        &f,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(
        result.is_ok(),
        "exactly MAX_MILESTONES milestones should be accepted"
    );
}

/// MAX_MILESTONES + 1 milestones must be rejected with TooManyMilestones.
#[test]
fn create_contract_rejects_over_max_milestones() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = milestone_vec(&env, MAX_MILESTONES + 1, 1_i128);
    let result = escrow.try_create_contract(
        &c,
        &f,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    match result {
        Err(Ok(e)) => {
            let want: soroban_sdk::Error = EscrowError::TooManyMilestones.into();
            assert_eq!(e, want, "expected TooManyMilestones for MAX_MILESTONES + 1");
        }
        other => panic!("expected TooManyMilestones, got {:?}", other),
    }
}

// ── Regression: valid inputs still accepted ───────────────────────────────────

/// A standard 3-milestone contract with typical amounts must still be created.
#[test]
fn regression_standard_three_milestone_contract() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    let c = Address::generate(&env);
    let f = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];
    let id = escrow.create_contract(
        &c,
        &f,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    // Verify a contract was created successfully.
    let _ = id;
}

/// set_protocol_fee_bps can be updated multiple times with valid values.
#[test]
fn regression_set_protocol_fee_bps_multiple_updates() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_protocol_fee_bps(&100_u32));
    assert!(escrow.set_protocol_fee_bps(&500_u32));
    assert!(escrow.set_protocol_fee_bps(&0_u32));
    assert!(escrow.set_protocol_fee_bps(&10_000_u32));
    assert_eq!(escrow.get_protocol_fee_bps(), 10_000_u32);
}

/// set_governed_params can be updated multiple times with valid values.
#[test]
fn regression_set_governed_params_multiple_updates() {
    let env = Env::default();
    let (escrow, admin) = setup_no_token(&env);
    assert!(escrow.set_governed_params(&admin, &0_u32, &1_000_000_i128));
    assert!(escrow.set_governed_params(&admin, &500_u32, &500_000_i128));
    assert!(escrow.set_governed_params(&admin, &0_u32, &i128::MAX));
    let params = escrow.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 0);
    assert_eq!(params.max_escrow_total_stroops, i128::MAX);
}

/// set_reputation_config can be updated multiple times with valid values.
#[test]
fn regression_set_reputation_config_multiple_updates() {
    let env = Env::default();
    let (escrow, _admin) = setup_no_token(&env);
    assert!(escrow.set_reputation_config(&1_u32, &5_u32, &200_u32));
    assert!(escrow.set_reputation_config(&2_u32, &8_u32, &150_u32));
    assert!(escrow.set_reputation_config(&1_u32, &10_u32, &1_000_u32));
    let cfg = escrow.get_reputation_config();
    assert_eq!(cfg.min_rating, 1);
    assert_eq!(cfg.max_rating, 10);
    assert_eq!(cfg.max_comment_bytes, 1_000);
}
