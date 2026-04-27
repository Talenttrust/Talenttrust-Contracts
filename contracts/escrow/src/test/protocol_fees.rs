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
