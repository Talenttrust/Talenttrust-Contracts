extern crate std;

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, ProtocolParameters, ReleaseAuthorization};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

#[test]
fn protocol_parameters_default_before_governance_is_initialized() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let parameters = client.get_protocol_parameters();

    assert_eq!(
        parameters,
        ProtocolParameters {
            min_milestone_amount: 1,
            max_milestones: 16,
            min_reputation_rating: 1,
            max_reputation_rating: 5,
        }
    );
    assert_eq!(client.get_governance_admin(), None);
    assert_eq!(client.get_pending_governance_admin(), None);
}

#[test]
fn initialize_governance_sets_admin_and_parameters() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    assert!(client.initialize_governance(&admin));

    assert_eq!(client.get_governance_admin(), Some(admin));
    assert_eq!(client.get_pending_governance_admin(), None);
}

#[test]
#[should_panic(expected = "protocol governance is already initialized")]
fn initialize_governance_twice_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    // Second initialization should panic
    let admin2 = Address::generate(&env);
    client.initialize_governance(&admin2);
}

#[test]
fn update_protocol_parameters_changes_validation_rules() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    assert!(client.update_protocol_parameters(&100_i128, &10_u32, &1_i128, &10_i128));

    let parameters = client.get_protocol_parameters();
    assert_eq!(parameters.min_milestone_amount, 100);
    assert_eq!(parameters.max_milestones, 10);
    assert_eq!(parameters.min_reputation_rating, 1);
    assert_eq!(parameters.max_reputation_rating, 10);
}

#[test]
#[should_panic(expected = "protocol governance is not initialized")]
fn update_protocol_parameters_without_initialization_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    // Try to update without initializing governance
    client.update_protocol_parameters(&100_i128, &10_u32, &1_i128, &10_i128);
}

#[test]
#[should_panic(expected = "minimum milestone amount must be positive")]
fn update_protocol_parameters_with_zero_min_milestone_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    client.update_protocol_parameters(&0_i128, &10_u32, &1_i128, &10_i128);
}

#[test]
#[should_panic(expected = "minimum milestone amount must be positive")]
fn update_protocol_parameters_with_negative_min_milestone_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    client.update_protocol_parameters(&-100_i128, &10_u32, &1_i128, &10_i128);
}

#[test]
#[should_panic(expected = "maximum milestones must be positive")]
fn update_protocol_parameters_with_zero_max_milestones_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    client.update_protocol_parameters(&100_i128, &0_u32, &1_i128, &10_i128);
}

#[test]
#[should_panic(expected = "minimum reputation rating must be positive")]
fn update_protocol_parameters_with_zero_min_reputation_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    client.update_protocol_parameters(&100_i128, &10_u32, &0_i128, &10_i128);
}

#[test]
#[should_panic(expected = "reputation rating range is invalid")]
fn update_protocol_parameters_with_invalid_rating_range_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    // min > max should panic
    client.update_protocol_parameters(&100_i128, &10_u32, &10_i128, &5_i128);
}

#[test]
fn governance_admin_transfer_is_two_step() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let next_admin = Address::generate(&env);
    client.initialize_governance(&admin);

    assert!(client.propose_governance_admin(&next_admin));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(next_admin.clone())
    );

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(next_admin));
    assert_eq!(client.get_pending_governance_admin(), None);
}

#[test]
#[should_panic(expected = "protocol governance is not initialized")]
fn propose_governance_admin_without_initialization_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let next_admin = Address::generate(&env);
    client.propose_governance_admin(&next_admin);
}

#[test]
#[should_panic(expected = "no pending admin transfer")]
fn accept_governance_admin_without_proposal_panics() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    // Try to accept without proposal
    client.accept_governance_admin();
}

#[test]
fn propose_governance_admin_can_be_overwritten() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let next_admin1 = Address::generate(&env);
    let next_admin2 = Address::generate(&env);

    client.initialize_governance(&admin);

    // First proposal
    client.propose_governance_admin(&next_admin1);
    assert_eq!(client.get_pending_governance_admin(), Some(next_admin1));

    // Second proposal overwrites first
    client.propose_governance_admin(&next_admin2);
    assert_eq!(client.get_pending_governance_admin(), Some(next_admin2));
}

#[test]
fn new_admin_can_update_parameters_after_transfer() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let next_admin = Address::generate(&env);

    client.initialize_governance(&admin);
    client.propose_governance_admin(&next_admin);
    client.accept_governance_admin();

    // New admin should be able to update parameters
    assert!(client.update_protocol_parameters(&200_i128, &20_u32, &1_i128, &10_i128));

    let parameters = client.get_protocol_parameters();
    assert_eq!(parameters.min_milestone_amount, 200);
}

#[test]
fn protocol_parameters_affect_contract_creation() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize_governance(&admin);

    // Set strict parameters
    client.update_protocol_parameters(&100_i128, &2_u32, &1_i128, &5_i128);

    let escrow_client = Address::generate(&env);
    let freelancer = Address::generate(&env);

    // This should work with parameters meeting requirements
    let milestones = vec![&env, 100_i128, 150_i128];
    let id = client.create_contract(
        &escrow_client,
        &freelancer,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert_eq!(id, 0);
}

#[test]
fn governance_operations_work_with_pause_controls() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let pause_admin = Address::generate(&env);
    let gov_admin = Address::generate(&env);

    // Initialize both control systems
    client.initialize(&pause_admin);
    client.initialize_governance(&gov_admin);

    // Pause the contract
    client.pause();

    // Governance operations should still work during pause
    assert!(client.update_protocol_parameters(&150_i128, &15_u32, &1_i128, &10_i128));

    let parameters = client.get_protocol_parameters();
    assert_eq!(parameters.min_milestone_amount, 150);
}

#[test]
fn complete_governance_lifecycle() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    // 1. Initialize governance
    let admin1 = Address::generate(&env);
    client.initialize_governance(&admin1);
    assert_eq!(client.get_governance_admin(), Some(admin1.clone()));

    // 2. Update parameters
    client.update_protocol_parameters(&100_i128, &10_u32, &1_i128, &10_i128);
    let params = client.get_protocol_parameters();
    assert_eq!(params.min_milestone_amount, 100);

    // 3. Propose new admin
    let admin2 = Address::generate(&env);
    client.propose_governance_admin(&admin2);
    assert_eq!(client.get_pending_governance_admin(), Some(admin2.clone()));

    // 4. Accept admin transfer
    client.accept_governance_admin();
    assert_eq!(client.get_governance_admin(), Some(admin2.clone()));
    assert_eq!(client.get_pending_governance_admin(), None);

    // 5. New admin updates parameters
    client.update_protocol_parameters(&200_i128, &20_u32, &2_i128, &8_i128);
    let params2 = client.get_protocol_parameters();
    assert_eq!(params2.min_milestone_amount, 200);
    assert_eq!(params2.max_milestones, 20);

    // 6. Transfer to third admin
    let admin3 = Address::generate(&env);
    client.propose_governance_admin(&admin3);
    client.accept_governance_admin();
    assert_eq!(client.get_governance_admin(), Some(admin3));
}
