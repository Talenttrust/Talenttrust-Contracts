#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, vec};
use crate::{Escrow, EscrowClient, DataKey, Error};

#[test]
fn test_set_protocol_fee_bps_above_max_is_clamped() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    // Initialize contract
    client.initialize(&admin);
    
    // Try to set fee above max (1000)
    client.set_protocol_fee_bps(&admin, &1500u32);
    
    // Verify it's clamped to 1000
    let stored_fee: u32 = env.storage().persistent().get(&DataKey::ProtocolFeeBps).unwrap();
    assert_eq!(stored_fee, 1000);
}

#[test]
#[should_panic]
fn test_set_protocol_fee_bps_non_admin_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let fake_admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    // Initialize contract
    client.initialize(&admin);
    
    // Try to set fee with non-admin
    client.set_protocol_fee_bps(&fake_admin, &100u32);
}

#[test]
fn test_fee_plus_net_equals_milestone_amount() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    // Initialize and set fee
    client.initialize(&admin);
    client.set_protocol_fee_bps(&admin, &100u32); // 1%
    
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &crate::types::ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &1000_i128);
    client.approve_milestone_release(&id, &client_addr, &0u32);
    client.release_milestone(&id, &client_addr, &0u32);
    
    // Check accumulated fees
    let accumulated: i128 = env.storage().persistent().get(&DataKey::AccumulatedProtocolFees).unwrap();
    // Fee should be 10 (1% of 1000)
    assert_eq!(accumulated, 10);
    // Fee + net = 10 + 990 = 1000
}

#[test]
fn test_default_bps_zero_pays_full_amount() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    // Initialize but don't set fee (defaults to 0)
    client.initialize(&admin);
    
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &crate::types::ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &1000_i128);
    client.approve_milestone_release(&id, &client_addr, &0u32);
    client.release_milestone(&id, &client_addr, &0u32);
    
    // Check accumulated fees is 0
    let accumulated: i128 = env.storage().persistent().get(&DataKey::AccumulatedProtocolFees).unwrap_or(0);
    assert_eq!(accumulated, 0);
}

#[test]
fn test_accumulated_fees_increment_correctly_across_releases() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);
    
    // Initialize and set fee
    client.initialize(&admin);
    client.set_protocol_fee_bps(&admin, &100u32); // 1%
    
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_i128, 2000_i128, 3000_i128];
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &crate::types::ReleaseAuthorization::ClientOnly,
    );
    
    client.deposit_funds(&id, &client_addr, &6000_i128);
    
    // Release first milestone
    client.approve_milestone_release(&id, &client_addr, &0u32);
    client.release_milestone(&id, &client_addr, &0u32);
    let accumulated1: i128 = env.storage().persistent().get(&DataKey::AccumulatedProtocolFees).unwrap();
    assert_eq!(accumulated1, 10);
    
    // Release second milestone
    client.approve_milestone_release(&id, &client_addr, &1u32);
    client.release_milestone(&id, &client_addr, &1u32);
    let accumulated2: i128 = env.storage().persistent().get(&DataKey::AccumulatedProtocolFees).unwrap();
    assert_eq!(accumulated2, 10 + 20);
    
    // Release third milestone
    client.approve_milestone_release(&id, &client_addr, &2u32);
    client.release_milestone(&id, &client_addr, &2u32);
    let accumulated3: i128 = env.storage().persistent().get(&DataKey::AccumulatedProtocolFees).unwrap();
    assert_eq!(accumulated3, 10 + 20 + 30);
}
