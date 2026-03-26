use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, ReleaseAuthorization};

fn setup_initialized() -> (Env, EscrowClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    assert!(client.initialize(&admin));

    (env, client, admin)
}

#[test]
fn test_activate_emergency_sets_flags() {
    let (_env, client, _admin) = setup_initialized();

    assert!(!client.is_emergency());
    assert!(!client.is_paused());

    assert!(client.activate_emergency_pause());

    assert!(client.is_emergency());
    assert!(client.is_paused());
}

#[test]
#[should_panic(expected = "Emergency pause active")]
fn test_unpause_fails_while_emergency_active() {
    let (_env, client, _admin) = setup_initialized();

    assert!(client.activate_emergency_pause());
    let _ = client.unpause();
}

#[test]
fn test_resolve_emergency_restores_operations() {
    let (env, client, _admin) = setup_initialized();

    assert!(client.activate_emergency_pause());
    assert!(client.resolve_emergency());

    assert!(!client.is_emergency());
    assert!(!client.is_paused());

    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 10_i128, 20_i128];

    let created = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert_eq!(created, 0);
}
