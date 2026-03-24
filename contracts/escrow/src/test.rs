use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};
use crate::{ApprovalAction, ApprovalRequest, Escrow, EscrowClient};

// ── Original tests (unchanged) ────────────────────────────────────────────────

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
    let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
    assert_eq!(id, 1);
}

#[test]
fn test_deposit_funds() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let result = client.deposit_funds(&1, &1_000_0000000);
    assert!(result);
}

#[test]
fn test_release_milestone() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let result = client.release_milestone(&1, &0);
    assert!(result);
}

// ── Signature-based approval tests ───────────────────────────────────────────

/// Helper: build a default ApprovalRequest for tests.
fn make_request(env: &Env, action: ApprovalAction, nonce: u64) -> ApprovalRequest {
    ApprovalRequest {
        contract_id:  1,
        milestone_id: 0,
        action,
        nonce,
    }
}

#[test]
fn test_get_nonce_initial_zero() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);
    let signer = Address::generate(&env);
    assert_eq!(client.get_nonce(&signer), 0u64);
}

#[test]
fn test_submit_milestone_approval_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);

    let signer = Address::generate(&env);
    let request = make_request(&env, ApprovalAction::MilestoneAcceptance, 0);
    // In the test environment we pass an empty Bytes as the signature;
    // ed25519_verify is stubbed by mock_all_auths / the test harness.
    let sig = soroban_sdk::Bytes::new(&env);

    let ok = client.submit_approval(&request, &signer, &sig);
    assert!(ok);
    // Nonce must have been incremented.
    assert_eq!(client.get_nonce(&signer), 1u64);
}

#[test]
fn test_submit_dispute_approval_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);

    let signer = Address::generate(&env);
    let request = make_request(&env, ApprovalAction::DisputeAction, 0);
    let sig = soroban_sdk::Bytes::new(&env);

    let ok = client.submit_approval(&request, &signer, &sig);
    assert!(ok);
}

#[test]
fn test_get_approval_returns_record() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);

    let signer = Address::generate(&env);
    let request = make_request(&env, ApprovalAction::MilestoneAcceptance, 0);
    let sig = soroban_sdk::Bytes::new(&env);
    client.submit_approval(&request, &signer, &sig);

    let record = client
        .get_approval(&1u32, &0u32, &ApprovalAction::MilestoneAcceptance)
        .expect("approval should exist");

    assert_eq!(record.signer, signer);
    assert_eq!(record.request.contract_id, 1);
    assert_eq!(record.request.milestone_id, 0);
}

#[test]
fn test_get_approval_missing_returns_none() {
    let env = Env::default();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);

    let result = client.get_approval(&99u32, &99u32, &ApprovalAction::MilestoneAcceptance);
    assert!(result.is_none());
}

#[test]
#[should_panic(expected = "invalid nonce")]
fn test_replay_attack_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);

    let signer = Address::generate(&env);
    let request = make_request(&env, ApprovalAction::MilestoneAcceptance, 0);
    let sig = soroban_sdk::Bytes::new(&env);

    // First submission succeeds.
    client.submit_approval(&request, &signer, &sig);
    // Second submission with the SAME nonce must panic.
    client.submit_approval(&request, &signer, &sig);
}

#[test]
#[should_panic(expected = "approval already recorded")]
fn test_duplicate_approval_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);

    let signer = Address::generate(&env);
    // First call — nonce 0.
    let req0 = make_request(&env, ApprovalAction::MilestoneAcceptance, 0);
    let sig = soroban_sdk::Bytes::new(&env);
    client.submit_approval(&req0, &signer, &sig);

    // Second call — correct nonce 1, but same (contract, milestone, action).
    let req1 = make_request(&env, ApprovalAction::MilestoneAcceptance, 1);
    client.submit_approval(&req1, &signer, &sig);
}

#[test]
fn test_nonce_increments_across_different_actions() {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &cid);

    let signer = Address::generate(&env);
    let sig = soroban_sdk::Bytes::new(&env);

    let req0 = make_request(&env, ApprovalAction::MilestoneAcceptance, 0);
    client.submit_approval(&req0, &signer, &sig);
    assert_eq!(client.get_nonce(&signer), 1u64);

    let req1 = make_request(&env, ApprovalAction::DisputeAction, 1);
    client.submit_approval(&req1, &signer, &sig);
    assert_eq!(client.get_nonce(&signer), 2u64);
}