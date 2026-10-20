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
        false,
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
    assert_eq!(rep.total_weighted_rating, 1500); // 5 * 300
    assert_eq!(rep.total_released_value, 300);
    let avg = s.escrow.get_weighted_average_rating(&s.freelancer);
    assert_eq!(avg, Some(50000)); // (5 * 300 * 10000) / 300 = 50000
}

#[test]
fn rejects_overfunding_and_double_release() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128],
        false,
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
        false,
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
        false,
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
            &vec![&s.env, 1i128],
            false
        ),
        Err(Ok(EscrowError::Paused))
    );
}

#[test]
fn sequential_milestones_enforced() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128, 300i128],
        true,
    );
    s.escrow.deposit_funds(&id, &600);
    
    // Cannot release milestone 2 before milestone 1
    assert_eq!(
        s.escrow.try_release_milestone(&id, &2),
        Err(Ok(EscrowError::MilestoneOutOfOrder))
    );
    
    // Cannot release milestone 1 before milestone 0
    assert_eq!(
        s.escrow.try_release_milestone(&id, &1),
        Err(Ok(EscrowError::MilestoneOutOfOrder))
    );
    
    // Can release milestone 0
    s.escrow.release_milestone(&id, &0);
    
    // Can now release milestone 1
    s.escrow.release_milestone(&id, &1);
    
    // Cannot release milestone 0 again
    assert_eq!(
        s.escrow.try_release_milestone(&id, &0),
        Err(Ok(EscrowError::MilestoneAlreadyReleased))
    );
    
    // Can now release milestone 2
    s.escrow.release_milestone(&id, &2);
    
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn non_sequential_milestones_allows_any_order() {
    let s = setup();
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128, 200i128, 300i128],
        false,
    );
    s.escrow.deposit_funds(&id, &600);
    
    // Can release milestone 2 first when sequential_milestones is false
    s.escrow.release_milestone(&id, &2);
    s.escrow.release_milestone(&id, &0);
    s.escrow.release_milestone(&id, &1);
    
    assert_eq!(s.escrow.get_contract(&id).status, ContractStatus::Completed);
}

#[test]
fn dust_threshold_blocks_reputation_farming() {
    let s = setup();
    // Create a contract with amount below dust threshold (1000)
    let id = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 100i128], // Below DUST_THRESHOLD of 1000
        false,
    );
    s.escrow.deposit_funds(&id, &100);
    s.escrow.release_milestone(&id, &0);
    s.escrow.issue_reputation(&id, &5);
    let rep = s.escrow.get_reputation(&s.freelancer);
    // Reputation should not be counted due to dust threshold
    assert_eq!(rep.completed_contracts, 0);
    assert_eq!(rep.total_rating, 0);
    assert_eq!(rep.total_weighted_rating, 0);
    assert_eq!(rep.total_released_value, 0);
    assert_eq!(s.escrow.get_weighted_average_rating(&s.freelancer), None);
}

#[test]
fn weighted_reputation_with_multiple_contracts() {
    let s = setup();
    // First contract: 200 amount, rating 5
    let id1 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 200i128],
        false,
    );
    s.escrow.deposit_funds(&id1, &200);
    s.escrow.release_milestone(&id1, &0);
    s.escrow.issue_reputation(&id1, &5);
    
    // Second contract: 400 amount, rating 3
    let id2 = s.escrow.create_contract(
        &s.client,
        &s.freelancer,
        &s.token.address,
        &vec![&s.env, 400i128],
        false,
    );
    s.escrow.deposit_funds(&id2, &400);
    s.escrow.release_milestone(&id2, &0);
    s.escrow.issue_reputation(&id2, &3);
    
    let rep = s.escrow.get_reputation(&s.freelancer);
    assert_eq!(rep.completed_contracts, 2);
    assert_eq!(rep.total_rating, 8);
    assert_eq!(rep.total_weighted_rating, 2200); // (5*200) + (3*400)
    assert_eq!(rep.total_released_value, 600);
    
    // Weighted average: (5*200 + 3*400) / 600 = 2200/600 = 3.666...
    // Scaled by 10000: 36666
    let avg = s.escrow.get_weighted_average_rating(&s.freelancer);
    assert_eq!(avg, Some(36666));
}
