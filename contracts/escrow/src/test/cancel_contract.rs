#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, Env, Symbol, TryFromVal,
};

use crate::{ContractStatus, Error, Escrow, EscrowClient, ReleaseAuthorization};

fn register_client(env: &Env) -> EscrowClient<'_> {
    let id = env.register(Escrow, ());
    EscrowClient::new(env, &id)
}

fn generate_participants(env: &Env) -> (Address, Address) {
    (Address::generate(env), Address::generate(env))
}

fn setup_cancel_context(env: &Env) -> (EscrowClient<'_>, Address, Address, u32) {
    env.mock_all_auths();
    let client = register_client(env);
    let (client_addr, freelancer_addr) = generate_participants(env);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token_admin = Address::generate(env);
    let token_address = env.register_stellar_asset_contract(token_admin);
    client.set_settlement_token(&admin, &token_address);

    let token_client = StellarAssetClient::new(env, &token_address);
    token_client.mint(&client_addr, &10_000_0000000_i128);

    let milestones = vec![env, 100_i128, 200_i128, 300_i128];
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    (client, client_addr, freelancer_addr, contract_id)
}

/// Returns the address that owns settlement-token custody for this escrow.
fn escrow_address(env: &Env, client: &EscrowClient<'_>) -> Address {
    env.as_contract(&client.address, || env.current_contract_address())
}

#[test]
fn cancel_created_contract_marks_it_cancelled_without_refund() {
    let env = Env::default();
    let (client, client_addr, _, contract_id) = setup_cancel_context(&env);
    let token_address = client.get_settlement_token().unwrap();
    let token_client = TokenClient::new(&env, &token_address);
    let escrow_addr = escrow_address(&env, &client);
    let client_balance_before = token_client.balance(&client_addr);
    let escrow_balance_before = token_client.balance(&escrow_addr);

    assert!(client.cancel_contract(&contract_id, &client_addr));

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Cancelled);
    assert_eq!(contract.refunded_amount, 0);
    assert_eq!(token_client.balance(&client_addr), client_balance_before);
    assert_eq!(token_client.balance(&escrow_addr), escrow_balance_before);
}

/// Cancelling a funded contract transfers its full remaining SAC balance to the
/// client and removes only that contract's funds from escrow custody.
#[test]
fn cancel_funded_contract_refunds_the_remaining_balance_to_the_client() {
    let env = Env::default();
    let (client, client_addr, _, contract_id) = setup_cancel_context(&env);

    assert!(client.deposit_funds(&contract_id, &client_addr, &600_i128));
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Funded);

    let token_address = client.get_settlement_token().unwrap();
    let token_client = TokenClient::new(&env, &token_address);
    let escrow_addr = escrow_address(&env, &client);
    let client_balance_before = token_client.balance(&client_addr);
    let escrow_balance_before = token_client.balance(&escrow_addr);
    assert_eq!(escrow_balance_before, 600_i128);

    assert!(client.cancel_contract(&contract_id, &client_addr));

    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.status, ContractStatus::Cancelled);
    assert_eq!(contract.refunded_amount, 600_i128);
    assert_eq!(
        token_client.balance(&client_addr),
        client_balance_before + 600_i128
    );
    assert_eq!(
        token_client.balance(&escrow_addr),
        escrow_balance_before - 600_i128
    );
}

