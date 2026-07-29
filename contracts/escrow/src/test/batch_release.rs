use super::{assert_contract_error, register_client, total_milestone_amount};
use crate::{ContractStatus, EscrowError, ReleaseAuthorization, MAX_BATCH_RELEASE};
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

fn setup_funded_contract(
    env: &Env,
    release_auth: ReleaseAuthorization,
) -> (crate::EscrowClient<'_>, Address, Address, u32) {
    let client = register_client(env);
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = super::default_milestones(env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &release_auth,
    );
    let total = total_milestone_amount();
    client.deposit_funds(&contract_id, &client_addr, &total);
    (client, client_addr, freelancer_addr, contract_id)
}

fn approve_all(client: &crate::EscrowClient<'_>, contract_id: u32, caller: &Address) {
    for i in 0..3u32 {
        assert!(client.approve_milestone_release(contract_id, caller, &i));
    }
}

// ===========================================================================
// Happy path
// ===========================================================================

#[test]
fn batch_release_single_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    approve_all(&client, contract_id, &client_addr);

    let indices = vec![&env, 0u32];
    assert!(client.release_milestones_batch(&contract_id, &client_addr, &indices));

    let c = client.get_contract(&contract_id);
    assert_eq!(c.status, ContractStatus::Funded);
}

#[test]
fn batch_release_all_three_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    approve_all(&client, contract_id, &client_addr);

    let indices = vec![&env, 0u32, 1, 2];
    assert!(client.release_milestones_batch(&contract_id, &client_addr, &indices));

    let c = client.get_contract(&contract_id);
    assert_eq!(c.status, ContractStatus::Completed);
}

#[test]
fn batch_release_completes_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    approve_all(&client, contract_id, &client_addr);

    let indices = vec![&env, 0u32, 1, 2];
    assert!(client.release_milestones_batch(&contract_id, &client_addr, &indices));

    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Completed
    );
    assert_eq!(client.get_pending_reputation_credits(&freelancer_addr), 1);
}

#[test]
fn batch_release_partial_subset() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    assert!(client.approve_milestone_release(&contract_id, &client_addr, &1));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &2));

    let indices = vec![&env, 1u32, 2];
    assert!(client.release_milestones_batch(&contract_id, &client_addr, &indices));

    let c = client.get_contract(&contract_id);
    assert_eq!(c.status, ContractStatus::Funded);
}

// ===========================================================================
// Cap / boundary tests
// ===========================================================================

#[test]
fn batch_release_at_cap_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let milestones_vec: soroban_sdk::Vec<i128> = (0..MAX_BATCH_RELEASE)
        .map(|i| (i as i128 + 1) * 100_0000000)
        .collect();
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_vec,
        &ReleaseAuthorization::ClientOnly,
    );
    let total: i128 = (0..MAX_BATCH_RELEASE)
        .map(|i| (i as i128 + 1) * 100_0000000)
        .sum();
    client.deposit_funds(&contract_id, &client_addr, &total);

    for i in 0..MAX_BATCH_RELEASE {
        assert!(client.approve_milestone_release(&contract_id, &client_addr, &i));
    }

    let indices: soroban_sdk::Vec<u32> = (0..MAX_BATCH_RELEASE).collect();
    assert!(client.release_milestones_batch(&contract_id, &client_addr, &indices));
}

#[test]
fn batch_release_over_cap_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    let indices: soroban_sdk::Vec<u32> = (0..=MAX_BATCH_RELEASE).collect();
    let result = client.try_release_milestones_batch(&contract_id, &client_addr, &indices);
    assert_contract_error(result, EscrowError::TooManyMilestones);
}

// ===========================================================================
// Error cases
// ===========================================================================

#[test]
fn batch_release_empty_vector_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    let indices = soroban_sdk::Vec::<u32>::new(&env);
    let result = client.try_release_milestones_batch(&contract_id, &client_addr, &indices);
    assert_contract_error(result, EscrowError::EmptyMilestones);
}

#[test]
fn batch_release_duplicate_indices_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    approve_all(&client, contract_id, &client_addr);

    let indices = vec![&env, 0u32, 0];
    let result = client.try_release_milestones_batch(&contract_id, &client_addr, &indices);
    assert_contract_error(result, EscrowError::DuplicateMilestoneInRefund);
}

#[test]
fn batch_release_rejects_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _client_addr, freelancer_addr, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    let stranger = Address::generate(&env);
    let indices = vec![&env, 0u32];
    let result = client.try_release_milestones_batch(&contract_id, &stranger, &indices);
    assert_contract_error(result, EscrowError::UnauthorizedRole);

    let result = client.try_release_milestones_batch(&contract_id, &freelancer_addr, &indices);
    assert_contract_error(result, EscrowError::UnauthorizedRole);
}

#[test]
fn batch_release_rejects_paused_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    client.pause();

    let indices = vec![&env, 0u32];
    let result = client.try_release_milestones_batch(&contract_id, &client_addr, &indices);
    assert_contract_error(result, EscrowError::ContractPaused);
}

#[test]
fn batch_release_rejects_nonexistent_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);

    let indices = vec![&env, 0u32];
    let result = client.try_release_milestones_batch(&999u32, &client_addr, &indices);
    assert_contract_error(result, EscrowError::ContractNotFound);
}

#[test]
fn batch_release_rejects_already_released_milestone() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    approve_all(&client, contract_id, &client_addr);

    // Release index 0 via single call first
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    // Try to include index 0 in a batch
    let indices = vec![&env, 0u32, 1];
    let result = client.try_release_milestones_batch(&contract_id, &client_addr, &indices);
    assert_contract_error(result, EscrowError::AlreadyReleased);
}

#[test]
fn batch_release_rejects_index_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, _freelancer, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::ClientOnly);

    approve_all(&client, contract_id, &client_addr);

    let indices = vec![&env, 0u32, 99];
    let result = client.try_release_milestones_batch(&contract_id, &client_addr, &indices);
    assert_contract_error(result, EscrowError::IndexOutOfBounds);
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn batch_release_requires_funded_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = super::default_milestones(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let indices = vec![&env, 0u32];
    let result = client.try_release_milestones_batch(&contract_id, &client_addr, &indices);
    assert_contract_error(result, EscrowError::InvalidState);
}

#[test]
fn batch_release_respects_release_authorization_multisig() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, client_addr, freelancer_addr, contract_id) =
        setup_funded_contract(&env, ReleaseAuthorization::MultiSig);

    // Only client approves — should be insufficient for MultiSig
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));

    let indices = vec![&env, 0u32];
    let result = client.try_release_milestones_batch(&contract_id, &client_addr, &indices);
    assert_contract_error(result, EscrowError::InsufficientApprovals);

    // Both approve — should succeed
    assert!(client.approve_milestone_release(&contract_id, &freelancer_addr, &0));
    assert!(client.release_milestones_batch(&contract_id, &client_addr, &indices));
}
