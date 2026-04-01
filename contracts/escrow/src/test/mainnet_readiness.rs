extern crate std;

use soroban_sdk::{testutils::Address as _, vec, Address, Env};

use crate::{
    Escrow, EscrowClient, MainnetReadinessInfo, MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
    MAINNET_PROTOCOL_VERSION,
};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    (env, contract_id)
}

#[test]
fn get_mainnet_readiness_info_reports_defaults_before_governance() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let info = client.get_mainnet_readiness_info();
    assert_eq!(
        info,
        MainnetReadinessInfo {
            protocol_version: MAINNET_PROTOCOL_VERSION,
            max_escrow_total_stroops: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
            min_milestone_amount: 1,
            max_milestones: 16,
            min_reputation_rating: 1,
            max_reputation_rating: 5,
        }
    );
}

#[test]
#[should_panic(expected = "total escrow exceeds mainnet hard cap")]
fn create_contract_rejects_total_above_mainnet_hard_cap() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let cap = MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS;
    let milestones = vec![&env, cap + 1_i128];

    let _ = client.create_contract(&client_addr, &freelancer_addr, &milestones);
}

#[test]
fn get_mainnet_readiness_info_reflects_governed_parameters() {
    let (env, contract_id) = setup();
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    assert!(client.initialize_protocol_governance(&admin, &50_i128, &4_u32, &2_i128, &4_i128));

    let info = client.get_mainnet_readiness_info();
    assert_eq!(info.protocol_version, MAINNET_PROTOCOL_VERSION);
    assert_eq!(
        info.max_escrow_total_stroops,
        MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
    );
    assert_eq!(info.min_milestone_amount, 50);
    assert_eq!(info.max_milestones, 4);
    assert_eq!(info.min_reputation_rating, 2);
    assert_eq!(info.max_reputation_rating, 4);
}
