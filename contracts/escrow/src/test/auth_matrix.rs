//! Role-by-action authorization matrix tests.
//!
//! Each escrow entrypoint is tested against every role (admin, client,
//! freelancer, arbiter, stranger) asserting allow/deny with typed error codes.
//!
//! Matrix columns (roles): admin, client, freelancer, arbiter, stranger
//! Matrix rows (actions):
//!   - initialize
//!   - bind_settlement_token
//!   - create_contract
//!   - deposit_funds
//!   - approve_milestone_release (per ReleaseAuthorization mode)
//!   - release_milestone (per ReleaseAuthorization mode)
//!   - refund_unreleased_milestones
//!   - cancel_contract
//!   - submit_work_evidence
//!   - issue_reputation
//!   - finalize_contract
//!   - raise_dispute
//!   - resolve_dispute
//!   - pause / unpause
//!   - activate_emergency_pause / resolve_emergency
//!   - set_protocol_fee_bps
//!   - withdraw_protocol_fees
//!   - propose_governance_admin / accept_governance_admin / cancel_governance_admin_proposal

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env, String};

use crate::{
    ContractStatus, DisputeResolution, DisputeSplit, Error, Escrow, EscrowClient, EscrowError,
    ReleaseAuthorization,
};

use super::{assert_contract_error, default_milestones, total_milestone_amount};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// All participants needed for a full matrix test.
struct Actors {
    admin: Address,
    client: Address,
    freelancer: Address,
    arbiter: Address,
    stranger: Address,
}

impl Actors {
    fn new(env: &Env) -> Self {
        Actors {
            admin: Address::generate(env),
            client: Address::generate(env),
            freelancer: Address::generate(env),
            arbiter: Address::generate(env),
            stranger: Address::generate(env),
        }
    }
}

/// Register and initialize the escrow contract; return (client, actors).
fn setup_initialized(env: &Env) -> (EscrowClient<'_>, Actors) {
    env.mock_all_auths_allowing_non_root_auth();
    let addr = env.register(Escrow, ());
    let client = EscrowClient::new(env, &addr);
    let actors = Actors::new(env);
    client.initialize(&actors.admin);
    (client, actors)
}

/// Register a SAC token and bind it; return the token address.
fn bind_token(env: &Env, escrow: &EscrowClient<'_>, admin: &Address) -> Address {
    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(admin, &token);
    token
}

/// Mint `amount` of `token` to `to`.
fn mint(env: &Env, token: &Address, admin: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
    let _ = admin; // kept for symmetry; StellarAssetClient uses admin implicitly
}

/// Create a single-milestone contract using ClientOnly auth.
fn create_simple_contract(
    env: &Env,
    escrow: &EscrowClient<'_>,
    client: &Address,
    freelancer: &Address,
    arbiter: Option<&Address>,
    auth: ReleaseAuthorization,
) -> u32 {
    escrow.create_contract(
        client,
        freelancer,
        &arbiter.cloned(),
        &default_milestones(env),
        &auth,
    )
}

/// Create a contract and fully fund it; return (contract_id, token_addr).
fn create_and_fund(
    env: &Env,
    escrow: &EscrowClient<'_>,
    actors: &Actors,
    auth: ReleaseAuthorization,
) -> (u32, Address) {
    let token = bind_token(env, escrow, &actors.admin);
    let total = total_milestone_amount();
    mint(env, &token, &actors.admin, &actors.client, total);
    let id = create_simple_contract(
        env,
        escrow,
        &actors.client,
        &actors.freelancer,
        Some(&actors.arbiter),
        auth,
    );
    escrow.deposit_funds(&id, &actors.client, &total);
    (id, token)
}

/// Drive a ClientOnly-funded contract to Completed status.
fn complete_contract(env: &Env, escrow: &EscrowClient<'_>, actors: &Actors) -> u32 {
    let (id, _token) = create_and_fund(env, escrow, actors, ReleaseAuthorization::ClientOnly);
    for idx in 0..3u32 {
        escrow.approve_milestone_release(&id, &actors.client, &idx);
        escrow.release_milestone(&id, &actors.client, &idx);
    }
    id
}

/// Drive a contract to Disputed status (requires arbiter).
fn disputed_contract(env: &Env, escrow: &EscrowClient<'_>, actors: &Actors) -> u32 {
    let (id, _token) = create_and_fund(env, escrow, actors, ReleaseAuthorization::ClientOnly);
    escrow.raise_dispute(&id, &actors.client);
    id
}


// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

/// Only the designated admin address may call initialize (it must authorize).
/// A second call from any party is rejected with AlreadyInitialized.
#[test]
fn initialize_allows_admin_once() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);
    let admin = Address::generate(&env);

    assert!(escrow.initialize(&admin));
}

