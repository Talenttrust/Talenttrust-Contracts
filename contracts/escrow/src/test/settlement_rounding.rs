#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{
    dispute::resolution_payouts, Contract, ContractStatus, DisputeResolution, DisputeSplit, Error,
    Escrow, EscrowClient, ReleaseAuthorization,
};

/// Verifies the worked numeric example from docs/settlement.md:
///
/// Milestones: [2000, 3000, 5000]  (total 10_000)
/// Fee: 500 bps (5 %)
///
/// Step 1: Release milestone 0 → fee = floor(2000 * 500 / 10_000) = 100, net = 1900
/// Step 2: Release milestone 1 → fee = floor(3000 * 500 / 10_000) = 150, net = 2850
/// Step 3: Refund milestone 2  → client receives 5000
///
/// Final: funded=10000, released=4750, refunded=5000, acc_fees=250, available=250
#[test]
fn settlement_doc_example_release_refund() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.set_protocol_fee_bps(&500u32); // 5 %

    // Bind a settlement token and mint funds for the client
    let token_admin = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract(token_admin);
    client.bind_settlement_token(&admin, &token_address);

    let token_sac = soroban_sdk::token::StellarAssetClient::new(&env, &token_address);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let milestones = vec![&env, 2000_i128, 3000_i128, 5000_i128];
    token_sac.mint(&client_addr, &10_000_i128);

    let id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Fund the full amount
    client.deposit_funds(&id, &client_addr, &10_000_i128);
    let contract = client.get_contract(&id);
    assert_eq!(contract.funded_amount, 10_000);
    assert_eq!(contract.status, ContractStatus::Funded);

    // Step 1: Release milestone 0 → fee = 100, net = 1900
    client.approve_milestone_release(&id, &client_addr, &0);
    client.release_milestone(&id, &client_addr, &0);
    assert_eq!(client.get_accumulated_protocol_fees(), 100);
    assert_eq!(client.get_contract(&id).released_amount, 1900);

    // Step 2: Release milestone 1 → fee = 150, net = 2850
    client.approve_milestone_release(&id, &client_addr, &1);
    client.release_milestone(&id, &client_addr, &1);
    assert_eq!(client.get_accumulated_protocol_fees(), 250);
    assert_eq!(client.get_contract(&id).released_amount, 4750);

    // Step 3: Refund milestone 2 → client receives 5000
    let refund_indices = vec![&env, 2_u32];
    let refunded = client.refund_unreleased_milestones(&id, &refund_indices);
    assert_eq!(refunded, 5000);

    let contract = client.get_contract(&id);
    assert_eq!(contract.funded_amount, 10_000);
    assert_eq!(contract.released_amount, 4750);
    assert_eq!(contract.refunded_amount, 5000);
    assert_eq!(contract.status, ContractStatus::Completed);

    // Available = funded - released - refunded = 10_000 - 4_750 - 5_000 = 250
    let available = contract.funded_amount - contract.released_amount - contract.refunded_amount;
    assert_eq!(available, 250);
    assert_eq!(client.get_accumulated_protocol_fees(), 250);
}

/// Verifies the dispute example from docs/settlement.md:
///
/// Funded: 10_000, released: 3_000, refunded: 1_000
/// Available: 6_000
/// PartialRefund: freelancer = floor(6_000 * 30 / 100) = 1_800, client = 4_200
#[test]
fn settlement_doc_example_dispute_partial_refund() {
    let env = Env::default();

    // Build contract state matching the doc example
    let contract = Contract {
        client: Address::generate(&env),
        freelancer: Address::generate(&env),
        arbiter: Some(Address::generate(&env)),
        status: ContractStatus::Disputed,
        total_deposited: 10_000,
        funded_amount: 10_000,
        released_amount: 3_000,
        refunded_amount: 1_000,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    };

    let resolution = DisputeResolution::PartialRefund;
    let (client_payout, freelancer_payout) =
        resolution_payouts(&contract, &resolution).expect("PartialRefund should succeed");

    assert_eq!(freelancer_payout, 1_800);
    assert_eq!(client_payout, 4_200);
    assert_eq!(client_payout + freelancer_payout, 6_000);
}

/// Verifies that FullRefund returns all available to client.
/// Status becomes Refunded only when the full funded amount has been refunded.
#[test]
fn settlement_doc_example_dispute_full_refund() {
    let env = Env::default();

    // Scenario where all available is refunded, reaching full refund of funded
    let contract = Contract {
        client: Address::generate(&env),
        freelancer: Address::generate(&env),
        arbiter: Some(Address::generate(&env)),
        status: ContractStatus::Disputed,
        total_deposited: 10_000,
        funded_amount: 10_000,
        released_amount: 0,
        refunded_amount: 4_000,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    };

    // available = 10_000 - 0 - 4_000 = 6_000
    let (client_payout, freelancer_payout) =
        resolution_payouts(&contract, &DisputeResolution::FullRefund)
            .expect("FullRefund should succeed");

    assert_eq!(freelancer_payout, 0);
    assert_eq!(client_payout, 6_000);
    assert_eq!(client_payout + freelancer_payout, 6_000);

    // After FullRefund: refunded = 4_000 + 6_000 = 10_000 == funded → Refunded
    let status = crate::dispute::final_status_after_resolution(&Contract {
        refunded_amount: contract.refunded_amount + client_payout,
        ..contract
    });
    assert_eq!(
        status,
        ContractStatus::Refunded,
        "status must be Refunded when refunded_amount == funded_amount"
    );
}

/// Verifies FullPayout and conservation: all available goes to freelancer.
#[test]
fn settlement_doc_example_dispute_full_payout() {
    let env = Env::default();

    let contract = Contract {
        client: Address::generate(&env),
        freelancer: Address::generate(&env),
        arbiter: Some(Address::generate(&env)),
        status: ContractStatus::Disputed,
        total_deposited: 10_000,
        funded_amount: 10_000,
        released_amount: 3_000,
        refunded_amount: 1_000,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    };

    let (client_payout, freelancer_payout) =
        resolution_payouts(&contract, &DisputeResolution::FullPayout)
            .expect("FullPayout should succeed");

    assert_eq!(freelancer_payout, 6_000);
    assert_eq!(client_payout, 0);
    assert_eq!(client_payout + freelancer_payout, 6_000);
}

/// Verifies Split resolution must conserve the available balance exactly.
#[test]
fn settlement_doc_example_dispute_custom_split() {
    let env = Env::default();

    let contract = Contract {
        client: Address::generate(&env),
        freelancer: Address::generate(&env),
        arbiter: Some(Address::generate(&env)),
        status: ContractStatus::Disputed,
        total_deposited: 10_000,
        funded_amount: 10_000,
        released_amount: 3_000,
        refunded_amount: 1_000,
        release_authorization: ReleaseAuthorization::ClientOnly,
        reputation_issued: false,
    };

    // Custom 50/50 split of 6_000 available
    let split = DisputeSplit {
        client_amount: 3_000,
        freelancer_amount: 3_000,
    };
    let (client_payout, freelancer_payout) =
        resolution_payouts(&contract, &DisputeResolution::Split(split))
            .expect("Split should succeed");
    assert_eq!(client_payout, 3_000);
    assert_eq!(freelancer_payout, 3_000);
    assert_eq!(client_payout + freelancer_payout, 6_000);

    // Non-conserving split rejected
    let bad_split = DisputeSplit {
        client_amount: 4_000,
        freelancer_amount: 3_000,
    };
    assert_eq!(
        resolution_payouts(&contract, &DisputeResolution::Split(bad_split)),
        Err(Error::InvalidDisputeSplit)
    );
}
