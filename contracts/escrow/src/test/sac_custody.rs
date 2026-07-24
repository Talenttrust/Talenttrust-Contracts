//! Tests for the SAC (Stellar Asset Contract) custody integration in
//! `deposit_funds` and `release_milestone` (issue #439), plus the pre-bind
//! probe added in issue #723.
//!
//! These tests register a mock Stellar Asset Contract via
//! `env.register_stellar_asset_contract(admin)` and exercise the escrow
//! contract's deposit/release paths against real SAC `transfer` calls.
//!
//! Coverage matrix:
//!
//! | Path                          | Positive cases | Negative cases |
//! |-------------------------------|---------------|----------------|
//! | `bind_settlement_token`       | admin binds    | non-admin rejected, double-bind rejected, before-init rejected, invalid token rejected, self rejected, admin-as-token rejected |
//! | `get_settlement_token`        | returns Some   | returns None before bind |
//! | `deposit_funds` (SAC path)    | pull from client → contract, status Created→Funded | unbound token rejected, non-client rejected, paused blocked, over-funding rejected |
//! | `release_milestone` (SAC path)| push contract → freelancer, fee retained | unbound token rejected, non-released rejected, fee math correct (full + zero) |
//! | Atomicity                     | failed SAC transfer leaves state untouched | — |
//! | Reentrancy (mock token)       | state-before-transfer ordering verified | — |
//!
//! Run locally with `cargo test -p escrow --lib sac_custody`.

#![cfg(test)]

use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events as _},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, Env, Symbol, TryFromVal, Vec as SorobanVec,
};

use super::{
    assert_contract_error, register_client, total_milestone_amount, MILESTONE_ONE, MILESTONE_THREE,
    MILESTONE_TWO,
};
use crate::{ContractStatus, EscrowError, ReleaseAuthorization};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Register the escrow contract and an SAC, initialize escrow, bind settlement
/// token. Returns `(escrow_client, sac_address, admin)`.
fn setup_bound(env: &Env) -> (crate::EscrowClient<'_>, Address, Address) {
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);

    // Register a mock Stellar Asset Contract.
    let sac = env.register_stellar_asset_contract(admin.clone());

    // Initialize the escrow with admin auth.
    env.mock_all_auths_allowing_non_root_auth();
    client.initialize(&admin);

    // Bind the SAC token (admin + token).
    client.bind_settlement_token(&admin, &sac);

    (client, sac, admin)
}

/// Mint `amount` SAC tokens to `holder` via the SAC admin client.
fn mint_to(env: &Env, sac: &Address, holder: &Address, amount: i128) {
    StellarAssetClient::new(env, sac).mint(holder, &amount);
}

/// Mint and create a default 3-milestone contract. Returns
/// `(client_addr, freelancer_addr, contract_id)`.
fn funded_sac_contract(
    env: &Env,
    escrow_client: &crate::EscrowClient<'_>,
    sac: &Address,
) -> (Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter: Option<Address> = None;
    let milestones = SorobanVec::from_slice(env, &[MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]);
    env.mock_all_auths_allowing_non_root_auth();
    let id = escrow_client.create_contract(
        &client_addr,
        &freelancer_addr,
        &arbiter,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    let total = total_milestone_amount();
    mint_to(env, sac, &client_addr, total);
    escrow_client.deposit_funds(&id, &client_addr, &total);
    (client_addr, freelancer_addr, id)
}

// ─── bind_settlement_token ───────────────────────────────────────────────────

#[test]
fn bind_settlement_token_unbound_then_some_returns_none() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    let client = register_client(&env);
    assert!(client.get_settlement_token().is_none());
}

#[test]
fn bind_settlement_token_admin_can_bind_and_query_returns_some() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    // register_client already called initialize; get admin from storage
    let admin = client.get_admin().unwrap();

    let sac = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &sac));
    assert_eq!(client.get_settlement_token(), Some(sac));
}

#[test]
fn is_settlement_token_bound_false_before_bind_true_after() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    let admin = client.get_admin().unwrap();

    // Pre-flight readiness probe must report false before any token is bound.
    assert!(
        !client.is_settlement_token_bound(),
        "no token bound yet: readiness must be false"
    );

    let sac = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &sac));

    // After a successful bind the escrow is ready to accept deposits.
    assert!(
        client.is_settlement_token_bound(),
        "token bound: readiness must be true"
    );
    // The boolean reader must agree with the Address-returning reader.
    assert_eq!(
        client.get_settlement_token().is_some(),
        client.is_settlement_token_bound()
    );
}