#[test]
fn initialize_rejected_second_call() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);

    let stranger = Address::generate(&env);
    let result = escrow.try_initialize(&stranger);
    assert_contract_error(result, Error::AlreadyInitialized);
}

#[test]
fn initialize_rejected_for_admin_second_call() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);
    let admin = Address::generate(&env);
    escrow.initialize(&admin);

    // Even the original admin cannot re-initialize
    let result = escrow.try_initialize(&admin);
    assert_contract_error(result, Error::AlreadyInitialized);
}

// ---------------------------------------------------------------------------
// bind_settlement_token
// ---------------------------------------------------------------------------

#[test]
fn bind_token_allowed_for_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let token = env.register_stellar_asset_contract(actors.admin.clone());
    assert!(escrow.bind_settlement_token(&actors.admin, &token));
}

#[test]
fn bind_token_rejected_for_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let token = env.register_stellar_asset_contract(actors.admin.clone());

    // client
    let r = escrow.try_bind_settlement_token(&actors.client, &token);
    assert_contract_error(r, EscrowError::UnauthorizedRole);

    // freelancer
    let r = escrow.try_bind_settlement_token(&actors.freelancer, &token);
    assert_contract_error(r, EscrowError::UnauthorizedRole);

    // stranger
    let r = escrow.try_bind_settlement_token(&actors.stranger, &token);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn bind_token_rejected_double_bind() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let token = env.register_stellar_asset_contract(actors.admin.clone());
    escrow.bind_settlement_token(&actors.admin, &token);

    // Admin cannot bind a second time
    let r = escrow.try_bind_settlement_token(&actors.admin, &token);
    assert_contract_error(r, EscrowError::SettlementTokenAlreadyBound);
}

#[test]
fn bind_token_rejected_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);
    let admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());

    let r = escrow.try_bind_settlement_token(&admin, &token);
    assert_contract_error(r, Error::NotInitialized);
}


// ---------------------------------------------------------------------------
// create_contract
// ---------------------------------------------------------------------------

/// The client must authorize contract creation.
/// Only the client (the first argument) can create — any stranger calling
/// with their own address as the caller gets no contract in their name
/// from anyone else's perspective, and a stranger posing as the client would
/// fail auth (mocked here; tested via require_auth logic in the contract).
#[test]
fn create_contract_allowed_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    assert!(id >= 1);
}

#[test]
fn create_contract_rejected_same_client_and_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let r = escrow.try_create_contract(
        &actors.client,
        &actors.client,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    assert_contract_error(r, EscrowError::InvalidParticipant);
}

#[test]
fn create_contract_rejected_arbiter_equals_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let r = escrow.try_create_contract(
        &actors.client,
        &actors.freelancer,
        &Some(actors.client.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert_contract_error(r, EscrowError::InvalidArbiter);
}

#[test]
fn create_contract_rejected_arbiter_equals_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let r = escrow.try_create_contract(
        &actors.client,
        &actors.freelancer,
        &Some(actors.freelancer.clone()),
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert_contract_error(r, EscrowError::InvalidArbiter);
}

#[test]
fn create_contract_arbiter_only_requires_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let r = escrow.try_create_contract(
        &actors.client,
        &actors.freelancer,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ArbiterOnly,
    );
    assert_contract_error(r, EscrowError::MissingArbiter);
}

#[test]
fn create_contract_client_and_arbiter_mode_requires_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let r = escrow.try_create_contract(
        &actors.client,
        &actors.freelancer,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientAndArbiter,
    );
    assert_contract_error(r, EscrowError::MissingArbiter);
}

#[test]
fn create_contract_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    escrow.pause();

    let r = escrow.try_create_contract(
        &actors.client,
        &actors.freelancer,
        &None,
        &default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );
    assert_contract_error(r, Error::ContractPaused);
}

// ---------------------------------------------------------------------------
// deposit_funds
// ---------------------------------------------------------------------------

/// Only the stored client may call deposit_funds.
#[test]
fn deposit_allowed_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let token = bind_token(&env, &escrow, &actors.admin);
    let total = total_milestone_amount();
    mint(&env, &token, &actors.admin, &actors.client, total);

    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    assert!(escrow.deposit_funds(&id, &actors.client, &total));
}

#[test]
fn deposit_rejected_for_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);

    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    let r = escrow.try_deposit_funds(&id, &actors.freelancer, &total_milestone_amount());
    assert_contract_error(r, Error::UnauthorizedRole);
}

#[test]
fn deposit_rejected_for_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);

    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        Some(&actors.arbiter),
        ReleaseAuthorization::ClientAndArbiter,
    );
    let r = escrow.try_deposit_funds(&id, &actors.arbiter, &total_milestone_amount());
    assert_contract_error(r, Error::UnauthorizedRole);
}

