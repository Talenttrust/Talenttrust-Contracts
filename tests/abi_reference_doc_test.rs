use std::{fs, path::Path};

#[test]
fn abi_reference_document_lists_current_public_entrypoints() {
    // Integration test lives under contracts/escrow; ABI docs are at repo root.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut root = manifest_dir.to_path_buf();
    while !root.join("docs").join("escrow").join("abi-reference.md").exists() && root.parent().is_some() {
        root = root.parent().unwrap().to_path_buf();
    }
    let doc_path = root.join("docs").join("escrow").join("abi-reference.md");
    let contents = fs::read_to_string(&doc_path)
        .unwrap_or_else(|_| panic!("expected ABI reference at {:?}", doc_path));

    let expected_entrypoints = [
        "initialize",
        "get_admin",
        "get_mainnet_readiness_info",
        "create_contract",
        "deposit_funds",
        "finalize_contract",
        "get_finalization_record",
        "propose_client_migration",
        "accept_client_migration",
        "has_pending_client_migration",
        "get_pending_client_migration",
        "approve_milestone_release",
        "release_milestone",
        "refund_unreleased_milestones",
        "get_contract",
        "get_milestones",
        "get_refundable_balance",
        "get_milestone_approvals",
        "pause",
        "unpause",
        "is_paused",
        "activate_emergency_pause",
        "resolve_emergency",
        "is_emergency",
        "cancel_contract",
        "raise_dispute",
        "raise_dispute_batch",
        "resolve_dispute",
        "issue_reputation",
        "issue_reputation_batch",
        "get_reputation_comment",
        "get_reputation",
        "get_average_rating",
        "get_pending_reputation_credits",
        "submit_work_evidence",
        "get_work_evidence",
        "set_protocol_fee_bps",
        "get_protocol_fee_bps_view",
        "get_accumulated_protocol_fees",
        "propose_governance_admin",
        "accept_governance_admin",
        "get_pending_governance_admin",
        "get_governance_admin",
        "set_governed_params",
        "get_governed_parameters",
        "is_governed_params_set",
    ];

    for entrypoint in expected_entrypoints {
        assert!(
            contents.contains(entrypoint),
            "ABI reference should document the public entrypoint `{entrypoint}`"
        );
    }

    let forbidden_entrypoints = [
        "hello",
        "withdraw_protocol_fees",
        "migrate_state",
        "get_state",
        "set_milestone_funded",
    ];

    for entrypoint in forbidden_entrypoints {
        assert!(
            !contents.contains(entrypoint),
            "ABI reference should not list planned entrypoint `{entrypoint}` as live"
        );
    }
}
