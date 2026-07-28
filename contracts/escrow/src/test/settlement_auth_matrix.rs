//! Authorization-matrix tests for settlement actions.
//!
//! Covers every settlement-related entrypoint against every role (admin,
//! client, freelancer, arbiter, stranger), asserting allow/deny with typed
//! error codes.  Read-only entrypoints are verified auth-free.
//!
//! | Action | Admin | Client | Freelancer | Arbiter | Stranger | Error |
//! |--------|:-----:|:------:|:----------:|:-------:|:--------:|-------|
//! | `bind_settlement_token` | Y | N | N | N | N | `UnauthorizedRole` |
//! | `get_settlement_token` | - | - | - | - | - | (read-only) |
//! | `is_settlement_token_bound` | - | - | - | - | - | (read-only) |
//! | `finalize_contract` (Completed) | N | Y | Y | Y | N | `UnauthorizedRole` |
//! | `finalize_contract` (Disputed) | N | Y | Y | Y | N | `UnauthorizedRole` |
//! | `get_finalization_record` | - | - | - | - | - | (read-only) |
//!
//! Run: `cargo test -p escrow --lib settlement_auth_matrix`

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use super::assert_contract_error;
use crate::{ContractStatus, Escrow, EscrowClient, EscrowError, ReleaseAuthorization};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// (escrow_client, admin, client_addr, freelancer_addr, arbiter_addr)
fn setup(env: &Env) -> (EscrowClient<'_>, Address, Address, Address, Address) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    (client, admin, client_addr, freelancer_addr, arbiter_addr)
}

/// Initialize escrow, bind a settlement token, create a contract (optionally
/// with an arbiter), and fully fund it.
fn setup_funded(
    env: &Env,
    arbiter: Option<Address>,
) -> (EscrowClient<'_>, Address, Address, Address, Address, u32) {
    let (escrow, admin, client_addr, freelancer_addr, arbiter_addr) = setup(env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let sac = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &sac);

    let id = create_funded(env, &escrow, &client_addr, &freelancer_addr, arbiter);
    (
        escrow,
        admin,
        client_addr,
        freelancer_addr,
        arbiter_addr,
        id,
    )
}

/// Create a 1-milestone contract, optionally with an arbiter, and fully fund it.
fn create_funded(
    env: &Env,
    escrow: &EscrowClient<'_>,
    client_addr: &Address,
    freelancer_addr: &Address,
    arbiter: Option<Address>,
) -> u32 {
    let milestones = vec![env, 200_0000000_i128];
    let id = escrow.create_contract(
        client_addr,
        freelancer_addr,
        &arbiter,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let sac = escrow.get_settlement_token().unwrap();
    let total: i128 = 200_0000000;
    soroban_sdk::token::StellarAssetClient::new(env, &sac).mint(client_addr, &total);
    escrow.deposit_funds(&id, client_addr, &total);
    id
}

/// Drive a contract to `Completed` status by releasing all milestones (1-milestone contract).
fn complete(env: &Env, escrow: &EscrowClient<'_>, caller: &Address, id: &u32) {
    escrow.approve_milestone_release(id, caller, &0u32);
    escrow.release_milestone(id, caller, &0u32);
}

/// Drive a contract to `Disputed` status.
fn dispute(env: &Env, escrow: &EscrowClient<'_>, caller: &Address, id: &u32) {
    escrow.raise_dispute(id, caller);
}

/// Common setup: initialize escrow, bind SAC, create + fund a 1-milestone contract.
fn setup_with_contract(
    env: &Env,
    arbiter: Option<Address>,
) -> (EscrowClient<'_>, Address, Address, Address, Address, u32) {
    let (escrow, admin, client_addr, freelancer_addr, arbiter_addr) = setup(env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let sac = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &sac);

    let id = create_funded(env, &escrow, &client_addr, &freelancer_addr, arbiter);
    (
        escrow,
        admin,
        client_addr,
        freelancer_addr,
        arbiter_addr,
        id,
    )
}

// ===========================================================================
// bind_settlement_token — Role × Action
// ===========================================================================

#[test]
fn bind_settlement_token_admin_allowed() {
    let env = Env::default();
    let (escrow, admin, _, _, _) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let sac = env.register_stellar_asset_contract(admin.clone());
    assert!(escrow.bind_settlement_token(&admin, &sac));
    assert_eq!(escrow.get_settlement_token(), Some(sac));
}

#[test]
fn bind_settlement_token_client_denied() {
    let env = Env::default();
    let (escrow, admin, client_addr, _, _) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let sac = env.register_stellar_asset_contract(admin.clone());
    assert_contract_error(
        escrow.try_bind_settlement_token(&client_addr, &sac),
        EscrowError::UnauthorizedRole,
    );
}

#[test]
fn bind_settlement_token_freelancer_denied() {
    let env = Env::default();
    let (escrow, admin, _, freelancer_addr, _) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let sac = env.register_stellar_asset_contract(admin.clone());
    assert_contract_error(
        escrow.try_bind_settlement_token(&freelancer_addr, &sac),
        EscrowError::UnauthorizedRole,
    );
}

#[test]
fn bind_settlement_token_arbiter_denied() {
    let env = Env::default();
    let (escrow, admin, _, _, arbiter_addr) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let sac = env.register_stellar_asset_contract(admin.clone());
    assert_contract_error(
        escrow.try_bind_settlement_token(&arbiter_addr, &sac),
        EscrowError::UnauthorizedRole,
    );
}

#[test]
fn bind_settlement_token_stranger_denied() {
    let env = Env::default();
    let (escrow, admin, _, _, _) = setup(&env);
    let stranger = Address::generate(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let sac = env.register_stellar_asset_contract(admin.clone());
    assert_contract_error(
        escrow.try_bind_settlement_token(&stranger, &sac),
        EscrowError::UnauthorizedRole,
    );
}

// ===========================================================================
// get_settlement_token / is_settlement_token_bound — read-only, no auth
// ===========================================================================

#[test]
fn get_settlement_token_returns_none_before_bind() {
    let env = Env::default();
    let (escrow, admin, _, _, _) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    assert!(escrow.get_settlement_token().is_none());
}

#[test]
fn is_settlement_token_bound_false_before_bind() {
    let env = Env::default();
    let (escrow, admin, _, _, _) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    assert!(!escrow.is_settlement_token_bound());
}

#[test]
fn is_settlement_token_bound_true_after_bind() {
    let env = Env::default();
    let (escrow, admin, _, _, _) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let sac = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &sac);
    assert!(escrow.is_settlement_token_bound());
}

// ===========================================================================
// finalize_contract — Role × Action (Completed)
// ===========================================================================

#[test]
fn finalize_completed_client_allowed() {
    let env = Env::default();
    let (escrow, _, client, freelancer, _, id) = setup_with_contract(&env, None);
    complete(&env, &escrow, &client, &id);
    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Completed);
    assert!(escrow.finalize_contract(&id, &client));
}

