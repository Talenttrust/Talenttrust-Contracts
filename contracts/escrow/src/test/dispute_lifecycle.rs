extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env, Error, String};

use crate::{ContractStatus, DisputeOutcome, EscrowError};

use super::{default_milestones, generated_participants, register_client, total_milestone_amount};

fn contract_error(error: EscrowError) -> Error {
    Error::from_contract_error(error as u32)
}

fn setup_funded_with_admin() -> (Env, super::EscrowClient<'static>, u32, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);

    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));

    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));
    assert!(client.deposit_funds(&contract_id, &total_milestone_amount()));

    (env, client, contract_id, admin, client_addr, freelancer_addr)
}

#[test]
fn dispute_happy_path_open_evidence_resolve_payout_freelancer() {
    let (env, client, contract_id, admin, client_addr, freelancer_addr) = setup_funded_with_admin();

    let reason = String::from_str(&env, "work not delivered");
    assert!(client.open_dispute(&contract_id, &client_addr, &reason));

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Disputed);

    let dispute = client
        .get_dispute(&contract_id)
        .expect("dispute should exist");
    assert_eq!(dispute.record.initiator, client_addr);
    assert_eq!(dispute.evidence.len(), 0);
    assert_eq!(dispute.resolution.len(), 0);
    assert!(!dispute.paid_out);

    let client_uri = String::from_str(&env, "ipfs://client-proof");
    assert!(client.submit_dispute_evidence(&contract_id, &client_addr, &client_uri));

    let freelancer_uri = String::from_str(&env, "ipfs://freelancer-proof");
    assert!(client.submit_dispute_evidence(&contract_id, &freelancer_addr, &freelancer_uri));

    let after_evidence = client.get_dispute(&contract_id).unwrap();
    assert_eq!(after_evidence.evidence.len(), 2);

    assert!(client.resolve_dispute(&contract_id, &admin, &DisputeOutcome::Freelancer));

    let after_resolution = client.get_dispute(&contract_id).unwrap();
    assert_eq!(after_resolution.resolution.len(), 1);

    assert!(client.payout_dispute(&contract_id));

    let post_payout = client.get_contract(&contract_id);
    assert_eq!(post_payout.status, ContractStatus::Completed);
    assert_eq!(post_payout.released_amount, post_payout.funded_amount);

    let dispute_post = client.get_dispute(&contract_id).unwrap();
    assert!(dispute_post.paid_out);
}

#[test]
fn open_dispute_requires_funded_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let client = register_client(&env);
    let (client_addr, freelancer_addr) = generated_participants(&env);
    let contract_id = client.create_contract(&client_addr, &freelancer_addr, &default_milestones(&env));

    let reason = String::from_str(&env, "not funded yet");
    let result = client.try_open_dispute(&contract_id, &client_addr, &reason);
    assert_eq!(result, Err(Ok(contract_error(EscrowError::InvalidState))));
}

#[test]
fn open_dispute_requires_participant_authorization() {
    let (env, client, contract_id, _admin, _client_addr, _freelancer_addr) = setup_funded_with_admin();

    let stranger = Address::generate(&env);
    let reason = String::from_str(&env, "no standing");
    let result = client.try_open_dispute(&contract_id, &stranger, &reason);
    assert_eq!(result, Err(Ok(contract_error(EscrowError::Unauthorized))));
}

#[test]
fn cannot_open_second_dispute_for_same_contract() {
    let (env, client, contract_id, _admin, client_addr, _freelancer_addr) = setup_funded_with_admin();

    let reason = String::from_str(&env, "initial");
    assert!(client.open_dispute(&contract_id, &client_addr, &reason));

    let reason2 = String::from_str(&env, "again");
    let result = client.try_open_dispute(&contract_id, &client_addr, &reason2);
    assert_eq!(result, Err(Ok(contract_error(EscrowError::InvalidState))));
}

#[test]
fn resolve_dispute_requires_admin() {
    let (env, client, contract_id, _admin, client_addr, _freelancer_addr) = setup_funded_with_admin();

    let reason = String::from_str(&env, "needs resolution");
    assert!(client.open_dispute(&contract_id, &client_addr, &reason));

    let not_admin = Address::generate(&env);
    let result = client.try_resolve_dispute(&contract_id, &not_admin, &DisputeOutcome::Client);
    assert_eq!(result, Err(Ok(contract_error(EscrowError::Unauthorized))));
}

#[test]
fn payout_requires_resolution() {
    let (env, client, contract_id, _admin, client_addr, _freelancer_addr) = setup_funded_with_admin();

    let reason = String::from_str(&env, "needs payout");
    assert!(client.open_dispute(&contract_id, &client_addr, &reason));

    let result = client.try_payout_dispute(&contract_id);
    assert_eq!(result, Err(Ok(contract_error(EscrowError::DisputeNotResolved))));
}

#[test]
fn payout_split_validates_amount_range() {
    let (env, client, contract_id, admin, client_addr, _freelancer_addr) = setup_funded_with_admin();

    let reason = String::from_str(&env, "split case");
    assert!(client.open_dispute(&contract_id, &client_addr, &reason));

    assert!(client.resolve_dispute(&contract_id, &admin, &DisputeOutcome::Split(total_milestone_amount() + 1)));

    let result = client.try_payout_dispute(&contract_id);
    assert_eq!(result, Err(Ok(contract_error(EscrowError::InvalidAmount))));
}

#[test]
fn payout_cannot_be_called_twice() {
    let (env, client, contract_id, admin, client_addr, _freelancer_addr) = setup_funded_with_admin();

    let reason = String::from_str(&env, "double payout");
    assert!(client.open_dispute(&contract_id, &client_addr, &reason));

    assert!(client.resolve_dispute(&contract_id, &admin, &DisputeOutcome::Client));
    assert!(client.payout_dispute(&contract_id));

    let result = client.try_payout_dispute(&contract_id);
    assert_eq!(result, Err(Ok(contract_error(EscrowError::InvalidState))));
}

#[test]
#[should_panic(expected = "dispute already resolved")]
fn evidence_submission_after_resolution_panics() {
    let (env, client, contract_id, admin, client_addr, _freelancer_addr) = setup_funded_with_admin();

    let reason = String::from_str(&env, "late evidence");
    assert!(client.open_dispute(&contract_id, &client_addr, &reason));

    assert!(client.resolve_dispute(&contract_id, &admin, &DisputeOutcome::Client));

    let uri = String::from_str(&env, "ipfs://too-late");
    let _ = client.submit_dispute_evidence(&contract_id, &client_addr, &uri);
}