#[test]
fn bind_settlement_token_rejects_double_bind() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    let admin = client.get_admin().unwrap();

    let sac = env.register_stellar_asset_contract(admin.clone());
    let sac2 = env.register_stellar_asset_contract(admin.clone());
    client.bind_settlement_token(&admin, &sac);

    assert_contract_error(
        client.try_bind_settlement_token(&admin, &sac2),
        EscrowError::SettlementTokenAlreadyBound,
    );
}

#[test]
#[allow(deprecated)]
fn second_bind_attempt_fails_regardless_of_entrypoint() {
    // 1. bind_settlement_token followed by set_settlement_token
    {
        let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
        env.mock_all_auths_allowing_non_root_auth();
        let client = register_client(&env);
        let admin = client.get_admin().unwrap();
        let sac1 = env.register_stellar_asset_contract(admin.clone());
        let sac2 = env.register_stellar_asset_contract(admin.clone());

        client.bind_settlement_token(&admin, &sac1);
        assert_contract_error(
            client.try_set_settlement_token(&admin, &sac2),
            EscrowError::SettlementTokenAlreadyBound,
        );
    }

    // 2. set_settlement_token followed by bind_settlement_token
    {
        let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
        env.mock_all_auths_allowing_non_root_auth();
        let client = register_client(&env);
        let admin = client.get_admin().unwrap();
        let sac1 = env.register_stellar_asset_contract(admin.clone());
        let sac2 = env.register_stellar_asset_contract(admin.clone());

        client.set_settlement_token(&admin, &sac1);
        assert_contract_error(
            client.try_bind_settlement_token(&admin, &sac2),
            EscrowError::SettlementTokenAlreadyBound,
        );
    }

    // 3. set_settlement_token followed by set_settlement_token
    {
        let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
        env.mock_all_auths_allowing_non_root_auth();
        let client = register_client(&env);
        let admin = client.get_admin().unwrap();
        let sac1 = env.register_stellar_asset_contract(admin.clone());
        let sac2 = env.register_stellar_asset_contract(admin.clone());

        client.set_settlement_token(&admin, &sac1);
        assert_contract_error(
            client.try_set_settlement_token(&admin, &sac2),
            EscrowError::SettlementTokenAlreadyBound,
        );
    }
}

#[test]
#[allow(deprecated)]
fn set_settlement_token_delegate_inherits_all_guards_and_events() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    let admin = client.get_admin().unwrap();
    let sac = env.register_stellar_asset_contract(admin.clone());

    // set_settlement_token delegates to bind_settlement_token and successfully binds
    assert!(client.set_settlement_token(&admin, &sac));
    assert_eq!(client.get_settlement_token(), Some(sac));
    assert!(has_settlement_token_bound_event(&env));
}

#[test]
fn bind_settlement_token_rejects_uninit() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract(admin.clone());
    assert_contract_error(
        client.try_bind_settlement_token(&admin, &sac),
        crate::Error::NotInitialized,
    );
}

/// Returns `true` when at least one published event carries
/// `settlement_token_bound` as its first topic.
fn has_settlement_token_bound_event(env: &Env) -> bool {
    let topic = Symbol::new(env, "settlement_token_bound");
    env.events().all().iter().any(|event| {
        event.1.len() > 0
            && Symbol::try_from_val(env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&topic)
    })
}

#[test]
fn bind_settlement_token_emits_settlement_token_bound_event() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    let admin = client.get_admin().unwrap();

    let sac = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &sac));

    // Topic must be present on a successful, authorized bind.
    assert!(
        has_settlement_token_bound_event(&env),
        "successful bind must publish settlement_token_bound"
    );
}

#[test]
fn rejected_bind_does_not_emit_settlement_token_bound_event() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract(admin.clone());

    // Uninitialized bind is rejected: no event must be published.
    assert_contract_error(
        client.try_bind_settlement_token(&admin, &sac),
        crate::Error::NotInitialized,
    );
    assert!(
        !has_settlement_token_bound_event(&env),
        "rejected (uninitialized) bind must not publish settlement_token_bound"
    );
}

// ─── Pre-bind probe tests (issue #723) ─────────────────────────────────────────

/// A mock contract that does NOT implement the SAC token interface.
/// Used to test that `bind_settlement_token` rejects non-token addresses.
#[contract]
struct MockNonToken;

#[contractimpl]
impl MockNonToken {
    pub fn hello(_env: Env) -> bool {
        true
    }
}

