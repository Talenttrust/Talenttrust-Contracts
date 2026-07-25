//! # Arbiter Input Bounds Validation Tests
//!
//! Validates explicit numeric bounds on arbiter entrypoints:
//!
//! 1. **`contract_id` non-zero** — `raise_dispute`, `resolve_dispute`, and
//!    `finalize_contract` reject `contract_id == 0` with
//!    `EscrowError::InvalidContractId`.
//!
//! 2. **Split amount caps** — `resolve_dispute` rejects `DisputeResolution::Split`
//!    amounts that individually exceed `MAX_TOTAL_ESCROW_STROOPS` with
//!    `EscrowError::TotalCapExceeded`.
//!
//! 3. **Existing valid inputs preserved** — contracts with valid IDs and
//!    legitimate split amounts still succeed unchanged.

#![cfg(test)]

use crate::{
    DisputeResolution, DisputeSplit, Escrow, EscrowClient, EscrowError, ReleaseAuthorization,
    MAX_TOTAL_ESCROW_STROOPS,
};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Address, Env};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env
}

fn make_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    // Bind a settlement token so deposit_funds works.
    let token_admin = Address::generate(env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.set_settlement_token(&admin, &token_address);

    client
}

/// Create a funded contract with an arbiter, ready for dispute.
fn funded_with_arbiter(env: &Env, client: &EscrowClient<'_>) -> (Address, Address, Address, u32) {
    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);
    let arbiter_addr = Address::generate(env);
    let milestones = vec![env, 100_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Mint tokens to the client so deposit_funds succeeds.
    let token_address = client.get_settlement_token().unwrap();
    let token_client = StellarAssetClient::new(env, &token_address);
    token_client.mint(&client_addr, &100_i128);

    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

/// Create a funded and disputed contract with an arbiter.
fn disputed(env: &Env, client: &EscrowClient<'_>) -> (Address, Address, Address, u32) {
    let (client_addr, freelancer_addr, arbiter_addr, contract_id) =
        funded_with_arbiter(env, client);
    assert!(client.raise_dispute(&contract_id, &client_addr));
    (client_addr, freelancer_addr, arbiter_addr, contract_id)
}

// ─── contract_id = 0 rejection ────────────────────────────────────────────────

/// `raise_dispute` rejects `contract_id == 0` with `InvalidContractId`.
#[test]
fn raise_dispute_rejects_zero_contract_id() {
    let env = make_env();
    let client = make_client(&env);
    let caller = Address::generate(&env);

    super::assert_contract_error(
        client.try_raise_dispute(&0, &caller),
        EscrowError::InvalidContractId,
    );
}

/// `resolve_dispute` rejects `contract_id == 0` with `InvalidContractId`.
#[test]
fn resolve_dispute_rejects_zero_contract_id() {
    let env = make_env();
    let client = make_client(&env);
    let arbiter = Address::generate(&env);

    super::assert_contract_error(
        client.try_resolve_dispute(&0, &arbiter, &DisputeResolution::FullRefund),
        EscrowError::InvalidContractId,
    );
}

/// `finalize_contract` rejects `contract_id == 0` with `InvalidContractId`.
#[test]
fn finalize_contract_rejects_zero_contract_id() {
    let env = make_env();
    let client = make_client(&env);
    let finalizer = Address::generate(&env);

    super::assert_contract_error(
        client.try_finalize_contract(&0, &finalizer),
        EscrowError::InvalidContractId,
    );
}

// ─── Split amount cap ─────────────────────────────────────────────────────────

/// `resolve_dispute` rejects Split where `client_amount` exceeds
/// `MAX_TOTAL_ESCROW_STROOPS`.
#[test]
fn resolve_dispute_rejects_split_client_amount_above_total_cap() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed(&env, &client);

    let split = DisputeSplit {
        client_amount: MAX_TOTAL_ESCROW_STROOPS + 1,
        freelancer_amount: 0,
    };
    super::assert_contract_error(
        client.try_resolve_dispute(
            &contract_id,
            &arbiter_addr,
            &DisputeResolution::Split(split),
        ),
        EscrowError::TotalCapExceeded,
    );
}

/// `resolve_dispute` rejects Split where `freelancer_amount` exceeds
/// `MAX_TOTAL_ESCROW_STROOPS`.
#[test]
fn resolve_dispute_rejects_split_freelancer_amount_above_total_cap() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed(&env, &client);

    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: MAX_TOTAL_ESCROW_STROOPS + 1,
    };
    super::assert_contract_error(
        client.try_resolve_dispute(
            &contract_id,
            &arbiter_addr,
            &DisputeResolution::Split(split),
        ),
        EscrowError::TotalCapExceeded,
    );
}

/// `resolve_dispute` rejects Split where both amounts are huge
/// (`i128::MAX` far exceeds `MAX_TOTAL_ESCROW_STROOPS`).
#[test]
fn resolve_dispute_rejects_split_i128_max_amounts() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed(&env, &client);

    let split = DisputeSplit {
        client_amount: i128::MAX,
        freelancer_amount: i128::MAX,
    };
    super::assert_contract_error(
        client.try_resolve_dispute(
            &contract_id,
            &arbiter_addr,
            &DisputeResolution::Split(split),
        ),
        EscrowError::TotalCapExceeded,
    );
}

// ─── contract_id = 0 check fires before other checks ──────────────────────────

/// contract_id=0 fires `InvalidContractId` before any auth or state checks.
#[test]
fn raise_dispute_zero_id_fires_before_auth() {
    let env = make_env();
    let client = make_client(&env);
    // Pass a zero contract_id — the bounds check fires before caller auth
    // or storage lookup.
    super::assert_contract_error(
        client.try_raise_dispute(&0, &Address::generate(&env)),
        EscrowError::InvalidContractId,
    );
}

