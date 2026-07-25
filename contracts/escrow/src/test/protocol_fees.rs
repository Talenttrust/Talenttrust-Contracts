#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, vec};
use crate::{Escrow, EscrowClient, DataKey, Error, ReleaseAuthorization};

#[test]
fn test_default_fees_are_zero() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Default values before initialization or setting must be 0
    assert_eq!(client.get_protocol_fee_bps(), 0);
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

/// Test that `get_protocol_fee_bps` returns 0 when uninitialized.
#[test]
fn test_get_protocol_fee_bps_returns_zero_when_uninitialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// Test that `get_accumulated_protocol_fees` returns 0 when uninitialized.
#[test]
fn test_get_accumulated_protocol_fees_returns_zero_when_uninitialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

/// Test that `get_protocol_fee_bps` returns the configured value after admin sets it.
#[test]
fn test_get_protocol_fee_bps_after_configuration() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    assert_eq!(client.get_protocol_fee_bps(), 0);

    client.set_protocol_fee_bps(&500u32);
    assert_eq!(client.get_protocol_fee_bps(), 500);

    client.set_protocol_fee_bps(&1000u32);
    assert_eq!(client.get_protocol_fee_bps(), 1000);
}

/// Test that protocol fee updates accept 0 and 10_000 basis points.
#[test]
fn test_set_protocol_fee_bps_accepts_boundary_values() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);

    assert!(client.set_protocol_fee_bps(&0u32));
    assert_eq!(client.get_protocol_fee_bps(), 0);

    assert!(client.set_protocol_fee_bps(&10_000u32));
    assert_eq!(client.get_protocol_fee_bps(), 10_000);
}

/// Test that protocol fee updates reject values above 100%.
#[test]
fn test_set_protocol_fee_bps_rejects_values_above_100_percent() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);
    assert!(client.set_protocol_fee_bps(&0u32));

    let result = client.try_set_protocol_fee_bps(&10_001u32);
    super::assert_contract_error(result, Error::InvalidProtocolParameters);
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

/// Test that `get_accumulated_protocol_fees` reflects fees accumulated after milestone releases.
#[test]
fn test_get_accumulated_protocol_fees_after_releases() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.set_protocol_fee_bps(&1000u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_i128, 2500_i128, 3333_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    client.deposit_funds(&id, &client_addr, &6833_i128);

    assert_eq!(client.get_accumulated_protocol_fees(), 0);

    // Fee: 1000 * 1000 / 10_000 = 100
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    assert_eq!(client.get_accumulated_protocol_fees(), 100);

    // Fee: 2500 * 1000 / 10_000 = 250
    client.approve_milestone_release(&id, &client_addr, &1);
    client.release_milestone(&id, &client_addr, &1);
    assert_eq!(client.get_accumulated_protocol_fees(), 350);

    // Fee: 3333 * 1000 / 10_000 = 333
    client.approve_milestone_release(&id, &client_addr, &2);
    client.release_milestone(&id, &client_addr, &2);
    assert_eq!(client.get_accumulated_protocol_fees(), 683);
}

/// Test that accumulated fees remain at 0 when fee rate is 0.
#[test]
fn test_no_fees_accumulated_when_rate_is_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);
    assert_eq!(client.get_protocol_fee_bps(), 0);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    client.deposit_funds(&id, &client_addr, &1000_i128);
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);

    assert_eq!(client.get_accumulated_protocol_fees(), 0);
}

/// Test that read functions bump TTL and can be called multiple times without error.
#[test]
fn test_readers_bump_ttl_and_are_non_destructive() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.set_protocol_fee_bps(&250u32);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &5000_i128);
    });

    for _ in 0..10 {
        assert_eq!(client.get_protocol_fee_bps(), 250);
        assert_eq!(client.get_accumulated_protocol_fees(), 5000);
    }
}

/// Test readers work when keys are set directly without initialization.
#[test]
fn test_readers_work_without_initialization() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBps, &123u32);
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &456_i128);
    });

    assert_eq!(client.get_protocol_fee_bps(), 123);
    assert_eq!(client.get_accumulated_protocol_fees(), 456);
}

