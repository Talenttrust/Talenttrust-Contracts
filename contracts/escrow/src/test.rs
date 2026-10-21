extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, token::TokenClient, vec};

struct Setup<'a> {
    env: Env,
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
fn dispute_outcomes_freelancer_client_and_exact_threshold() {
    let s = setup();

    // 1. Freelancer-favoured resolution (> 5_000 bps): 7_500 bps
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id1, &200);
    s.escrow.open_dispute(&id1, &s.client);
    s.escrow.resolve_dispute(&id1, &7_500);

    let client_outcomes1 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes1 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes1.disputes_opened, 1);
    assert_eq!(client_outcomes1.disputes_won, 0);
    assert_eq!(client_outcomes1.disputes_lost, 1);
    assert_eq!(freelancer_outcomes1.disputes_opened, 0);
    assert_eq!(freelancer_outcomes1.disputes_won, 1);
    assert_eq!(freelancer_outcomes1.disputes_lost, 0);

    // 2. Client-favoured resolution (< 5_000 bps): 2_000 bps
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id2, &200);
    s.escrow.open_dispute(&id2, &s.freelancer);
    s.escrow.resolve_dispute(&id2, &2_000);

    let client_outcomes2 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes2 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes2.disputes_opened, 1);
    assert_eq!(client_outcomes2.disputes_won, 1);
    assert_eq!(client_outcomes2.disputes_lost, 1);
    assert_eq!(freelancer_outcomes2.disputes_opened, 1);
    assert_eq!(freelancer_outcomes2.disputes_won, 1);
    assert_eq!(freelancer_outcomes2.disputes_lost, 1);

    // 3. Exactly 5_000 bps threshold: counts as a win for freelancer, loss for client
    let id3 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
    );
    s.escrow.deposit_funds(&id3, &200);
    s.escrow.open_dispute(&id3, &s.client);
    s.escrow.resolve_dispute(&id3, &5_000);

    let client_outcomes3 = s.escrow.get_outcomes(&s.client);
    let freelancer_outcomes3 = s.escrow.get_outcomes(&s.freelancer);
    assert_eq!(client_outcomes3.disputes_opened, 2);
    assert_eq!(client_outcomes3.disputes_won, 1);
    assert_eq!(client_outcomes3.disputes_lost, 2);
    assert_eq!(freelancer_outcomes3.disputes_opened, 1);
    assert_eq!(freelancer_outcomes3.disputes_won, 2);
    assert_eq!(freelancer_outcomes3.disputes_lost, 1);
}

#[test]
fn rate_resolved_escrow_succeeds_once_and_cancelled_rejected() {
    let s = setup();

    // Create and resolve dispute
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.open_dispute(&id, &s.client);
    s.escrow.resolve_dispute(&id, &5_000);

    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Resolved);

    // Rating a Resolved escrow succeeds once
    s.escrow.issue_reputation(&id, &4);
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 1);
    assert_eq!(rep.total_rating, 4);

    // Second rating attempt is rejected
    assert_eq!(
        s.escrow.try_issue_reputation(&id, &4),
        Err(Ok(EscrowError::ReputationAlreadyIssued))
    );

    // Cancelled escrow rating is rejected
    let id_cancel = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 50i128],
    );
    s.escrow.deposit_funds(&id_cancel, &50);
    s.escrow.cancel_contract(&id_cancel);
    assert_eq!(
        s.escrow.get_contract(&id_cancel).status,
        ContractStatus::Cancelled
    );
    assert_eq!(
        s.escrow.try_issue_reputation(&id_cancel, &5),
        Err(Ok(EscrowError::InvalidStatus))
    );
}

#[test]
fn cancel_unfunded_escrow_does_not_count_cancellation() {
    let s = setup();

    // 1. Unfunded cancellation -> cancellations remains 0
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.cancel_contract(&id1);
    assert_eq!(s.escrow.get_outcomes(&s.client).cancellations, 0);

    // 2. Funded cancellation -> cancellations increments to 1
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
    );
    s.escrow.deposit_funds(&id2, &100);
    s.escrow.cancel_contract(&id2);
    assert_eq!(s.escrow.get_outcomes(&s.client).cancellations, 1);
}

#[test]
fn sponsor_client_cancel_pro_rata() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    StellarAssetClient::new(&env, &sac.address()).mint(&sponsor, &1_000);
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &vec![&env, 100i128],
    );

    // sponsor funds 60, client funds 40
    escrow.deposit_from(&id, &sponsor, &60);
    escrow.deposit_funds(&id, &40);

    let funders = escrow.get_funders(&id);
    assert_eq!(funders.get(sponsor.clone()).unwrap(), 60);
    assert_eq!(funders.get(client.clone()).unwrap(), 40);

    let client_before = token.balance(&client);
    let sponsor_before = token.balance(&sponsor);
    let freelancer_before = token.balance(&freelancer);
    let contract_addr = escrow.address.clone();
    let contract_before = token.balance(&contract_addr);

    // client cancels with no release: refund = 100 split pro-rata
    escrow.cancel_contract(&id);

    // sponsor gets 60 back, client gets 40 back (not all to client)
    assert_eq!(token.balance(&sponsor), sponsor_before + 60);
    assert_eq!(token.balance(&client), client_before + 40);
    assert_eq!(token.balance(&freelancer), freelancer_before);
    // the contract is fully refunded, so it holds nothing left
    assert_eq!(token.balance(&contract_addr), 0);

    // conservation: sum of all balance deltas == 0
    let sum = (token.balance(&client) - client_before)
        + (token.balance(&sponsor) - sponsor_before)
        + (token.balance(&freelancer) - freelancer_before)
        + (token.balance(&contract_addr) - contract_before);
    assert_eq!(sum, 0);

    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Cancelled);
}