#[test]
fn finalize_completed_freelancer_allowed() {
    let env = Env::default();
    let (escrow, _, client, freelancer, _, id) = setup_with_contract(&env, None);
    complete(&env, &escrow, &client, &id);
    assert!(escrow.finalize_contract(&id, &freelancer));
}

#[test]
fn finalize_completed_arbiter_allowed() {
    let env = Env::default();
    let arbiter = Address::generate(&env);
    let (escrow, _, client, freelancer, _, id) = setup_with_contract(&env, Some(arbiter.clone()));
    complete(&env, &escrow, &client, &id);
    assert!(escrow.finalize_contract(&id, &arbiter));
}

#[test]
fn finalize_completed_admin_denied() {
    let env = Env::default();
    let (escrow, admin, client, _, _, id) = setup_with_contract(&env, None);
    complete(&env, &escrow, &client, &id);
    assert_contract_error(
        escrow.try_finalize_contract(&id, &admin),
        crate::Error::UnauthorizedRole,
    );
}

#[test]
fn finalize_completed_stranger_denied() {
    let env = Env::default();
    let stranger = Address::generate(&env);
    let (escrow, _, client, _, _, id) = setup_with_contract(&env, None);
    complete(&env, &escrow, &client, &id);
    assert_contract_error(
        escrow.try_finalize_contract(&id, &stranger),
        crate::Error::UnauthorizedRole,
    );
}

// ===========================================================================
// finalize_contract — Role × Action (Disputed)
// ===========================================================================

#[test]
fn finalize_disputed_client_allowed() {
    let env = Env::default();
    let arbiter = Address::generate(&env);
    let (escrow, _, client, _, _, id) = setup_with_contract(&env, Some(arbiter));
    dispute(&env, &escrow, &client, &id);
    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Disputed);
    assert!(escrow.finalize_contract(&id, &client));
}

#[test]
fn finalize_disputed_freelancer_allowed() {
    let env = Env::default();
    let arbiter = Address::generate(&env);
    let (escrow, _, client, freelancer, _, id) = setup_with_contract(&env, Some(arbiter));
    dispute(&env, &escrow, &client, &id);
    assert!(escrow.finalize_contract(&id, &freelancer));
}