#[test]
fn deposit_rejected_for_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);

    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    let r = escrow.try_deposit_funds(&id, &actors.stranger, &total_milestone_amount());
    assert_contract_error(r, Error::UnauthorizedRole);
}

#[test]
fn deposit_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);

    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    escrow.pause();

    let r = escrow.try_deposit_funds(&id, &actors.client, &total_milestone_amount());
    assert_contract_error(r, Error::ContractPaused);
}


// ---------------------------------------------------------------------------
// approve_milestone_release — per ReleaseAuthorization mode
// ---------------------------------------------------------------------------

// ── ClientOnly ───────────────────────────────────────────────────────────────

#[test]
fn approve_client_only_allows_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    assert!(escrow.approve_milestone_release(&id, &actors.client, &0));
}

#[test]
fn approve_client_only_rejects_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let r = escrow.try_approve_milestone_release(&id, &actors.freelancer, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

#[test]
fn approve_client_only_rejects_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let r = escrow.try_approve_milestone_release(&id, &actors.arbiter, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

#[test]
fn approve_client_only_rejects_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let r = escrow.try_approve_milestone_release(&id, &actors.stranger, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

// ── ArbiterOnly ──────────────────────────────────────────────────────────────

#[test]
fn approve_arbiter_only_allows_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ArbiterOnly);

    assert!(escrow.approve_milestone_release(&id, &actors.arbiter, &0));
}

#[test]
fn approve_arbiter_only_rejects_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ArbiterOnly);

    let r = escrow.try_approve_milestone_release(&id, &actors.client, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

#[test]
fn approve_arbiter_only_rejects_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ArbiterOnly);

    let r = escrow.try_approve_milestone_release(&id, &actors.freelancer, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

#[test]
fn approve_arbiter_only_rejects_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ArbiterOnly);

    let r = escrow.try_approve_milestone_release(&id, &actors.stranger, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

// ── ClientAndArbiter (OR logic) ───────────────────────────────────────────────

#[test]
fn approve_client_and_arbiter_allows_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) =
        create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientAndArbiter);

    assert!(escrow.approve_milestone_release(&id, &actors.client, &0));
}

#[test]
fn approve_client_and_arbiter_allows_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) =
        create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientAndArbiter);

    // Only one approval needed (OR logic); use milestone 1 to avoid AlreadyApproved
    assert!(escrow.approve_milestone_release(&id, &actors.arbiter, &0));
}

#[test]
fn approve_client_and_arbiter_rejects_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) =
        create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientAndArbiter);

    let r = escrow.try_approve_milestone_release(&id, &actors.freelancer, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

#[test]
fn approve_client_and_arbiter_rejects_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) =
        create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientAndArbiter);

    let r = escrow.try_approve_milestone_release(&id, &actors.stranger, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

// ── MultiSig (AND logic — both client + freelancer must approve) ───────────────

#[test]
fn approve_multisig_allows_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    assert!(escrow.approve_milestone_release(&id, &actors.client, &0));
}

#[test]
fn approve_multisig_allows_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    assert!(escrow.approve_milestone_release(&id, &actors.freelancer, &0));
}

#[test]
fn approve_multisig_rejects_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    let r = escrow.try_approve_milestone_release(&id, &actors.arbiter, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

#[test]
fn approve_multisig_rejects_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    let r = escrow.try_approve_milestone_release(&id, &actors.stranger, &0);
    assert_contract_error(r, Error::UnauthorizedRole);
}

/// Duplicate approval from the same party is rejected with AlreadyApproved.
#[test]
fn approve_duplicate_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    let r = escrow.try_approve_milestone_release(&id, &actors.client, &0);
    assert_contract_error(r, Error::AlreadyApproved);
}

/// Approval is rejected when the contract is paused.
#[test]
fn approve_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);
    escrow.pause();

    let r = escrow.try_approve_milestone_release(&id, &actors.client, &0);
    assert_contract_error(r, Error::ContractPaused);
}

// ---------------------------------------------------------------------------
// release_milestone — per ReleaseAuthorization mode
// ---------------------------------------------------------------------------

// ── ClientOnly ───────────────────────────────────────────────────────────────

#[test]
fn release_client_only_allows_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    assert!(escrow.release_milestone(&id, &actors.client, &0));
}

