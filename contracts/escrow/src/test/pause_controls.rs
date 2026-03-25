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
#[should_panic(expected = "Pause controls already initialized")]
fn test_initialize_only_once_fails() {
    let (_env, client, admin) = setup_initialized();
    let _ = client.initialize(&admin);
}

#[test]
fn test_pause_then_unpause_toggles_state() {
    let (_env, client, _admin) = setup_initialized();

    assert!(!client.is_paused());
    assert!(client.pause());
    assert!(client.is_paused());

    assert!(client.unpause());
    assert!(!client.is_paused());
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_pause_blocks_create_contract() {
    let (env, client, _admin) = setup_initialized();
    assert!(client.pause());

    let client_addr = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, 50_i128, 75_i128];
    let _ = client.create_contract(
        &client_addr,
        &freelancer,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
}

#[test]
#[should_panic(expected = "Pause controls are not initialized")]
fn test_pause_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let _ = client.pause();
}