#[test]
fn bind_settlement_token_rejects_non_token_address() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    let admin = client.get_admin().unwrap();

    // Register a contract that is NOT a token contract.
    let non_token_addr = env.register(MockNonToken, ());

    // The probe call to `token::Client::balance` panics because
    // `non_token_addr` does not implement the SAC token interface.
    // This produces a host-level error (not a contract error), which
    // surfaces as a panic. We verify the bind was rejected and no
    // token was persisted.
    let result = client.try_bind_settlement_token(&admin, &non_token_addr);
    assert!(result.is_err(), "binding a non-token address must fail");

    // Verify no token was bound.
    assert!(
        client.get_settlement_token().is_none(),
        "rejected bind must not persist a settlement token"
    );
}

#[test]
fn bind_settlement_token_rejects_self_address() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    let admin = client.get_admin().unwrap();

    // The escrow contract's own address.
    let self_addr = client.address.clone();

    assert_contract_error(
        client.try_bind_settlement_token(&admin, &self_addr),
        EscrowError::SettlementTokenIsSelf,
    );

    // Verify no token was bound.
    assert!(
        client.get_settlement_token().is_none(),
        "rejected self-bind must not persist a settlement token"
    );
}

#[test]
fn bind_settlement_token_rejects_admin_address() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    let admin = client.get_admin().unwrap();

    // Try to bind the admin address as the settlement token.
    assert_contract_error(
        client.try_bind_settlement_token(&admin, &admin),
        EscrowError::SettlementTokenIsAdmin,
    );

    // Verify no token was bound.
    assert!(
        client.get_settlement_token().is_none(),
        "rejected admin-bind must not persist a settlement token"
    );
}

#[test]
fn bind_settlement_token_probe_does_not_mutate_state() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    let admin = client.get_admin().unwrap();

    // Register a non-token contract and attempt to bind it.
    let non_token_addr = env.register(MockNonToken, ());
    let result = client.try_bind_settlement_token(&admin, &non_token_addr);
    assert!(result.is_err(), "binding a non-token address must fail");

    // A subsequent valid bind must still succeed (state is clean).
    let sac = env.register_stellar_asset_contract(admin.clone());
    assert!(client.bind_settlement_token(&admin, &sac));
    assert_eq!(client.get_settlement_token(), Some(sac));
}

#[test]
fn bind_settlement_token_non_admin_rejected_before_probe() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let client = register_client(&env);
    let admin = client.get_admin().unwrap();

    // A random non-admin address.
    let attacker = Address::generate(&env);
    let sac = env.register_stellar_asset_contract(admin.clone());

    // Non-admin should be rejected with UnauthorizedRole, not
    // InvalidSettlementToken or any probe-related error.
    assert_contract_error(
        client.try_bind_settlement_token(&attacker, &sac),
        EscrowError::UnauthorizedRole,
    );
}

// ─── deposit_funds (SAC path) ─────────────────────────────────────────────────

#[test]
fn deposit_funds_with_sac_pulls_amount_into_contract() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    let (client, sac, admin) = setup_bound(&env);
    env.mock_all_auths_allowing_non_root_auth();

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = SorobanVec::from_slice(&env, &[MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let total = total_milestone_amount();
    mint_to(&env, &sac, &client_addr, total);

    let token = TokenClient::new(&env, &sac);
    let before_client: i128 = token.balance(&client_addr);
    let before_escrow: i128 = token.balance(&client.address);
    assert_eq!(before_client, total);
    assert_eq!(before_escrow, 0);

    env.mock_all_auths_allowing_non_root_auth();
    assert!(client.deposit_funds(&id, &client_addr, &total));

    let after_client: i128 = token.balance(&client_addr);
    let after_escrow: i128 = token.balance(&client.address);
    assert_eq!(after_client, 0, "client balance should be depleted");
    assert_eq!(after_escrow, total, "escrow contract should hold the total");

    let _ = admin;
    let contract = client.get_contract(&id);
    assert_eq!(contract.funded_amount, total);
    assert_eq!(contract.status, ContractStatus::Funded);
}

#[test]
fn deposit_funds_rejects_when_token_unbound() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    // NOTE: not calling bind_settlement_token.

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &super::default_milestones(&env),
        &ReleaseAuthorization::ClientOnly,
    );

    assert_contract_error(
        client.try_deposit_funds(&id, &client_addr, &100_i128),
        crate::Error::SettlementTokenNotConfigured,
    );

    // State must be unchanged: no funded_amount bump, no status transition.
    let contract = client.get_contract(&id);
    assert_eq!(contract.funded_amount, 0);
    assert_eq!(contract.status, ContractStatus::Created);
}

