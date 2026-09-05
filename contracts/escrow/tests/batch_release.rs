use escrow::{
    milestones_consts::MAX_BATCH_MILESTONES, ContractStatus, Escrow, EscrowClient,
    ReleaseAuthorization,
};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn setup_and_create_escrow<'a>(
    env: &'a Env,
    milestone_amounts: &[i128],
) -> (EscrowClient<'a>, Address, Address, Address, u32) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);

    let admin = Address::generate(env);
    client.initialize(&admin);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);

    let mut milestones = Vec::new(env);
    let mut total_amount = 0i128;
    for &amount in milestone_amounts {
        milestones.push_back(amount);
        total_amount += amount;
    }

    let c_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    // Deposit full amount
    client.deposit_funds(&c_id, &client_addr, &total_amount);

    (client, admin, client_addr, freelancer_addr, c_id)
}

#[test]
fn test_batch_release_empty_batch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    let indices: Vec<u32> = Vec::new(&env);
    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(result.is_err(), "Empty batch must be rejected");
}

#[test]
fn test_batch_release_limit_exceeded_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    let mut indices: Vec<u32> = Vec::new(&env);
    for i in 0..=MAX_BATCH_MILESTONES {
        indices.push_back(i);
    }

    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(result.is_err(), "Over-limit batch must be rejected");
}

#[test]
fn test_batch_release_duplicate_index_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(0);

    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(
        result.is_err(),
        "Duplicate indices in batch must be rejected"
    );
}

#[test]
fn test_batch_release_all_or_nothing_atomicity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    // Release milestone 0 individually first
    assert!(client.release_milestone(&c_id, &client_addr, &0, &0));

    // Try batch with [0, 1] -> index 0 is already released -> entire batch must fail
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(1);

    let result = client.try_release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(result.is_err(), "Batch containing released item must fail");

    // Verify milestone 1 remains unreleased (atomic rollback / all-or-nothing)
    let contract_milestones = client.get_milestones(&c_id);
    assert!(!contract_milestones.get(1).unwrap().released);

    let contract = client.get_contract(&c_id);
    assert_eq!(contract.released_amount, 100);
}

#[test]
fn test_batch_release_valid_batch_succeeds_and_completes_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _, c_id) = setup_and_create_escrow(&env, &[100, 200, 300]);

    // Release all 3 milestones in a single batch
    let mut indices: Vec<u32> = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(1);
    indices.push_back(2);

    let success = client.release_milestone_batch(&c_id, &client_addr, &indices);
    assert!(success);

    // Verify all milestones are marked released
    let contract_milestones = client.get_milestones(&c_id);
    assert!(contract_milestones.get(0).unwrap().released);
    assert!(contract_milestones.get(1).unwrap().released);
    assert!(contract_milestones.get(2).unwrap().released);

    // Verify contract transitioned to Completed
    let contract = client.get_contract(&c_id);
    assert_eq!(contract.status, ContractStatus::Completed);
    assert_eq!(contract.released_amount, 600);
}
