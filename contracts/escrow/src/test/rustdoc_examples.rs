use super::EscrowFixture;
use crate::{DisputeResolution, ReleaseAuthorization};
use soroban_sdk::{vec, String};

#[test]
fn test_rustdoc_examples_flow_verification() {
    let fixture = EscrowFixture::builder().funded().build();
    let env = &fixture.env;
    let client = fixture.escrow();
    let admin = &fixture.admin;
    let client_addr = &fixture.client;
    let freelancer_addr = &fixture.freelancer;

    // 1. Check settlement token binding and getters
    assert!(client.is_settlement_token_bound());
    assert!(client.get_settlement_token().is_some());

    // 2. Read bounds and readiness info
    let bounds = client.get_bounds();
    assert_eq!(bounds.max_milestones, 10);

    let readiness = client.get_mainnet_readiness_info();
    assert!(readiness.initialized);

    // 3. Admin & Governance readers
    assert_eq!(client.get_admin(), Some(admin.clone()));
    assert_eq!(client.get_governance_admin(), Some(admin.clone()));
    assert_eq!(client.get_protocol_fee_bps(), 0);
    assert_eq!(client.get_accumulated_protocol_fees(), 0);
    assert_eq!(client.get_pending_admin_proposed_at(), None);
    assert_eq!(client.get_governed_parameters(), None);

    // 4. Create contract and query contract state
    let milestones = vec![env, 100_0000000];
    let contract_id = client.create_contract(
        client_addr,
        freelancer_addr,
        &None,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );
    assert!(client.contract_exists(&contract_id));
    let contract = client.get_contract(&contract_id);
    assert_eq!(contract.client, *client_addr);

    let next_id = client.get_next_contract_id();
    assert!(next_id > contract_id);

    let summary = client.get_contract_summary(&contract_id);
    assert_eq!(summary.schema_version, 1);

    let milestone_list = client.get_milestones(&contract_id);
    assert_eq!(milestone_list.len(), 1);

    let single_milestone = client.get_milestone(&contract_id, &0);
    assert!(single_milestone.is_some());

    let is_overdue = client.is_milestone_overdue(&contract_id, &0);
    assert!(!is_overdue);

    let refundable = client.get_refundable_balance(&contract_id);
    assert_eq!(refundable, 0); // Not funded yet

    // 5. Client migration query
    assert!(!client.has_pending_client_migration(&contract_id));

    // 6. Approval & deadline check
    let approved = client.approve_milestone_release(&contract_id, client_addr, &0);
    assert!(approved);
    assert!(client.get_milestone_approvals(&contract_id, &0).is_some());
    assert!(client.get_approval_deadline(&contract_id, &0).is_some());

    // 7. Pause & Emergency readers
    assert!(!client.is_paused());
    assert!(!client.is_emergency());

    // 8. Work evidence query
    assert_eq!(client.get_work_evidence(&contract_id, &0), None);

    // 9. Reputation getters
    assert_eq!(client.get_reputation_comment(&contract_id), None);
    assert_eq!(client.get_reputation(freelancer_addr), None);
    assert_eq!(client.get_average_rating(freelancer_addr), None);
    assert_eq!(client.get_pending_reputation_credits(freelancer_addr), 0);

    // 10. Finalization record query
    assert_eq!(client.get_finalization_record(&contract_id), None);
}
