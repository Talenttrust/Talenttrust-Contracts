#![cfg(test)]

use crate::types::{ContractStatus, ReleaseAuthorization};
use crate::{Escrow, EscrowClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal, String, Symbol, Vec,
};

fn setup_escrow_for_events<'a>(
    env: &'a Env,
    amounts: &[i128],
) -> (EscrowClient<'a>, Address, Address, Address, u32) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);

    let admin = Address::generate(env);
    client.initialize(&admin);

    let client_addr = Address::generate(env);
    let freelancer_addr = Address::generate(env);

    let mut milestones = Vec::new(env);
    let mut total_amount = 0i128;
    for &amt in amounts {
        milestones.push_back(amt);
        total_amount += amt;
    }

    let c_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    client.deposit_funds(&c_id, &client_addr, &total_amount);

    (client, admin, client_addr, freelancer_addr, c_id)
}

#[test]
fn test_milestone_release_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _freelancer_addr, c_id) =
        setup_escrow_for_events(&env, &[1_000, 2_000]);

    let initial_events_count = env.events().all().len();

    // Release milestone 0
    assert!(client.release_milestone(&c_id, &client_addr, &0));

    let events_after = env.events().all();
    assert!(
        events_after.len() > initial_events_count,
        "Milestone release must emit events"
    );
}

#[test]
fn test_milestone_refund_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _freelancer_addr, c_id) =
        setup_escrow_for_events(&env, &[1_000, 2_000]);

    let initial_events_count = env.events().all().len();

    let mut indices = Vec::new(&env);
    indices.push_back(0);
    indices.push_back(1);
    let refund_res = client.refund_unreleased_milestones(&c_id, &indices);
    assert_eq!(refund_res, 3_000);

    let events_after = env.events().all();
    assert!(
        events_after.len() > initial_events_count,
        "Milestone refund must emit events"
    );
}

#[test]
fn test_work_evidence_submission_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _client_addr, freelancer_addr, c_id) =
        setup_escrow_for_events(&env, &[1_000, 2_000]);

    let initial_events_count = env.events().all().len();

    let evidence = String::from_str(&env, "ipfs://bafybeic555");
    assert!(client.submit_work_evidence(&c_id, &freelancer_addr, &0, &evidence));

    let events_after = env.events().all();
    assert!(
        events_after.len() > initial_events_count,
        "Work evidence submission must emit events"
    );
}

#[test]
fn test_read_only_entrypoints_emit_no_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, client_addr, _freelancer_addr, c_id) =
        setup_escrow_for_events(&env, &[1_000, 2_000]);

    let baseline_count = env.events().all().len();

    // Call various read-only methods
    let _contract = client.get_contract(&c_id);
    let _milestones = client.get_milestones(&c_id);
    let _summary = client.get_contract_summary(&c_id);
    let _evidence = client.get_work_evidence(&c_id, &0);
    let _progress = client.get_milestone_progress(&c_id);
    let _schema = client.get_schema_version();

    let after_reads_count = env.events().all().len();
    assert_eq!(
        baseline_count, after_reads_count,
        "Read-only queries must never emit events"
    );
}