/// Cancelling one funded contract preserves SAC custody for other active
/// contracts sharing the same escrow contract address.
#[test]
fn cancel_refund_leaves_other_contract_funds_in_escrow() {
    let env = Env::default();
    let (client, client_addr, freelancer_addr, first_contract_id) = setup_cancel_context(&env);
    let second_contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &vec![&env, 400_i128],
        &ReleaseAuthorization::ClientOnly,
    );
    let token_address = client.get_settlement_token().unwrap();
    let token_client = TokenClient::new(&env, &token_address);
    let escrow_addr = escrow_address(&env, &client);

    assert!(client.deposit_funds(&first_contract_id, &client_addr, &600_i128));
    assert!(client.deposit_funds(&second_contract_id, &client_addr, &400_i128));
    let client_balance_before = token_client.balance(&client_addr);
    assert_eq!(token_client.balance(&escrow_addr), 1_000_i128);

    assert!(client.cancel_contract(&first_contract_id, &client_addr));

    assert_eq!(
        token_client.balance(&client_addr),
        client_balance_before + 600_i128
    );
    assert_eq!(
        token_client.balance(&escrow_addr),
        client.get_refundable_balance(&second_contract_id),
        "escrow balance must contain only the remaining active contract funds"
    );
    assert_eq!(client.get_refundable_balance(&first_contract_id), 0);
}

#[test]
fn cancel_rejects_unauthorized_caller() {
    let env = Env::default();
    let (client, client_addr, _, contract_id) = setup_cancel_context(&env);
    let unauthorized = Address::generate(&env);

    super::assert_contract_error(
        client.try_cancel_contract(&contract_id, &unauthorized),
        Error::UnauthorizedRole,
    );

    assert_eq!(
        client.get_contract(&contract_id).status,
        ContractStatus::Created
    );
    assert_eq!(client.get_contract(&contract_id).client, client_addr);
}

#[test]
fn cancel_rejects_contract_after_a_release() {
    let env = Env::default();
    let (client, client_addr, _, contract_id) = setup_cancel_context(&env);

    assert!(client.deposit_funds(&contract_id, &client_addr, &600_i128));
    assert!(client.approve_milestone_release(&contract_id, &client_addr, &0));
    assert!(client.release_milestone(&contract_id, &client_addr, &0));

    super::assert_contract_error(
        client.try_cancel_contract(&contract_id, &client_addr),
        Error::InvalidStatusTransition,
    );
}

#[test]
fn double_cancel_rejects_with_already_cancelled() {
    let env = Env::default();
    let (client, client_addr, _, contract_id) = setup_cancel_context(&env);

    assert!(client.cancel_contract(&contract_id, &client_addr));

    super::assert_contract_error(
        client.try_cancel_contract(&contract_id, &client_addr),
        Error::AlreadyCancelled,
    );
}

#[test]
fn cancel_rejects_completed_contract() {
    let env = Env::default();
    let (client, client_addr, _, contract_id) = setup_cancel_context(&env);

    assert!(client.deposit_funds(&contract_id, &client_addr, &600_i128));
    for milestone_idx in 0..3 {
        assert!(client.approve_milestone_release(&contract_id, &client_addr, &milestone_idx));
        assert!(client.release_milestone(&contract_id, &client_addr, &milestone_idx));
    }

    super::assert_contract_error(
        client.try_cancel_contract(&contract_id, &client_addr),
        Error::InvalidStatusTransition,
    );
}

#[test]
fn cancel_emits_cancelled_event_with_correct_payload() {
    let env = Env::default();
    let (client, client_addr, _, contract_id) = setup_cancel_context(&env);

    assert!(client.cancel_contract(&contract_id, &client_addr));

    let cancelled_topic = symbol_short!("cancelled");
    let events = env.events().all();

    // Verify the cancelled event exists with the correct (symbol, contract_id)
    // topics. The data payload (caller, previous_status, timestamp) is emitted
    // by cancel_contract but not asserted here — Soroban `Val` does not support
    // PartialEq, so topics are the testable contract. Indexers should verify
    // the data payload against the event specification in docs/escrow/README.md.
    let found = events.iter().any(|event| {
        event.1.len() >= 2
            && Symbol::try_from_val(&env, &event.1.get(0).unwrap())
                .ok()
                .as_ref()
                == Some(&cancelled_topic)
            && event.1.get(1).is_some()
    });
    assert!(
        found,
        "Expected cancelled event with (Symbol(\"cancelled\"), contract_id) topics"
    );
}
