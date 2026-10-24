extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    token::TokenClient,
    vec, IntoVal,
};

struct Setup<'a> {
    env: Env,
    escrow: EscrowClient<'a>,
    token: TokenClient<'a>,
    client: Address,
    freelancer: Address,
    admin: Address,
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
        escrow,
        token,
        client,
        freelancer,
        admin,
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
    s.escrow.resolve_dispute(&s.admin, &id, &2_500);
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
    s.escrow.set_paused(&s.admin, &true);
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
fn pauser_can_pause_but_cannot_resolve_disputes() {
    let s = setup();
    let pauser = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Pauser, &pauser);
    assert!(s.escrow.has_role(&Role::Pauser, &pauser));

    s.escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&pauser, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .set_paused(&pauser, &true);
    assert!(s.escrow.is_paused());

    s.escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&pauser, false).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .set_paused(&pauser, &false);
    assert!(!s.escrow.is_paused());

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.open_dispute(&id, &s.freelancer);

    let result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "resolve_dispute",
                args: (&pauser, id, 5_000u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_resolve_dispute(&pauser, &id, &5_000);
    assert_eq!(result, Err(Ok(EscrowError::Unauthorized)));
}

#[test]
fn resolver_can_resolve_but_cannot_pause() {
    let s = setup();
    let resolver = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Resolver, &resolver);
    assert!(s.escrow.has_role(&Role::Resolver, &resolver));

    let pause_result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &resolver,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&resolver, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_set_paused(&resolver, &true);
    assert_eq!(pause_result, Err(Ok(EscrowError::Unauthorized)));

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.open_dispute(&id, &s.freelancer);

    s.escrow
        .mock_auths(&[MockAuth {
            address: &resolver,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "resolve_dispute",
                args: (&resolver, id, 10_000u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .resolve_dispute(&resolver, &id, &10_000);
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);
    assert_eq!(s.token.balance(&s.freelancer), 100);
}

#[test]
fn revoking_pauser_role_takes_effect_immediately() {
    let s = setup();
    let pauser = Address::generate(&s.env);
    s.escrow.grant_role(&Role::Pauser, &pauser);
    s.escrow.revoke_role(&Role::Pauser, &pauser);
    assert!(!s.escrow.has_role(&Role::Pauser, &pauser));

    let result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &pauser,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "set_paused",
                args: (&pauser, true).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_set_paused(&pauser, &true);
    assert_eq!(result, Err(Ok(EscrowError::Unauthorized)));
}

#[test]
fn resolver_who_is_a_party_cannot_resolve_that_escrow() {
    let s = setup();
    s.escrow.grant_role(&Role::Resolver, &s.client);

    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.open_dispute(&id, &s.freelancer);

    let result = s
        .escrow
        .mock_auths(&[MockAuth {
            address: &s.client,
            invoke: &MockAuthInvoke {
                contract: &s.escrow.address,
                fn_name: "resolve_dispute",
                args: (&s.client, id, 5_000u32).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_resolve_dispute(&s.client, &id, &5_000);
    assert_eq!(result, Err(Ok(EscrowError::Unauthorized)));
}
