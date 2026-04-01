#![cfg(test)]

use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

#[test]
fn test_hello() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

#[test]
fn test_create_contract() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );
    assert_eq!(id, 0);
}

#[test]
fn test_create_contract_with_arbiter() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
        &100_u32,
        &client_addr,
    );
    assert_eq!(id, 0);
}

#[test]
#[should_panic(expected = "At least one milestone required")]
fn test_create_contract_no_milestones() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );
}

#[test]
#[should_panic(expected = "Client and freelancer cannot be the same address")]
fn test_create_contract_same_addresses() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &client_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );
}

#[test]
#[should_panic(expected = "Milestone amounts must be positive")]
fn test_create_contract_negative_amount() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, -1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );
}

#[test]
fn test_deposit_funds() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract first
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    // Note: Authentication tests would require proper mock setup
    // For now, we test the basic contract creation logic

    env.mock_all_auths();
    let result = client.deposit_funds(&1, &client_addr, &1000_0000000);
    assert!(result);
}

#[test]
#[should_panic(expected = "Deposit amount must equal total milestone amounts")]
fn test_deposit_funds_wrong_amount() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract first
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    // Note: Authentication tests would require proper mock setup
    // For now, we test the basic contract creation logic

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &500_0000000);
}

#[test]
fn test_approve_milestone_release_client_only() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    let result = client.approve_milestone_release(&1, &client_addr, &0);
    assert!(result);
}

#[test]
fn test_approve_milestone_release_client_and_arbiter() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientAndArbiter,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    let result = client.approve_milestone_release(&1, &client_addr, &0);
    assert!(result);

    let result = client.approve_milestone_release(&1, &arbiter_addr, &0);
    assert!(result);
}

#[test]
#[should_panic(expected = "Caller not authorized to approve milestone release")]
fn test_approve_milestone_release_unauthorized() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let unauthorized_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &unauthorized_addr, &0);
}

#[test]
#[should_panic(expected = "Invalid milestone ID")]
fn test_approve_milestone_release_invalid_id() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &client_addr, &5);
}

#[test]
#[should_panic(expected = "Milestone already approved by this address")]
fn test_approve_milestone_release_already_approved() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    // First approval should succeed
    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    let result = client.approve_milestone_release(&1, &client_addr, &0);
    assert!(result);

    // Second approval should fail
    client.approve_milestone_release(&1, &client_addr, &0);
}

#[test]
fn test_release_milestone_client_only() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &client_addr, &0);

    let result = client.release_milestone(&1, &client_addr, &0);
    assert!(result);
}

#[test]
fn test_release_milestone_arbiter_only() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ArbiterOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &arbiter_addr, &0);

    let result = client.release_milestone(&1, &arbiter_addr, &0);
    assert!(result);
}

#[test]
#[should_panic(expected = "Insufficient approvals for milestone release")]
fn test_release_milestone_no_approval() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.release_milestone(&1, &client_addr, &0);
}

#[test]
#[should_panic(expected = "Milestone already released")]
fn test_release_milestone_already_released() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // Use 2 milestones so releasing the first one doesn't set status to Completed
    let milestones = vec![&env, 1000_0000000_i128, 2000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &3000_0000000);
    client.approve_milestone_release(&1, &client_addr, &0);

    let result = client.release_milestone(&1, &client_addr, &0);
    assert!(result);

    // Try to release again — should panic with "Milestone already released"
    client.release_milestone(&1, &client_addr, &0);
}

#[test]
fn test_release_milestone_multi_sig() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr),
        &milestones,
        &ReleaseAuthorization::MultiSig,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &client_addr, &0);

    let result = client.release_milestone(&1, &client_addr, &0);
    assert!(result);
}

#[test]
fn test_contract_completion_all_milestones_released() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128, 2000_0000000_i128];

    // Create contract
    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &3000_0000000);

    client.approve_milestone_release(&1, &client_addr, &0);
    client.release_milestone(&1, &client_addr, &0);

    client.approve_milestone_release(&1, &client_addr, &1);
    client.release_milestone(&1, &client_addr, &1);

    // All milestones should be released and contract completed
    // Note: In a real implementation, we would check the contract status
    // For this simplified version, we just verify no panics occurred
}

#[test]
fn test_edge_cases() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1_0000000_i128]; // Minimum amount

    // Test with minimum amount
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );
    assert_eq!(id, 0);

    // Test with multiple milestones
    let many_milestones = vec![
        &env,
        100_0000000_i128,
        200_0000000_i128,
        300_0000000_i128,
        400_0000000_i128,
    ];
    let id2 = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &many_milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );
    assert_eq!(id2, 0); // ledger sequence stays the same in test env
}

#[test]
fn test_release_milestone_protocol_fee_accrual() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &client_addr, &0);
    client.release_milestone(&1, &client_addr, &0);

    let accrued = client.get_protocol_fee_accrued(&1);
    assert_eq!(accrued, 10_0000000_i128); // 1% of 1000_0000000
}

#[test]
fn test_withdraw_protocol_fees() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &client_addr, &0);
    client.release_milestone(&1, &client_addr, &0);

    let result = client.withdraw_protocol_fees(&1, &client_addr, &10_0000000_i128);
    assert!(result);
    let accrued_after = client.get_protocol_fee_accrued(&1);
    assert_eq!(accrued_after, 0);
}

#[test]
#[should_panic(expected = "Only protocol fee account can withdraw accrued fees")]
fn test_withdraw_protocol_fees_unauthorized() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let other_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.deposit_funds(&1, &client_addr, &1000_0000000);
    client.approve_milestone_release(&1, &client_addr, &0);
    client.release_milestone(&1, &client_addr, &0);

    client.withdraw_protocol_fees(&1, &other_addr, &10_0000000_i128);
}

#[test]
#[should_panic(expected = "Protocol fee out of range")]
fn test_set_protocol_fee_bps_invalid() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
        &100_u32,
        &client_addr,
    );

    env.mock_all_auths();
    client.set_protocol_fee_bps(&1, &client_addr, &10_001_u32);
}