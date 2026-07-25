use crate::{ContractStatus, DepositMode, DisputeResolution, Error, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, vec, Address, Env, String};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

fn escrow_client<'a>(env: &'a Env, contract_id: &Address) -> EscrowClient<'a> {
    EscrowClient::new(env, contract_id)
}

fn completed_contract(env: &Env, client: &EscrowClient<'_>) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Completed
    );
    (client_addr, freelancer_addr, contract_id)
}

fn disputed_contract(env: &Env, client: &EscrowClient<'_>) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter = Address::generate(env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter.clone()),
        &vec![env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    assert!(client.raise_dispute(&contract_id, &client_addr));
    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Disputed
    );
    (client_addr, freelancer_addr, arbiter, contract_id)
}

#[test]
fn finalize_completed_contract_persists_immutable_close_record() {
    let (env, client) = setup();
    let client = escrow_client(&env, &client);
    env.ledger().with_mut(|li| li.timestamp = 1_717_171_717);
    let (client_addr, freelancer_addr, contract_id) = completed_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    let record = client
        .get_finalization_record(&contract_id)
        .expect("finalization record should exist");
    assert_eq!(record.finalizer, client_addr);
    assert_eq!(record.timestamp, 1_717_171_717);
    assert_eq!(record.summary.client, record.finalizer);
    assert_eq!(record.summary.freelancer, freelancer_addr);
    assert_eq!(record.summary.status, ContractStatus::Completed);
    assert_eq!(record.summary.total_amount, 100);
    assert_eq!(record.summary.funded_amount, 100);
    assert_eq!(record.summary.released_amount, 100);
    assert_eq!(record.summary.refundable_balance, 0);
    assert_eq!(record.summary.released_milestone_count, 1);
}

#[test]
fn finalize_disputed_contract_can_be_called_by_assigned_arbiter() {
    let (env, client) = setup();
    let client = escrow_client(&env, &client);
    let (_, _, arbiter, contract_id) = disputed_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &arbiter));

    let record = client
        .get_finalization_record(&contract_id)
        .expect("finalization record should exist");
    assert_eq!(record.finalizer, arbiter);
    assert_eq!(record.summary.status, ContractStatus::Disputed);
    assert_eq!(record.summary.refundable_balance, 100);
}

#[test]
fn finalize_completed_contract_can_be_called_by_freelancer() {
    let (env, client) = setup();
    let client = escrow_client(&env, &client);
    let (_, freelancer_addr, contract_id) = completed_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &freelancer_addr));

    let record = client
        .get_finalization_record(&contract_id)
        .expect("finalization record should exist");
    assert_eq!(record.finalizer, freelancer_addr);
    assert_eq!(record.summary.status, ContractStatus::Completed);
}

#[test]
fn finalize_rejects_unauthorized_finalizer() {
    let (env, client) = setup();
    let client = escrow_client(&env, &client);
    let (_, _, contract_id) = completed_contract(&env, &client);
    let outsider = Address::generate(&env);

    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &outsider),
        EscrowError::UnauthorizedRole,
    );
    assert!(client.get_finalization_record(&contract_id).is_none());
}

#[test]
fn finalize_rejects_non_terminal_contract() {
    let (env, client) = setup();
    let client = escrow_client(&env, &client);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        EscrowError::InvalidStatusTransition,
    );
    assert!(client.get_finalization_record(&contract_id).is_none());
}

#[test]
fn finalize_is_idempotent_guarded() {
    let (env, client) = setup();
    let client = escrow_client(&env, &client);
    let (client_addr, _, contract_id) = completed_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));
    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        EscrowError::AlreadyFinalized,
    );
}

#[test]
fn finalized_contract_rejects_subsequent_mutations() {
    let (env, client) = setup();
    let client = escrow_client(&env, &client);
    let (client_addr, freelancer_addr, contract_id) = completed_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    super::assert_contract_error(
        client.try_deposit_funds(&contract_id, &client_addr, &1_i128),
        EscrowError::AlreadyFinalized,
    );
    super::assert_contract_error(
        client.try_release_milestone(&contract_id, &client_addr, &0),
        EscrowError::AlreadyFinalized,
    );
    super::assert_contract_error(
        client.try_issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5_i128),
        EscrowError::AlreadyFinalized,
    );
    super::assert_contract_error(
        client.try_cancel_contract(&contract_id, &client_addr),
        EscrowError::AlreadyFinalized,
    );
}

#[test]
fn finalized_dispute_rejects_resolution() {
    let (env, client) = setup();
    let client = escrow_client(&env, &client);
    let (client_addr, _, arbiter, contract_id) = disputed_contract(&env, &client);

    assert!(client.finalize_contract(&contract_id, &client_addr));

    super::assert_contract_error(
        client.try_resolve_dispute(&contract_id, &arbiter, &DisputeResolution::FullRefund),
        EscrowError::AlreadyFinalized,
    );
    super::assert_contract_error(
        client.try_raise_dispute(&contract_id, &client_addr),
        EscrowError::AlreadyFinalized,
    );
}

#[test]
fn pause_blocks_finalization() {
    let (env, client) = setup();
    let client = escrow_client(&env, &client);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));
    let (client_addr, _, contract_id) = completed_contract(&env, &client);
    assert!(client.pause());

    super::assert_contract_error(
        client.try_finalize_contract(&contract_id, &client_addr),
        EscrowError::ContractPaused,
    );
    assert!(client.get_finalization_record(&contract_id).is_none());
}

