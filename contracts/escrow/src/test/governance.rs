extern crate std;

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{Escrow, EscrowClient, ProtocolParameters};

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
fn governance_initialization_and_updates_change_live_validation_rules() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    assert!(client.initialize_protocol_governance(&admin, &50_i128, &2_u32, &2_i128, &4_i128));

    let parameters = client.get_protocol_parameters();
    assert_eq!(
        parameters,
        ProtocolParameters {
            min_milestone_amount: 50,
            max_milestones: 2,
            min_reputation_rating: 2,
            max_reputation_rating: 4,
        }
    );
    assert_eq!(client.get_governance_admin(), Some(admin.clone()));

    assert!(client.update_protocol_parameters(&75_i128, &3_u32, &1_i128, &5_i128));

    let updated = client.get_protocol_parameters();
    assert_eq!(updated.min_milestone_amount, 75);
    assert_eq!(updated.max_milestones, 3);
    assert_eq!(updated.min_reputation_rating, 1);
    assert_eq!(updated.max_reputation_rating, 5);

    let escrow_client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let milestones = vec![&env, 75_i128, 90_i128, 120_i128];

    let id = client.create_contract(&escrow_client, &freelancer, &milestones);
    client.deposit_funds(&id, &285_i128);
    client.release_milestone(&id, &0);
    client.release_milestone(&id, &1);
    client.release_milestone(&id, &2);

    assert!(client.issue_reputation(&freelancer, &5_i128));
}

#[test]
fn governance_admin_transfer_is_two_step() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let next_admin = Address::generate(&env);
    client.initialize_protocol_governance(&admin, &10_i128, &4_u32, &1_i128, &5_i128);

    assert!(client.propose_governance_admin(&next_admin));
    assert_eq!(
        client.get_pending_governance_admin(),
        Some(next_admin.clone())
    );

    assert!(client.accept_governance_admin());
    assert_eq!(client.get_governance_admin(), Some(next_admin));
    assert_eq!(client.get_pending_governance_admin(), None);
}