#[test]
fn test_fee_math_0_bps() {
    let env = Env::default();
    let fee = Escrow::calculate_protocol_fee(&env, 1000, 0);
    assert_eq!(fee, 0);
}

#[test]
#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, vec, String};
use crate::{Escrow, EscrowClient, DataKey};

fn create_token_contract(e: &Env, admin: &Address) -> Address {
    e.register_stellar_asset_contract(admin.clone())
}

#[test]
fn test_fee_accrual_and_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    let token_admin = Address::generate(&env);
    let token = create_token_contract(&env, &token_admin);
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);

    // Initialize with 1000 bps (10%)
    client.initialize(&admin, &1000u32);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    
    // Milestones: 1000, 2500, 3333
    let milestones = vec![&env, 1000_i128, 2500_i128, 3333_i128];
    
    // Note: create_contract has different arguments depending on the current iteration of the code.
    // Based on lib.rs line 145: pub fn create_contract(env: Env, client: Address, freelancer: Address, arbiter: Option<Address>, milestones: Vec<i128>, terms_hash: Option<Bytes>, grace_period_seconds: Option<u64>)
    // Wait, let's use the actual create_contract signature from lib.rs.
    // Looking at lib.rs, create_contract in test.rs uses:
    // client.create_contract(&client_addr, &freelancer_addr, &None, &milestones);
    let id = client.create_contract(&client_addr, &freelancer_addr, &None, &milestones, &None, &None);

    client.deposit_funds(&id, &6833_i128); // 1000 + 2500 + 3333 = 6833

    // Release milestone 0 (1000)
    // Fee: (1000 * 1000 + 9999) / 10000 = (1000000 + 9999) / 10000 = 1009999 / 10000 = 100
    assert!(client.release_milestone(&id, &0));
    
    // Release milestone 1 (2500)
    // Fee: (2500 * 1000 + 9999) / 10000 = (2500000 + 9999) / 10000 = 2509999 / 10000 = 250
    assert!(client.release_milestone(&id, &1));
    
    // Release milestone 2 (3333)
    // Fee: (3333 * 1000 + 9999) / 10000 = (3333000 + 9999) / 10000 = 3342999 / 10000 = 334
    assert!(client.release_milestone(&id, &2));

    // Total accumulated fees: 100 + 250 + 334 = 684
    
    // Mint tokens to the contract so it has funds to transfer out
    token_admin_client.mint(&contract_id, &684);

    let destination = Address::generate(&env);
    
    // Admin withdraws protocol fees
    let success = client.withdraw_protocol_fees(&admin, &destination, &684_i128, &token);
    assert!(success);
    
    assert_eq!(token_client.balance(&destination), 684);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #6)")] // UnauthorizedRole
fn test_unauthorized_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    client.initialize(&admin, &1000u32);
    
    let fake_admin = Address::generate(&env);
    let destination = Address::generate(&env);
    let token = Address::generate(&env);
    
    // This should panic
    client.withdraw_protocol_fees(&fake_admin, &destination, &100_i128, &token);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #13)")] // InsufficientAccumulatedFees
fn test_over_withdrawal() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    client.initialize(&admin, &1000u32);
    
    let destination = Address::generate(&env);
    let token = Address::generate(&env);
    
    // Withdraw more than 0
    client.withdraw_protocol_fees(&admin, &destination, &100_i128, &token);
}

#[test]
fn test_fee_math_0_bps() {
    let env = Env::default();
    let fee = Escrow::calculate_protocol_fee(&env, 1000, 0);
    assert_eq!(fee, 0);
}

#[test]
fn test_fee_math_normal_bps() {
    let env = Env::default();
    let fee = Escrow::calculate_protocol_fee(&env, 1000, 1000);
    assert_eq!(fee, 100);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #25)")] // PotentialOverflow
fn test_fee_math_overflow() {
    let env = Env::default();
    Escrow::calculate_protocol_fee(&env, i128::MAX, 1000);
}

#[test]
fn test_fee_math_tiny_amount() {
    let env = Env::default();
    // 9 * 1000 = 9000. 9000 / 10000 = 0 (rounds to zero)
    let fee = Escrow::calculate_protocol_fee(&env, 9, 1000);
    assert_eq!(fee, 0);
}