#[test]
fn partial_release_cancel_pro_rata() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    StellarAssetClient::new(&env, &sac.address()).mint(&sponsor, &1_000);
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let milestones = vec![&env, 30i128, 70i128];
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &milestones,
    );
    let contract_addr = escrow.address.clone();

    // sponsor funds 60, client funds 40
    escrow.deposit_from(&id, &sponsor, &60);
    escrow.deposit_funds(&id, &40);

    let client_before = token.balance(&client);
    let sponsor_before = token.balance(&sponsor);
    let freelancer_before = token.balance(&freelancer);
    let contract_before = token.balance(&contract_addr);

    // release 30 to freelancer
    escrow.release_milestone(&id, &0);
    assert_eq!(token.balance(&freelancer), freelancer_before + 30);

    // cancel: refund = 70, split pro-rata: sponsor 42, client 28
    escrow.cancel_contract(&id);

    assert_eq!(token.balance(&sponsor), sponsor_before + 42);
    assert_eq!(token.balance(&client), client_before + 28);

    // conservation: no dust lost to rounding
    let sum = (token.balance(&client) - client_before)
        + (token.balance(&sponsor) - sponsor_before)
        + (token.balance(&freelancer) - freelancer_before)
        + (token.balance(&contract_addr) - contract_before);
    assert_eq!(sum, 0);

    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Cancelled);
}

#[test]
fn dispute_split_pro_rata() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    StellarAssetClient::new(&env, &sac.address()).mint(&sponsor, &1_000);
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &vec![&env, 100i128],
    );
    let contract_addr = escrow.address.clone();

    // sponsor funds 60, client funds 40
    escrow.deposit_from(&id, &sponsor, &60);
    escrow.deposit_funds(&id, &40);

    let client_before = token.balance(&client);
    let sponsor_before = token.balance(&sponsor);
    let freelancer_before = token.balance(&freelancer);
    let contract_before = token.balance(&contract_addr);

    // freelancer opens dispute, admin resolves 25% to freelancer
    escrow.open_dispute(&id, &freelancer);
    escrow.resolve_dispute(&id, &2_500);

    // freelancer gets 25 (25% of 100 held)
    assert_eq!(token.balance(&freelancer), freelancer_before + 25);
    // client share = 75 split pro-rata: sponsor 45, client 30
    assert_eq!(token.balance(&sponsor), sponsor_before + 45);
    assert_eq!(token.balance(&client), client_before + 30);

    // conservation
    let sum = (token.balance(&client) - client_before)
        + (token.balance(&sponsor) - sponsor_before)
        + (token.balance(&freelancer) - freelancer_before)
        + (token.balance(&contract_addr) - contract_before);
    assert_eq!(sum, 0);
}

#[test]
fn too_many_funders_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &2_000);
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    escrow.initialize(&admin);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &vec![&env, 2_000i128],
    );

    // 10 funders deposit 100 each
    for _ in 0..10 {
        let funder = Address::generate(&env);
        StellarAssetClient::new(&env, &sac.address()).mint(&funder, &100);
        escrow.deposit_from(&id, &funder, &100);
    }

    // 11th funder rejected
    let funder11 = Address::generate(&env);
    StellarAssetClient::new(&env, &sac.address()).mint(&funder11, &100);
    assert_eq!(
        escrow.try_deposit_from(&id, &funder11, &100),
        Err(Ok(EscrowError::TooManyFunders))
    );
}

#[test]
fn funder_auth_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let client = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    StellarAssetClient::new(&env, &sac.address()).mint(&client, &1_000);
    StellarAssetClient::new(&env, &sac.address()).mint(&sponsor, &1_000);
    let token = TokenClient::new(&env, &sac.address());
    let id = env.register(Escrow, ());
    let escrow = EscrowClient::new(&env, &id);
    let contract_addr = escrow.address.clone();
    escrow.initialize(&admin);
    let id = escrow.create_contract(
        &client,
        &freelancer,
        &token.address,
        &vec![&env, 100i128],
    );

    // sponsor (a third party) funds the escrow and is recorded as a funder
    escrow.deposit_from(&id, &sponsor, &60);

    let funders = escrow.get_funders(&id);
    assert_eq!(funders.get(sponsor.clone()).unwrap(), 60);
    assert_eq!(funders.len(), 1);

    // the sponsor's tokens moved into the contract
    assert_eq!(token.balance(&sponsor), 1_000 - 60);
    assert_eq!(token.balance(&contract_addr), 60);
}