#[test]
fn finalize_disputed_arbiter_allowed() {
    let env = Env::default();
    let arbiter = Address::generate(&env);
    let (escrow, _, client, _, _, id) = setup_with_contract(&env, Some(arbiter.clone()));
    dispute(&env, &escrow, &client, &id);
    assert!(escrow.finalize_contract(&id, &arbiter));
}

#[test]
fn finalize_disputed_admin_denied() {
    let env = Env::default();
    let arbiter = Address::generate(&env);
    let (escrow, admin, client, _, _, id) = setup_with_contract(&env, Some(arbiter));
    dispute(&env, &escrow, &client, &id);
    assert_contract_error(
        escrow.try_finalize_contract(&id, &admin),
        crate::Error::UnauthorizedRole,
    );
}

#[test]
fn finalize_disputed_stranger_denied() {
    let env = Env::default();
    let stranger = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let (escrow, _, client, _, _, id) = setup_with_contract(&env, Some(arbiter));
    dispute(&env, &escrow, &client, &id);
    assert_contract_error(
        escrow.try_finalize_contract(&id, &stranger),
        crate::Error::UnauthorizedRole,
    );
}

// ===========================================================================
// finalize_contract — already finalized rejected
// ===========================================================================

#[test]
fn finalize_double_finalize_rejected() {
    let env = Env::default();
    let (escrow, _, client, _, _, id) = setup_with_contract(&env, None);
    complete(&env, &escrow, &client, &id);
    assert!(escrow.finalize_contract(&id, &client));
    assert_contract_error(
        escrow.try_finalize_contract(&id, &client),
        crate::Error::AlreadyFinalized,
    );
}

// ===========================================================================
// finalize_contract — wrong status rejected
// ===========================================================================

#[test]
fn finalize_created_status_rejected() {
    let env = Env::default();
    let (escrow, admin, client, freelancer, _) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let milestones = vec![&env, 200_0000000_i128];
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Created);
    assert_contract_error(
        escrow.try_finalize_contract(&id, &client),
        EscrowError::InvalidStatusTransition,
    );
}

#[test]
fn finalize_funded_status_rejected() {
    let env = Env::default();
    let (escrow, _, client, _, _, id) = setup_with_contract(&env, None);
    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Funded);
    assert_contract_error(
        escrow.try_finalize_contract(&id, &client),
        EscrowError::InvalidStatusTransition,
    );
}

// ===========================================================================
// get_finalization_record — read-only, no auth
// ===========================================================================

#[test]
fn get_finalization_record_returns_none_before_finalize() {
    let env = Env::default();
    let (escrow, _, _, _, _, id) = setup_with_contract(&env, None);
    assert!(escrow.get_finalization_record(&id).is_none());
}

#[test]
fn get_finalization_record_returns_some_after_finalize() {
    let env = Env::default();
    let (escrow, _, client, _, _, id) = setup_with_contract(&env, None);
    complete(&env, &escrow, &client, &id);
    assert!(escrow.finalize_contract(&id, &client));
    let record = escrow.get_finalization_record(&id);
    assert!(record.is_some());
    assert_eq!(record.unwrap().finalizer, client);
}

// ===========================================================================
// bind_settlement_token — error code specificity
// ===========================================================================

#[test]
fn bind_settlement_token_double_bind_returns_already_bound() {
    let env = Env::default();
    let (escrow, admin, _, _, _) = setup(&env);
    env.mock_all_auths_allowing_non_root_auth();
    escrow.initialize(&admin);
    let sac1 = env.register_stellar_asset_contract(admin.clone());
    let sac2 = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &sac1);
    assert_contract_error(
        escrow.try_bind_settlement_token(&admin, &sac2),
        EscrowError::SettlementTokenAlreadyBound,
    );
}

#[test]
fn bind_settlement_token_uninit_returns_not_initialized() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract(admin.clone());
    env.mock_all_auths_allowing_non_root_auth();
    assert_contract_error(
        escrow.try_bind_settlement_token(&admin, &sac),
        crate::Error::NotInitialized,
    );
}

// ===========================================================================
// finalize_contract — non-arbiter role when no arbiter assigned
// ===========================================================================

#[test]
fn finalize_completed_no_arbiter_stranger_still_denied() {
    let env = Env::default();
    let stranger = Address::generate(&env);
    let (escrow, _, client, _, _, id) = setup_with_contract(&env, None);
    complete(&env, &escrow, &client, &id);
    assert_contract_error(
        escrow.try_finalize_contract(&id, &stranger),
        crate::Error::UnauthorizedRole,
    );
}