/// contract_id=0 fires `InvalidContractId` before Split amount validation.
#[test]
fn resolve_dispute_zero_id_fires_before_split_cap_check() {
    let env = make_env();
    let client = make_client(&env);
    let arbiter = Address::generate(&env);
    let split = DisputeSplit {
        client_amount: MAX_TOTAL_ESCROW_STROOPS + 1,
        freelancer_amount: 0,
    };
    // contract_id=0 check fires first, so we get InvalidContractId, not TotalCapExceeded.
    super::assert_contract_error(
        client.try_resolve_dispute(&0, &arbiter, &DisputeResolution::Split(split)),
        EscrowError::InvalidContractId,
    );
}

// ─── Existing valid inputs preserved ──────────────────────────────────────────

/// `raise_dispute` with a valid (non-zero) contract_id still succeeds.
#[test]
fn raise_dispute_with_valid_id_still_succeeds() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, _, contract_id) = funded_with_arbiter(&env, &client);

    assert!(contract_id > 0, "contract_id must be non-zero");
    assert!(client.raise_dispute(&contract_id, &client_addr));
}

/// `resolve_dispute` with a valid contract_id and non-Split resolution still succeeds.
#[test]
fn resolve_dispute_with_valid_id_full_refund_still_succeeds() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed(&env, &client);

    assert!(contract_id > 0, "contract_id must be non-zero");
    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund));
}

/// `resolve_dispute` with a valid Split where amounts are within the cap still succeeds.
#[test]
fn resolve_dispute_with_valid_split_still_succeeds() {
    let env = make_env();
    let client = make_client(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);
    let milestones = vec![&env, 50_i128, 50_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    let token_address = client.get_settlement_token().unwrap();
    let token_client = StellarAssetClient::new(&env, &token_address);
    token_client.mint(&client_addr, &100_i128);

    assert!(client.deposit_funds(&contract_id, &client_addr, &100_i128));
    assert!(client.raise_dispute(&contract_id, &client_addr));

    let split = DisputeSplit {
        client_amount: 40,
        freelancer_amount: 60,
    };
    assert!(client.resolve_dispute(
        &contract_id,
        &arbiter_addr,
        &DisputeResolution::Split(split),
    ));

    let c = client.get_contract(&contract_id);
    assert_eq!(c.refunded_amount, 40);
    assert_eq!(c.released_amount, 60);
}

/// `finalize_contract` with a valid contract_id still succeeds.
#[test]
fn finalize_contract_with_valid_id_still_succeeds() {
    let env = make_env();
    let client = make_client(&env);
    let (client_addr, _, arbiter_addr, contract_id) = disputed(&env, &client);

    // Resolve with FullPayout → Completed (valid for finalization).
    // FullRefund would result in Refunded, which is not accepted by finalize_contract.
    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullPayout));
    assert!(contract_id > 0, "contract_id must be non-zero");
    assert!(client.finalize_contract(&contract_id, &client_addr));
}

// ─── Zero amount Split (boundary) ─────────────────────────────────────────────

/// Split with both amounts at zero (valid boundary) is accepted by bounds gate
/// but fails the `available` conservation check inside `resolution_payouts`.
#[test]
fn resolve_dispute_split_zero_amounts_accepted_by_bounds() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed(&env, &client);

    // Zero amounts are within the cap. The bounds gate passes them through.
    // resolution_payouts will reject because 0 + 0 != 100 (available).
    let split = DisputeSplit {
        client_amount: 0,
        freelancer_amount: 0,
    };
    // This should NOT fail with TotalCapExceeded (bounds gate is fine).
    // It fails deeper in resolution_payouts with InvalidDisputeSplit.
    super::assert_contract_error(
        client.try_resolve_dispute(
            &contract_id,
            &arbiter_addr,
            &DisputeResolution::Split(split),
        ),
        crate::Error::InvalidDisputeSplit,
    );
}

// ─── Non-Split resolutions unaffected by cap gate ─────────────────────────────

/// Non-Split resolutions (FullRefund, PartialRefund, FullPayout) are
/// unaffected by the Split cap gate.
#[test]
fn resolve_dispute_non_split_resolutions_unaffected_by_cap() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed(&env, &client);

    // FullRefund works as before.
    assert!(client.resolve_dispute(&contract_id, &arbiter_addr, &DisputeResolution::FullRefund));
    let c = client.get_contract(&contract_id);
    assert_eq!(c.status, crate::ContractStatus::Refunded);
}

// ─── Split at exact cap boundary ──────────────────────────────────────────────

/// Split with one amount exactly at `MAX_TOTAL_ESCROW_STROOPS` and the other
/// at 0 passes the bounds gate (though it fails the `available` conservation
/// check in `resolution_payouts`).
#[test]
fn resolve_dispute_split_at_exact_cap_passes_bounds_gate() {
    let env = make_env();
    let client = make_client(&env);
    let (_, _, arbiter_addr, contract_id) = disputed(&env, &client);

    let split = DisputeSplit {
        client_amount: MAX_TOTAL_ESCROW_STROOPS,
        freelancer_amount: 0,
    };
    // Bounds gate accepts exact-cap amounts. resolution_payouts will reject
    // because the amounts don't match available balance — but that's a
    // different error, not TotalCapExceeded.
    super::assert_contract_error(
        client.try_resolve_dispute(
            &contract_id,
            &arbiter_addr,
            &DisputeResolution::Split(split),
        ),
        crate::Error::InvalidDisputeSplit,
    );
}
