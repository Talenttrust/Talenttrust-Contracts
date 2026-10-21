extern crate std;

use super::*;
use soroban_sdk::{
    symbol_short, testutils::Address as _, token::StellarAssetClient, token::TokenClient, vec, Symbol,
};

struct Setup<'a> {
    env: Env,
    admin: Address,
    escrow: EscrowClient<'a>,
    token: TokenClient<'a>,
    client: Address,
    freelancer: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let token = TokenClient::new(&env, &sac.address());
    Setup {
        env,
        admin,
        escrow,
        token,
        client,
        freelancer,
    }
}

#[test]
fn initialize_only_once() {
    let s = setup();
    let other = Address::generate(&s.env);
    assert_eq!(
        s.escrow.try_initialize(&other),
        Err(Ok(EscrowError::AlreadyInitialized))
    );
}

#[test]
fn full_happy_path() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128],
    );
    assert_eq!(id, 1);
    s.escrow.deposit_funds(&id, &300);
    assert_eq!(s.token.balance(&s.client), 700);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Funded);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    assert_eq!(s.token.balance(&s.freelancer), 300);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
    s.escrow.issue_reputation(&id, &5);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 5);
}

#[test]
fn rejects_overfunding_and_double_release() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    assert_eq!(
        s.escrow.try_deposit_funds(&id, &101),
        Err(Ok(EscrowError::OverFunded))
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    assert!(s.escrow.try_release_milestone(&id, &0).is_err());
}

#[test]
fn dispute_split_and_cancel() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);
    s.escrow.open_dispute(&id, &s.freelancer);
    s.escrow.resolve_dispute(&id, &2_500);
    assert_eq!(s.token.balance(&s.freelancer), 100);
    assert_eq!(s.token.balance(&s.client), 900);

    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id2, &50);
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.token.balance(&s.client), 900);
    assert_eq!(
        s.escrow.get_contract(&id2).status,
        ContractStatus::Cancelled
    );
}

#[test]
fn pause_blocks_mutations() {
    let s = setup();
    s.escrow.set_paused(&true);
    assert_eq!(
        s.escrow.try_create_contract(
            &s.client,
            &s.freelancer,
            &s.token.address,
            &vec![&s.env, 1i128]
        ),
        Err(Ok(EscrowError::Paused))
    );
}

#[test]
fn test_per_escrow_freeze_isolation_and_unfreeze() {
    let s = setup();

    // Create two separate escrows
    let id_a = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128],
    );
    let id_b = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128],
    );

    // Freeze escrow A
    let reason = symbol_short!("fraud");
    s.escrow.freeze_escrow(&id_a, &reason);
    assert!(s.escrow.is_frozen(&id_a));
    assert!(!s.escrow.is_frozen(&id_b));

    // Record initial token balances
    let client_bal_before = s.token.balance(&s.client);
    let freelancer_bal_before = s.token.balance(&s.freelancer);
    let contract_bal_before = s.token.balance(&s.escrow.address);

    // All listed mutations on A must return EscrowFrozen and leave balances unchanged
    assert_eq!(
        s.escrow.try_deposit_funds(&id_a, &300),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    assert_eq!(
        s.escrow.try_release_milestone(&id_a, &0),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    assert_eq!(
        s.escrow.try_open_dispute(&id_a, &s.client),
        Err(Ok(EscrowError::EscrowFrozen))
    );
    assert_eq!(
        s.escrow.try_cancel_contract(&id_a),
        Err(Ok(EscrowError::EscrowFrozen))
    );

    // Verify token balances are unchanged
    assert_eq!(s.token.balance(&s.client), client_bal_before);
    assert_eq!(s.token.balance(&s.freelancer), freelancer_bal_before);
    assert_eq!(s.token.balance(&s.escrow.address), contract_bal_before);

    // Escrow B operations succeed normally
    assert_eq!(s.escrow.try_deposit_funds(&id_b, &300), Ok(Ok(())));
    assert_eq!(s.escrow.try_release_milestone(&id_b, &0), Ok(Ok(())));
    assert_eq!(s.token.balance(&s.freelancer), freelancer_bal_before + 100);

    // Unfreeze Escrow A and verify normal operation is restored
    s.escrow.unfreeze_escrow(&id_a);
    assert!(!s.escrow.is_frozen(&id_a));
    assert_eq!(s.escrow.try_deposit_funds(&id_a, &300), Ok(Ok(())));
}

#[test]
fn test_resolve_dispute_allowed_on_frozen_escrow() {
    let s = setup();

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
    );
    s.escrow.deposit_funds(&id, &400);

    // Open dispute before freezing
    s.escrow.open_dispute(&id, &s.freelancer);

    // Admin freezes escrow
    let reason = symbol_short!("suspect");
    s.escrow.freeze_escrow(&id, &reason);
    assert!(s.escrow.is_frozen(&id));

    // Admin resolves dispute on frozen escrow (must succeed)
    assert_eq!(s.escrow.try_resolve_dispute(&id, &2_500), Ok(Ok(())));
    assert_eq!(s.token.balance(&s.freelancer), 100);
    assert_eq!(s.token.balance(&s.client), 700);
}

#[test]
fn test_freeze_error_conditions() {
    let s = setup();

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );

    // ContractNotFound for unknown IDs
    assert_eq!(
        s.escrow.try_freeze_escrow(&999, &symbol_short!("none")),
        Err(Ok(EscrowError::ContractNotFound))
    );
    assert_eq!(
        s.escrow.try_unfreeze_escrow(&999),
        Err(Ok(EscrowError::ContractNotFound))
    );

    // Unfreezing non-frozen escrow returns InvalidStatus
    assert_eq!(
        s.escrow.try_unfreeze_escrow(&id),
        Err(Ok(EscrowError::InvalidStatus))
    );

    // Freeze valid escrow
    s.escrow.freeze_escrow(&id, &symbol_short!("reason"));

    // Freezing an already-frozen escrow returns EscrowFrozen
    assert_eq!(
        s.escrow.try_freeze_escrow(&id, &symbol_short!("again")),
        Err(Ok(EscrowError::EscrowFrozen))
    );
}

#[test]
fn test_non_admin_freeze_rejected() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);

    let contract_id = escrow.create_contract(
        &client,
        &freelancer,
        &sac.address(),
        &vec![&env, 100i128],
    );

    // Mock non-admin authorization
    env.mock_all_auths_excluding(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &id,
            fn_name: "freeze_escrow",
            args: (contract_id, symbol_short!("fraud")).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    // Attempting to freeze from a non-admin key fails due to auth check
    let res = escrow.try_freeze_escrow(&contract_id, &symbol_short!("fraud"));
    assert!(res.is_err());
}