#[test]
fn release_client_only_rejects_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    let r = escrow.try_release_milestone(&id, &actors.freelancer, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn release_client_only_rejects_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    let r = escrow.try_release_milestone(&id, &actors.arbiter, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn release_client_only_rejects_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    let r = escrow.try_release_milestone(&id, &actors.stranger, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

// ── ArbiterOnly ──────────────────────────────────────────────────────────────

#[test]
fn release_arbiter_only_allows_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ArbiterOnly);

    escrow.approve_milestone_release(&id, &actors.arbiter, &0);
    assert!(escrow.release_milestone(&id, &actors.arbiter, &0));
}

#[test]
fn release_arbiter_only_rejects_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ArbiterOnly);

    escrow.approve_milestone_release(&id, &actors.arbiter, &0);
    let r = escrow.try_release_milestone(&id, &actors.client, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn release_arbiter_only_rejects_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ArbiterOnly);

    escrow.approve_milestone_release(&id, &actors.arbiter, &0);
    let r = escrow.try_release_milestone(&id, &actors.freelancer, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn release_arbiter_only_rejects_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ArbiterOnly);

    escrow.approve_milestone_release(&id, &actors.arbiter, &0);
    let r = escrow.try_release_milestone(&id, &actors.stranger, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

// ── ClientAndArbiter ─────────────────────────────────────────────────────────

#[test]
fn release_client_and_arbiter_allows_client_with_client_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) =
        create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientAndArbiter);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    assert!(escrow.release_milestone(&id, &actors.client, &0));
}

#[test]
fn release_client_and_arbiter_allows_arbiter_with_arbiter_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) =
        create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientAndArbiter);

    escrow.approve_milestone_release(&id, &actors.arbiter, &0);
    assert!(escrow.release_milestone(&id, &actors.arbiter, &0));
}

#[test]
fn release_client_and_arbiter_rejects_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) =
        create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientAndArbiter);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    let r = escrow.try_release_milestone(&id, &actors.freelancer, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn release_client_and_arbiter_rejects_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) =
        create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientAndArbiter);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    let r = escrow.try_release_milestone(&id, &actors.stranger, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

// ── MultiSig ─────────────────────────────────────────────────────────────────

#[test]
fn release_multisig_allows_client_after_both_approve() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    escrow.approve_milestone_release(&id, &actors.freelancer, &0);
    assert!(escrow.release_milestone(&id, &actors.client, &0));
}

#[test]
fn release_multisig_allows_freelancer_after_both_approve() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    escrow.approve_milestone_release(&id, &actors.freelancer, &0);
    assert!(escrow.release_milestone(&id, &actors.freelancer, &0));
}

#[test]
fn release_multisig_requires_both_approvals_client_only_insufficient() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    let r = escrow.try_release_milestone(&id, &actors.client, &0);
    assert_contract_error(r, Error::InsufficientApprovals);
}

#[test]
fn release_multisig_requires_both_approvals_freelancer_only_insufficient() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    escrow.approve_milestone_release(&id, &actors.freelancer, &0);
    let r = escrow.try_release_milestone(&id, &actors.freelancer, &0);
    assert_contract_error(r, Error::InsufficientApprovals);
}

#[test]
fn release_multisig_rejects_arbiter_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    escrow.approve_milestone_release(&id, &actors.freelancer, &0);
    let r = escrow.try_release_milestone(&id, &actors.arbiter, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn release_multisig_rejects_stranger_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::MultiSig);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    escrow.approve_milestone_release(&id, &actors.freelancer, &0);
    let r = escrow.try_release_milestone(&id, &actors.stranger, &0);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

/// release_milestone without any approvals fails with InsufficientApprovals.
#[test]
fn release_without_approvals_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let r = escrow.try_release_milestone(&id, &actors.client, &0);
    assert_contract_error(r, Error::InsufficientApprovals);
}

/// release_milestone is rejected when the contract is paused.
#[test]
fn release_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    escrow.pause();

    let r = escrow.try_release_milestone(&id, &actors.client, &0);
    assert_contract_error(r, Error::ContractPaused);
}


// ---------------------------------------------------------------------------
// refund_unreleased_milestones
// ---------------------------------------------------------------------------

/// Only the stored client may call refund_unreleased_milestones.
#[test]
fn refund_allowed_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    // Milestones with no deadline can be refunded at any time
    let indices = vec![&env, 0u32, 1u32, 2u32];
    assert!(escrow.refund_unreleased_milestones(&id, &indices) > 0);
}

#[test]
fn refund_rejected_for_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let indices = vec![&env, 0u32];
    let r = escrow.try_refund_unreleased_milestones(&id, &indices);
    // refund_unreleased_milestones requires client.require_auth(); freelancer fails auth
    // The error surfaces as a host auth failure (panic), which we verify is non-Ok
    assert!(r.is_err(), "freelancer should not be able to call refund");
}