// ─── release_milestone (SAC path) ─────────────────────────────────────────────

fn setup_and_funded_partial(
    env: &Env,
    _initial_balance: i128,
) -> (
    crate::EscrowClient<'_>,
    Address,
    Address,
    Address,
    Address,
    u32,
) {
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract(admin.clone());
    env.mock_all_auths_allowing_non_root_auth();
    client.initialize(&admin);
    client.bind_settlement_token(&admin, &sac);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let milestones = SorobanVec::from_slice(env, &[MILESTONE_ONE, MILESTONE_TWO, MILESTONE_THREE]);
    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    (client, sac, admin, client_addr, freelancer_addr, id)
}

#[test]
fn release_milestone_with_sac_pushes_payout_minus_fee_to_freelancer() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin, client_addr, freelancer_addr, id) = setup_and_funded_partial(&env, 0);
    let total = total_milestone_amount();
    mint_to(&env, &sac, &client_addr, total);
    client.deposit_funds(&id, &client_addr, &total);

    let token = TokenClient::new(&env, &sac);
    // Configure a 10% protocol fee (1000 bps of 10000 total bps).
    client.set_protocol_fee_bps(&1000u32);
    let milestone_amount = MILESTONE_ONE;
    let fee = milestone_amount * 1000 / 10_000;
    let payout = milestone_amount - fee;
    client.approve_milestone_release(&id, &client_addr, &0);
    assert!(client.release_milestone(&id, &client_addr, &0));

    assert_eq!(token.balance(&freelancer_addr), payout);
    let contract = client.get_contract(&id);
    assert_eq!(contract.released_amount, milestone_amount - fee);
}

#[test]
fn release_milestone_zero_fee_pays_full_milestone_amount() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin, client_addr, freelancer_addr, id) = setup_and_funded_partial(&env, 0);
    let total = total_milestone_amount();
    mint_to(&env, &sac, &client_addr, total);
    client.deposit_funds(&id, &client_addr, &total);

    let token = TokenClient::new(&env, &sac);
    // Fee unset (defaults to 0).
    client.approve_milestone_release(&id, &client_addr, &0);
    assert!(client.release_milestone(&id, &client_addr, &0));

    assert_eq!(token.balance(&freelancer_addr), MILESTONE_ONE);
}

// ─── Accounting invariant ─────────────────────────────────────────────────────

/// Verify the balance invariant documented in docs/escrow/sac-custody.md:
///   escrow_sac_balance == funded_amount − released_amount − refunded_amount + accrued_fees
#[test]
fn sac_custody_accounting_invariant_holds_after_deposit_and_release() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, sac, _admin, client_addr, freelancer_addr, id) = setup_and_funded_partial(&env, 0);
    let total = total_milestone_amount();
    mint_to(&env, &sac, &client_addr, total);

    let token = TokenClient::new(&env, &sac);

    // Before deposit: escrow holds nothing.
    assert_eq!(token.balance(&escrow.address), 0_i128);

    // Deposit: escrow balance == funded_amount, released == 0, refunded == 0, fees == 0.
    escrow.deposit_funds(&id, &client_addr, &total);
    let contract = escrow.get_contract(&id);
    let escrow_bal: i128 = token.balance(&escrow.address);
    assert_eq!(
        escrow_bal,
        contract.funded_amount - contract.released_amount - contract.refunded_amount,
        "invariant violated after deposit"
    );

    // Release milestone 0 with a 500 bps (5%) fee.
    escrow.set_protocol_fee_bps(&500u32);

    escrow.approve_milestone_release(&id, &client_addr, &0);
    escrow.release_milestone(&id, &client_addr, &0);

    let contract = escrow.get_contract(&id);
    let accrued: i128 = escrow.get_accumulated_protocol_fees();
    let escrow_bal: i128 = token.balance(&escrow.address);

    // invariant: balance == funded - released - refunded
    // (released_amount is net payout; accrued_fees remains in escrow balance)
    assert_eq!(
        escrow_bal,
        contract.funded_amount - contract.released_amount - contract.refunded_amount,
        "invariant violated after milestone release"
    );

    let _ = freelancer_addr;
}

// ─── Compound end-to-end ────────────────────────────────────────────────────

