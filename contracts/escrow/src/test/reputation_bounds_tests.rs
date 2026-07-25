use super::{complete_contract, create_contract, register_client};
use crate::{EscrowError, ReleaseAuthorization};
use soroban_sdk::{Address, Env, String};

fn valid_comment(env: &Env) -> String {
    String::from_str(env, "Great job!")
}

#[test]
fn issue_reputation_rejects_invalid_contract_id_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let result = client.try_issue_reputation(&0, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn issue_reputation_rejects_invalid_contract_id_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create one contract so next_contract_id = 2
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    // Try to use contract_id = 2 (which is next_contract_id)
    let result = client.try_issue_reputation(&2, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::InvalidContractId);

    // Try to use contract_id = 100 (way out of bounds)
    let result = client.try_issue_reputation(&100, &client_addr, &5, &valid_comment(&env));
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn get_reputation_comment_rejects_invalid_contract_id_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let result = client.try_get_reputation_comment(&0);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn get_reputation_comment_rejects_invalid_contract_id_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create one contract so next_contract_id = 2
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    // Try to use contract_id = 2 (which is next_contract_id)
    let result = client.try_get_reputation_comment(&2);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn submit_work_evidence_rejects_invalid_contract_id_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let evidence = String::from_str(&env, "ipfs://QmHash");

    let result = client.try_submit_work_evidence(&0, &freelancer_addr, &0, &evidence);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn submit_work_evidence_rejects_invalid_contract_id_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let evidence = String::from_str(&env, "ipfs://QmHash");

    // Create one contract so next_contract_id = 2
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    // Try to use contract_id = 2 (which is next_contract_id)
    let result = client.try_submit_work_evidence(&2, &freelancer_addr, &0, &evidence);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn get_work_evidence_rejects_invalid_contract_id_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);

    let result = client.try_get_work_evidence(&0, &0);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn get_work_evidence_rejects_invalid_contract_id_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create one contract so next_contract_id = 2
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    // Try to use contract_id = 2 (which is next_contract_id)
    let result = client.try_get_work_evidence(&2, &0);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn raise_dispute_rejects_invalid_contract_id_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let caller = Address::generate(&env);

    let result = client.try_raise_dispute(&0, &caller);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn raise_dispute_rejects_invalid_contract_id_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    // Create one contract so next_contract_id = 2
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    // Try to use contract_id = 2 (which is next_contract_id)
    let result = client.try_raise_dispute(&2, &client_addr);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn resolve_dispute_rejects_invalid_contract_id_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let arbiter = Address::generate(&env);
    let resolution = crate::DisputeResolution::FullRefund;

    let result = client.try_resolve_dispute(&0, &arbiter, &resolution);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}

#[test]
fn resolve_dispute_rejects_invalid_contract_id_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = register_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let resolution = crate::DisputeResolution::FullRefund;

    // Create one contract so next_contract_id = 2
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    // Try to use contract_id = 2 (which is next_contract_id)
    let result = client.try_resolve_dispute(&2, &arbiter, &resolution);
    super::assert_contract_error(result, EscrowError::InvalidContractId);
}