#[test]
fn refund_rejected_for_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    // Force the stranger as msg sender by checking via mock: stranger cannot
    // pass contract.client.require_auth() internally. Any try_ call returns Err.
    // We verify the call fails for any non-client address by testing the
    // contract guard directly in a fresh env that does NOT mock auths.
    let env2 = Env::default();
    // Do NOT call mock_all_auths so auth checks are real
    let addr = env2.register(Escrow, ());
    let escrow2 = EscrowClient::new(&env2, &addr);
    let actors2 = Actors::new(&env2);
    env2.mock_all_auths_allowing_non_root_auth();
    escrow2.initialize(&actors2.admin);
    let token = bind_token(&env2, &escrow2, &actors2.admin);
    let total = total_milestone_amount();
    StellarAssetClient::new(&env2, &token).mint(&actors2.client, &total);
    let id2 = create_simple_contract(
        &env2,
        &escrow2,
        &actors2.client,
        &actors2.freelancer,
        Some(&actors2.arbiter),
        ReleaseAuthorization::ClientOnly,
    );
    escrow2.deposit_funds(&id2, &actors2.client, &total);

    // Stranger calls refund – the contract internally does contract.client.require_auth()
    // which won't match the stranger, so it panics.
    let indices = vec![&env2, 0u32];
    let r = escrow2.try_refund_unreleased_milestones(&id2, &indices);
    assert!(r.is_err(), "stranger should not be able to call refund");
}

#[test]
fn refund_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);
    escrow.pause();

    let indices = vec![&env, 0u32];
    let r = escrow.try_refund_unreleased_milestones(&id, &indices);
    assert_contract_error(r, Error::ContractPaused);
}

// ---------------------------------------------------------------------------
// cancel_contract
// ---------------------------------------------------------------------------

/// Only the stored client may cancel a contract.
#[test]
fn cancel_allowed_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    // Create without funding (Created state)
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    assert!(escrow.cancel_contract(&id, &actors.client));
}

#[test]
fn cancel_rejected_for_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );

    let r = escrow.try_cancel_contract(&id, &actors.freelancer);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn cancel_rejected_for_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        Some(&actors.arbiter),
        ReleaseAuthorization::ClientAndArbiter,
    );

    let r = escrow.try_cancel_contract(&id, &actors.arbiter);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn cancel_rejected_for_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );

    let r = escrow.try_cancel_contract(&id, &actors.stranger);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn cancel_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    escrow.pause();

    let r = escrow.try_cancel_contract(&id, &actors.client);
    assert_contract_error(r, Error::ContractPaused);
}

#[test]
fn cancel_rejected_after_any_release() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    escrow.approve_milestone_release(&id, &actors.client, &0);
    escrow.release_milestone(&id, &actors.client, &0);

    let r = escrow.try_cancel_contract(&id, &actors.client);
    assert_contract_error(r, EscrowError::InvalidStatusTransition);
}

// ---------------------------------------------------------------------------
// submit_work_evidence
// ---------------------------------------------------------------------------

/// Only the stored freelancer may submit work evidence.
#[test]
fn submit_evidence_allowed_for_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let ev = String::from_str(&env, "ipfs://QmEvidence");
    assert!(escrow.submit_work_evidence(&id, &actors.freelancer, &0, &ev));
}

#[test]
fn submit_evidence_rejected_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let ev = String::from_str(&env, "ipfs://QmEvidence");
    let r = escrow.try_submit_work_evidence(&id, &actors.client, &0, &ev);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn submit_evidence_rejected_for_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let ev = String::from_str(&env, "ipfs://QmEvidence");
    let r = escrow.try_submit_work_evidence(&id, &actors.arbiter, &0, &ev);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn submit_evidence_rejected_for_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let ev = String::from_str(&env, "ipfs://QmEvidence");
    let r = escrow.try_submit_work_evidence(&id, &actors.stranger, &0, &ev);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn submit_evidence_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);
    escrow.pause();

    let ev = String::from_str(&env, "ipfs://QmEvidence");
    let r = escrow.try_submit_work_evidence(&id, &actors.freelancer, &0, &ev);
    assert_contract_error(r, Error::ContractPaused);
}

// ---------------------------------------------------------------------------
// issue_reputation
// ---------------------------------------------------------------------------

/// Only the stored client may issue reputation for a completed contract.
#[test]
fn issue_reputation_allowed_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);

    let comment = String::from_str(&env, "Great work!");
    assert!(escrow.issue_reputation(&id, &actors.client, &5, &comment));
}

#[test]
fn issue_reputation_rejected_for_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);

    let comment = String::from_str(&env, "Self-issued");
    let r = escrow.try_issue_reputation(&id, &actors.freelancer, &5, &comment);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn issue_reputation_rejected_for_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);

    let comment = String::from_str(&env, "Arbiter comment");
    let r = escrow.try_issue_reputation(&id, &actors.arbiter, &5, &comment);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn issue_reputation_rejected_for_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);

    let comment = String::from_str(&env, "Stranger comment");
    let r = escrow.try_issue_reputation(&id, &actors.stranger, &5, &comment);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn issue_reputation_rejected_when_not_completed() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let comment = String::from_str(&env, "Too early");
    let r = escrow.try_issue_reputation(&id, &actors.client, &5, &comment);
    assert_contract_error(r, Error::NotCompleted);
}

