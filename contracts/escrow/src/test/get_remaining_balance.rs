//! Tests for the new `get_remaining_balance` getter.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};
use crate::{Escrow, EscrowClient, ReleaseAuthorization};

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn make_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

fn participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

#[test]
fn remaining_balance_before_any_release() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &None,
        &vec![&env, 500_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &ca, 500_i128);
    // No releases yet, remaining balance should equal funded amount.
    assert_eq!(client.get_remaining_balance(&id), 500);
}

#[test]
fn remaining_balance_after_partial_release() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &None,
        &vec![&env, 300_i128, 200_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &ca, 500_i128);
    // Release first milestone (300) – protocol fee is zero in test env.
    client.release_milestone(&id, &ca, 0, &0);
    // Remaining should be 200.
    assert_eq!(client.get_remaining_balance(&id), 200);
}

#[test]
fn remaining_balance_after_full_release() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &None,
        &vec![&env, 400_i128, 100_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &ca, 500_i128);
    client.release_milestone(&id, &ca, 0, &0);
    client.release_milestone(&id, &ca, 1, &0);
    // All funds released, remaining balance should be 0.
    assert_eq!(client.get_remaining_balance(&id), 0);
}

#[test]
#[should_panic]
fn remaining_balance_over_release_panics() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &None,
        &vec![&env, 250_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &ca, 250_i128);
    // First release works (250)
    client.release_milestone(&id, &ca, 0, &0);
    // Attempt another release should panic.
    client.release_milestone(&id, &ca, 0, &0);
}

#[test]
fn remaining_balance_repeat_final_release_no_change() {
    let env = make_env();
    let client = make_client(&env);
    let (ca, fa) = participants(&env);
    let id = client.create_contract(
        &ca,
        &fa,
        &None,
        &vec![&env, 150_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    client.deposit_funds(&id, &ca, 150_i128);
    client.release_milestone(&id, &ca, 0, &0);
    // Balance is now 0.
    assert_eq!(client.get_remaining_balance(&id), 0);
    // Repeated getter should still be 0.
    assert_eq!(client.get_remaining_balance(&id), 0);
}