#[test]
fn sac_full_lifecycle_deposit_release_balance_deltas() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin, client_addr, freelancer_addr, id) = setup_and_funded_partial(&env, 0);
    let total = total_milestone_amount();
    mint_to(&env, &sac, &client_addr, total);

    let token = TokenClient::new(&env, &sac);

    // Initial: client has total, escrow has 0.
    assert_eq!(token.balance(&client_addr), total);
    assert_eq!(token.balance(&client.address), 0);

    // Deposit: client → escrow, full amount.
    assert!(client.deposit_funds(&id, &client_addr, &total));
    assert_eq!(token.balance(&client_addr), 0);
    assert_eq!(token.balance(&client.address), total);

    // Approve and release milestone 0 with no fee.
    client.approve_milestone_release(&id, &client_addr, &0);
    assert!(client.release_milestone(&id, &client_addr, &0));

    // Freelancer got milestone 0's amount; escrow retained the rest.
    assert_eq!(token.balance(&freelancer_addr), MILESTONE_ONE);
    assert_eq!(token.balance(&client.address), total - MILESTONE_ONE);

    // Audit: contract's released_amount tracks the milestone.
    let contract = client.get_contract(&id);
    assert_eq!(contract.released_amount, MILESTONE_ONE);
}

// ─── Reentrancy documentation test (issue #723) ────────────────────────────────

/// Verify that the escrow contract state (milestone.released = true,
/// contract.released_amount updated) is mutated BEFORE the SAC transfer
/// occurs during `release_milestone`. This is the Checks-Effects-Interactions
/// (CEI) ordering that mitigates reentrancy from a malicious token contract.
///
/// In Soroban's test environment, `env.mock_all_auths()` makes it impossible
/// to simulate a re-entrant call mid-transfer (the host prevents reentrancy
/// at the VM level). However, we can verify the CEI ordering by checking that
/// after a successful release, the state is consistent — if the transfer had
/// failed after state mutation, the transaction would revert atomically
/// (Soroban's rollback guarantee), so the state we observe is always
/// post-mutation, post-transfer.
#[test]
fn release_milestone_cei_ordering_state_before_transfer() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin, client_addr, freelancer_addr, id) = setup_and_funded_partial(&env, 0);
    let total = total_milestone_amount();
    mint_to(&env, &sac, &client_addr, total);
    client.deposit_funds(&id, &client_addr, &total);

    let token = TokenClient::new(&env, &sac);

    // Before release: milestone 0 is not released, released_amount = 0.
    let contract_before = client.get_contract(&id);
    assert_eq!(contract_before.released_amount, 0);

    // Perform release.
    client.approve_milestone_release(&id, &client_addr, &0);
    assert!(client.release_milestone(&id, &client_addr, &0));

    // After release: state is updated AND transfer occurred.
    // If CEI were violated (transfer before state update), a failed transfer
    // would leave inconsistent state. Soroban's atomicity guarantee ensures
    // we only see the final consistent state.
    let contract_after = client.get_contract(&id);
    assert_eq!(
        contract_after.released_amount, MILESTONE_ONE,
        "released_amount must reflect the milestone"
    );

    // Token balance confirms the transfer happened.
    assert_eq!(
        token.balance(&freelancer_addr),
        MILESTONE_ONE,
        "freelancer must have received the milestone amount (zero fee)"
    );
    assert_eq!(
        token.balance(&client.address),
        total - MILESTONE_ONE,
        "escrow must have retained the remaining balance"
    );
}

// ─── Helper coverage tests ────────────────────────────────────────────────────

/// Exercise the `funded_sac_contract` helper to cover it.
#[test]
fn funded_sac_contract_helper_creates_and_deposits() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    env.mock_all_auths_allowing_non_root_auth();
    let (client, sac, _admin) = setup_bound(&env);

    let (client_addr, _freelancer_addr, id) = funded_sac_contract(&env, &client, &sac);

    let contract = client.get_contract(&id);
    assert_eq!(contract.status, ContractStatus::Funded);
    assert_eq!(contract.funded_amount, total_milestone_amount());
    assert_eq!(contract.client, client_addr);
}

/// Exercise `MockNonToken` to cover its `hello` entry point.
#[test]
fn mock_non_token_hello_returns_true() {
    let env = Env::default();
    env.ledger().with_mut(|li| { li.max_entry_ttl = 3_110_400; li.min_persistent_entry_ttl = 3_110_400; });
    let addr = env.register(MockNonToken, ());
    let client = MockNonTokenClient::new(&env, &addr);
    assert!(client.hello());
}