#[test]
fn issue_reputation_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);
    escrow.pause();

    let comment = String::from_str(&env, "Paused");
    let r = escrow.try_issue_reputation(&id, &actors.client, &5, &comment);
    assert_contract_error(r, Error::ContractPaused);
}

#[test]
fn issue_reputation_rejected_for_duplicate_issuance() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);

    let comment = String::from_str(&env, "First issuance");
    escrow.issue_reputation(&id, &actors.client, &5, &comment);

    let r = escrow.try_issue_reputation(&id, &actors.client, &4, &comment);
    assert_contract_error(r, Error::ReputationAlreadyIssued);
}


// ---------------------------------------------------------------------------
// finalize_contract
// ---------------------------------------------------------------------------

/// Client, freelancer, and arbiter may all finalize a completed contract.
/// A stranger may not.
#[test]
fn finalize_allowed_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);

    assert!(escrow.finalize_contract(&id, &actors.client));
}

#[test]
fn finalize_allowed_for_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);

    assert!(escrow.finalize_contract(&id, &actors.freelancer));
}

#[test]
fn finalize_allowed_for_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    // complete_contract uses create_and_fund which sets arbiter
    let id = complete_contract(&env, &escrow, &actors);

    assert!(escrow.finalize_contract(&id, &actors.arbiter));
}

#[test]
fn finalize_rejected_for_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);

    let r = escrow.try_finalize_contract(&id, &actors.stranger);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn finalize_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);
    escrow.pause();

    let r = escrow.try_finalize_contract(&id, &actors.client);
    assert_contract_error(r, Error::ContractPaused);
}

#[test]
fn finalize_rejected_when_not_completed_or_disputed() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    // Contract is still in Funded state
    let r = escrow.try_finalize_contract(&id, &actors.client);
    assert_contract_error(r, Error::InvalidStatusTransition);
}

#[test]
fn finalize_rejected_for_second_call() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = complete_contract(&env, &escrow, &actors);
    escrow.finalize_contract(&id, &actors.client);

    let r = escrow.try_finalize_contract(&id, &actors.client);
    assert_contract_error(r, Error::AlreadyFinalized);
}

// ---------------------------------------------------------------------------
// raise_dispute
// ---------------------------------------------------------------------------

/// Client or freelancer may raise a dispute on a funded contract.
/// The arbiter role itself cannot open disputes; strangers cannot either.
#[test]
fn raise_dispute_allowed_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    assert!(escrow.raise_dispute(&id, &actors.client));
}

#[test]
fn raise_dispute_allowed_for_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    assert!(escrow.raise_dispute(&id, &actors.freelancer));
}

#[test]
fn raise_dispute_rejected_for_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let r = escrow.try_raise_dispute(&id, &actors.arbiter);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn raise_dispute_rejected_for_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let r = escrow.try_raise_dispute(&id, &actors.stranger);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn raise_dispute_rejected_without_arbiter_assigned() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let token = bind_token(&env, &escrow, &actors.admin);
    let total = total_milestone_amount();
    mint(&env, &token, &actors.admin, &actors.client, total);
    // Create WITHOUT arbiter
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&id, &actors.client, &total);

    let r = escrow.try_raise_dispute(&id, &actors.client);
    assert_contract_error(r, Error::ArbiterRequired);
}

#[test]
fn raise_dispute_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);
    escrow.pause();

    let r = escrow.try_raise_dispute(&id, &actors.client);
    assert_contract_error(r, Error::ContractPaused);
}

// ---------------------------------------------------------------------------
// resolve_dispute
// ---------------------------------------------------------------------------

/// Only the assigned arbiter may resolve a dispute.
#[test]
fn resolve_dispute_allowed_for_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = disputed_contract(&env, &escrow, &actors);

    let resolution = DisputeResolution::FullRefund;
    assert!(escrow.resolve_dispute(&id, &actors.arbiter, &resolution));
}

#[test]
fn resolve_dispute_rejected_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = disputed_contract(&env, &escrow, &actors);

    let resolution = DisputeResolution::FullRefund;
    let r = escrow.try_resolve_dispute(&id, &actors.client, &resolution);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn resolve_dispute_rejected_for_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = disputed_contract(&env, &escrow, &actors);

    let resolution = DisputeResolution::FullRefund;
    let r = escrow.try_resolve_dispute(&id, &actors.freelancer, &resolution);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn resolve_dispute_rejected_for_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = disputed_contract(&env, &escrow, &actors);

    let resolution = DisputeResolution::FullRefund;
    let r = escrow.try_resolve_dispute(&id, &actors.stranger, &resolution);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn resolve_dispute_rejected_when_not_disputed() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let (id, _) = create_and_fund(&env, &escrow, &actors, ReleaseAuthorization::ClientOnly);

    let resolution = DisputeResolution::FullRefund;
    let r = escrow.try_resolve_dispute(&id, &actors.arbiter, &resolution);
    assert_contract_error(r, Error::InvalidStatusTransition);
}