// ---------------------------------------------------------------------------
// submit_work_evidence: overwrite and 256-byte length bound
// ---------------------------------------------------------------------------

/// Helper that produces a Soroban `String` from a plain `&str`.
fn ev(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

/// Helper that builds a funded contract and returns (client_addr, freelancer_addr, contract_id).
fn funded_contract(env: &Env, client: &EscrowClient<'_>) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    (client_addr, freelancer_addr, contract_id)
}

/// A successful `submit_work_evidence` call is round-tripped by `get_work_evidence`.
#[test]
fn submit_work_evidence_round_trips_via_get() {
    let (env, contract_addr) = setup();
    let client = escrow_client(&env, &contract_addr);
    let (_, freelancer_addr, contract_id) = funded_contract(&env, &client);

    let evidence = ev(&env, "ipfs://QmRoundTrip");
    assert!(client.submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence));

    let stored = client.get_work_evidence(&contract_id, &0);
    assert_eq!(stored, Some(evidence));
}

/// Overwriting evidence before release replaces the prior value,
/// confirmed by `get_work_evidence`.
#[test]
fn submit_work_evidence_overwrite_replaces_prior_value() {
    let (env, contract_addr) = setup();
    let client = escrow_client(&env, &contract_addr);
    let (_, freelancer_addr, contract_id) = funded_contract(&env, &client);

    let first = ev(&env, "ipfs://first-version");
    let second = ev(&env, "ipfs://second-version");
    assert!(client.submit_work_evidence(&contract_id, &freelancer_addr, &0, &first));
    assert!(client.submit_work_evidence(&contract_id, &freelancer_addr, &0, &second));

    // Only the latest value must be visible.
    let stored = client.get_work_evidence(&contract_id, &0);
    assert_eq!(stored, Some(second));
}

/// Exactly 256 bytes of evidence must be accepted.
#[test]
fn submit_work_evidence_accepts_exactly_256_bytes() {
    let (env, contract_addr) = setup();
    let client = escrow_client(&env, &contract_addr);
    let (_, freelancer_addr, contract_id) = funded_contract(&env, &client);

    let boundary = String::from_str(&env, &"x".repeat(256));
    assert!(client.submit_work_evidence(&contract_id, &freelancer_addr, &0, &boundary));

    let stored = client.get_work_evidence(&contract_id, &0);
    assert_eq!(stored.map(|s| s.len()), Some(256));
}

/// 257 bytes must be rejected with `EvidenceTooLong`.
#[test]
fn submit_work_evidence_rejects_257_bytes() {
    let (env, contract_addr) = setup();
    let client = escrow_client(&env, &contract_addr);
    let (_, freelancer_addr, contract_id) = funded_contract(&env, &client);

    let too_long = String::from_str(&env, &"x".repeat(257));
    let result = client.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &too_long);
    super::assert_contract_error(result, EscrowError::EvidenceTooLong);
}

/// Only the freelancer may submit evidence; any other caller gets `UnauthorizedRole`.
#[test]
fn submit_work_evidence_rejects_non_freelancer() {
    let (env, contract_addr) = setup();
    let client = escrow_client(&env, &contract_addr);
    let (client_addr, _, contract_id) = funded_contract(&env, &client);

    let evidence = ev(&env, "ipfs://QmUnauth");
    let result = client.try_submit_work_evidence(&contract_id, &client_addr, &0, &evidence);
    super::assert_contract_error(result, Error::UnauthorizedRole);
}

/// Submitting to an unfunded (Created) contract must fail with `InvalidState`.
#[test]
fn submit_work_evidence_rejects_unfunded_contract() {
    let (env, contract_addr) = setup();
    let client = escrow_client(&env, &contract_addr);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    // Create without depositing — remains in Created status.
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );

    let evidence = ev(&env, "ipfs://QmNoFunds");
    let result = client.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    super::assert_contract_error(result, Error::InvalidState);
}

/// `get_work_evidence` returns `None` for an out-of-bounds milestone index.
#[test]
fn get_work_evidence_returns_none_for_out_of_bounds_index() {
    let (env, contract_addr) = setup();
    let client = escrow_client(&env, &contract_addr);
    let (_, _, contract_id) = funded_contract(&env, &client);

    // milestone index 99 does not exist.
    let stored = client.get_work_evidence(&contract_id, &99);
    assert!(stored.is_none());
}

/// `get_work_evidence` returns `None` when no evidence has been submitted yet.
#[test]
fn get_work_evidence_returns_none_before_submission() {
    let (env, contract_addr) = setup();
    let client = escrow_client(&env, &contract_addr);
    let (_, _, contract_id) = funded_contract(&env, &client);

    assert!(client.get_work_evidence(&contract_id, &0).is_none());
}

/// A paused contract blocks `submit_work_evidence` with `ContractPaused`.
#[test]
fn submit_work_evidence_rejects_paused_contract() {
    let (env, contract_addr) = setup();
    let client = escrow_client(&env, &contract_addr);
    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));

    let (_, freelancer_addr, contract_id) = funded_contract(&env, &client);
    assert!(client.pause());

    let evidence = ev(&env, "ipfs://QmPaused");
    let result = client.try_submit_work_evidence(&contract_id, &freelancer_addr, &0, &evidence);
    super::assert_contract_error(result, EscrowError::ContractPaused);
}
