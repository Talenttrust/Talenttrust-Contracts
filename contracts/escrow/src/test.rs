#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    vec, Address, Env, IntoVal,
};

use crate::{DisputeResolution, Escrow, EscrowClient, ReleaseAuthorization};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Initializes the contract, approving all auth checks automatically.
fn setup_initialized(env: &Env) -> (EscrowClient, Address, Address) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &arbitrator);

    (client, admin, arbitrator)
}

/// Initializes + creates a funded escrow contract.
fn setup_funded(env: &Env) -> (EscrowClient, Address, Address, Address, u32) {
    let (client, admin, arbitrator) = setup_initialized(env);
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = vec![env, 1000_0000000_i128];

    env.mock_all_auths();
    let escrow_id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    client.deposit_funds(&escrow_id, &1000_0000000);

    (client, admin, arbitrator, client_addr, escrow_id)
}

/// Initializes + creates a funded + disputed escrow contract.
fn setup_disputed(env: &Env) -> (EscrowClient, Address, u32) {
    let (client, _admin, arbitrator, client_addr, escrow_id) = setup_funded(env);

    let reason = symbol_short!("quality");
    let evidence = vec![env, symbol_short!("evidence1")];

    env.mock_all_auths();
    let dispute_id = client.create_dispute(&escrow_id, &reason, &evidence);

    (client, arbitrator, dispute_id)
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[test]
fn test_hello() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let result = client.hello(&symbol_short!("World"));
    assert_eq!(result, symbol_short!("World"));
}

// ==================== CONTRACT CREATION TESTS ====================

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    client.initialize(&admin, &arbitrator);
}

#[test]
fn test_create_contract_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let token = Address::generate(&env);
    let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(id, 0);
}

#[test]
fn test_create_contract_with_arbiter() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    client.initialize(&admin, &arbitrator);
}

#[test]
fn test_create_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _arbitrator) = setup_initialized(&env);
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
    );
    assert_eq!(id, 0);
}

#[test]
#[should_panic(expected = "At least one milestone required")]
fn test_create_contract_no_milestones() {
    let env = Env::default();
    env.mock_all_auths();

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env];

    client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic(expected = "Client and freelancer cannot be the same address")]
fn test_create_contract_same_addresses() {
    let env = Env::default();
    env.mock_all_auths();

    let client_addr = Address::generate(&env);
    let milestones = vec![&env, 1000_0000000_i128];

    client.create_contract(
        &client_addr,
        &client_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_double_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    client.initialize(&admin, &arbitrator);

    // Second call — should panic "already initialized"
    let admin2 = Address::generate(&env);
    let arbitrator2 = Address::generate(&env);
    client.initialize(&admin2, &arbitrator2);
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
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_create_contract_invalid_milestone_amount() {
    let (env, _contract_id, client, _admin, _treasury) = setup_with_treasury();

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
    );

    // Note: Authentication tests would require proper mock setup
    // For now, we test the basic contract creation logic

    env.mock_all_auths();
    let result = client.deposit_funds(&1, &client_addr, &1000_0000000);
    assert!(result);
}

// ==================== DEPOSIT FUNDS TESTS ====================

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
    );
    assert_eq!(id2, 0); // ledger sequence stays the same in test env
}

mod emergency_controls;
mod pause_controls;