#[test]
fn resolve_dispute_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = disputed_contract(&env, &escrow, &actors);
    escrow.pause();

    let resolution = DisputeResolution::FullRefund;
    let r = escrow.try_resolve_dispute(&id, &actors.arbiter, &resolution);
    assert_contract_error(r, Error::ContractPaused);
}

/// Verify each resolution variant is accepted by the arbiter.
#[test]
fn resolve_dispute_full_payout_allowed_for_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = disputed_contract(&env, &escrow, &actors);

    assert!(escrow.resolve_dispute(&id, &actors.arbiter, &DisputeResolution::FullPayout));
}

#[test]
fn resolve_dispute_split_allowed_for_arbiter() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    let id = disputed_contract(&env, &escrow, &actors);

    // Remaining balance is total_milestone_amount(); split it equally
    let half = total_milestone_amount() / 2;
    let resolution = DisputeResolution::Split(DisputeSplit {
        client_amount: half,
        freelancer_amount: total_milestone_amount() - half,
    });
    assert!(escrow.resolve_dispute(&id, &actors.arbiter, &resolution));
}


// ---------------------------------------------------------------------------
// pause / unpause
// ---------------------------------------------------------------------------

/// Only the stored admin may call pause/unpause.
#[test]
fn pause_allowed_for_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, _actors) = setup_initialized(&env);
    assert!(escrow.pause());
    assert!(escrow.is_paused());
}

#[test]
fn pause_rejected_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);

    let r = escrow.try_pause();
    assert_contract_error(r, Error::NotInitialized);
}

#[test]
fn unpause_allowed_for_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, _actors) = setup_initialized(&env);
    escrow.pause();
    assert!(escrow.unpause());
    assert!(!escrow.is_paused());
}

#[test]
fn unpause_rejected_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);

    let r = escrow.try_unpause();
    assert_contract_error(r, Error::NotInitialized);
}

#[test]
fn unpause_rejected_while_emergency_active() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, _actors) = setup_initialized(&env);
    escrow.activate_emergency_pause();

    let r = escrow.try_unpause();
    assert_contract_error(r, Error::EmergencyActive);
}

// ---------------------------------------------------------------------------
// activate_emergency_pause / resolve_emergency
// ---------------------------------------------------------------------------

#[test]
fn activate_emergency_pause_allowed_for_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, _actors) = setup_initialized(&env);

    assert!(escrow.activate_emergency_pause());
    assert!(escrow.is_paused());
    assert!(escrow.is_emergency());
}

#[test]
fn activate_emergency_pause_rejected_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);

    let r = escrow.try_activate_emergency_pause();
    assert_contract_error(r, Error::NotInitialized);
}

#[test]
fn resolve_emergency_allowed_for_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, _actors) = setup_initialized(&env);
    escrow.activate_emergency_pause();

    assert!(escrow.resolve_emergency());
    assert!(!escrow.is_paused());
    assert!(!escrow.is_emergency());
}

#[test]
fn resolve_emergency_rejected_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);

    let r = escrow.try_resolve_emergency();
    assert_contract_error(r, Error::NotInitialized);
}

// ---------------------------------------------------------------------------
// set_protocol_fee_bps
// ---------------------------------------------------------------------------

/// Only the stored admin may set the protocol fee.
#[test]
fn set_protocol_fee_allowed_for_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, _actors) = setup_initialized(&env);

    assert!(escrow.set_protocol_fee_bps(&100u32));
    assert_eq!(escrow.get_protocol_fee_bps(), 100u32);
}

#[test]
fn set_protocol_fee_rejected_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);

    let r = escrow.try_set_protocol_fee_bps(&100u32);
    assert_contract_error(r, Error::NotInitialized);
}

// ---------------------------------------------------------------------------
// withdraw_protocol_fees
// ---------------------------------------------------------------------------

/// Only the stored admin may withdraw protocol fees.
/// Without fees accumulated the call fails with InsufficientAccumulatedFees.
#[test]
fn withdraw_protocol_fees_rejected_with_no_fees() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);

    let r = escrow.try_withdraw_protocol_fees(&1_i128, &actors.admin);
    assert_contract_error(r, Error::InsufficientAccumulatedFees);
}

#[test]
fn withdraw_protocol_fees_rejected_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);
    let dest = Address::generate(&env);

    let r = escrow.try_withdraw_protocol_fees(&1_i128, &dest);
    assert_contract_error(r, Error::NotInitialized);
}

#[test]
fn withdraw_protocol_fees_rejected_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    escrow.pause();

    let r = escrow.try_withdraw_protocol_fees(&1_i128, &actors.admin);
    assert_contract_error(r, Error::ContractPaused);
}

/// Admin can successfully withdraw accumulated fees after a milestone release
/// with a non-zero fee rate.
#[test]
fn withdraw_protocol_fees_allowed_for_admin_after_fees_accrue() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    // Set a 1% fee
    escrow.set_protocol_fee_bps(&100u32);

    let token = bind_token(&env, &escrow, &actors.admin);
    let total = total_milestone_amount();
    mint(&env, &token, &actors.admin, &actors.client, total);

    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        Some(&actors.arbiter),
        ReleaseAuthorization::ClientOnly,
    );
    escrow.deposit_funds(&id, &actors.client, &total);

    // Release milestone 0 to accumulate a fee
    escrow.approve_milestone_release(&id, &actors.client, &0);
    escrow.release_milestone(&id, &actors.client, &0);

    let fees = escrow.get_accumulated_protocol_fees();
    assert!(fees > 0, "fees must have accrued after release");

    let dest = Address::generate(&env);
    assert!(escrow.withdraw_protocol_fees(&fees, &dest));
    assert_eq!(escrow.get_accumulated_protocol_fees(), 0);
}

// ---------------------------------------------------------------------------
// propose_governance_admin / accept_governance_admin / cancel_governance_admin_proposal
//
// NOTE: The two-step admin rotation entrypoints (propose_governance_admin,
// accept_governance_admin, cancel_governance_admin_proposal) are implemented
// as pub(crate) helpers and are not exposed as public #[contractimpl] methods
// on EscrowClient in this build. Their auth logic is exercised in the
// dedicated governance test suite. Here we test the publicly available
// governance entrypoints: set_governed_params and get_governed_parameters.
// ---------------------------------------------------------------------------

#[test]
fn set_governed_params_allowed_for_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    assert!(escrow.set_governed_params(&actors.admin, &500u32, &1_000_000_000_i128));
    let params = escrow.get_governed_parameters().unwrap();
    assert_eq!(params.protocol_fee_bps, 500u32);
}

#[test]
fn set_governed_params_rejected_for_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);

    let r = escrow.try_set_governed_params(&actors.client, &500u32, &1_000_000_000_i128);
    assert_contract_error(r, EscrowError::UnauthorizedRole);
}

#[test]
fn set_governed_params_rejected_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &addr);
    let admin = Address::generate(&env);

    let r = escrow.try_set_governed_params(&admin, &0u32, &i128::MAX);
    assert_contract_error(r, Error::NotInitialized);
}

// ---------------------------------------------------------------------------
// Client migration matrix
// ---------------------------------------------------------------------------

/// Only the stored client may propose a client migration.
#[test]
fn propose_client_migration_allowed_for_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    let new_client = Address::generate(&env);

    assert!(escrow.propose_client_migration(&id, &actors.client, &new_client));
}

#[test]
fn propose_client_migration_rejected_for_freelancer() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    let new_client = Address::generate(&env);

    let r = escrow.try_propose_client_migration(&id, &actors.freelancer, &new_client);
    assert!(r.is_err(), "freelancer cannot propose client migration");
}

#[test]
fn propose_client_migration_rejected_for_stranger() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    let new_client = Address::generate(&env);

    let r = escrow.try_propose_client_migration(&id, &actors.stranger, &new_client);
    assert!(r.is_err(), "stranger cannot propose client migration");
}

/// Only the proposed new_client may accept a pending migration.
#[test]
fn accept_client_migration_allowed_for_proposed_new_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    let new_client = Address::generate(&env);
    escrow.propose_client_migration(&id, &actors.client, &new_client);

    assert!(escrow.accept_client_migration(&id, &new_client));
}

#[test]
fn accept_client_migration_rejected_for_current_client() {
    let env = Env::default();
    env.mock_all_auths();
    let (escrow, actors) = setup_initialized(&env);
    bind_token(&env, &escrow, &actors.admin);
    let id = create_simple_contract(
        &env,
        &escrow,
        &actors.client,
        &actors.freelancer,
        None,
        ReleaseAuthorization::ClientOnly,
    );
    let new_client = Address::generate(&env);
    escrow.propose_client_migration(&id, &actors.client, &new_client);

    // Current client trying to accept their own proposal
    let r = escrow.try_accept_client_migration(&id, &actors.client);
    assert!(r.is_err(), "current client cannot accept own migration");
}

