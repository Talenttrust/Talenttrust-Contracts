//! Documentation/API drift guards for the escrow contract.
//!
//! These tests keep reviewer-facing docs aligned with the public entrypoints in
//! `lib.rs` so planned features are not accidentally documented as live API.

#![cfg(test)]

extern crate std;

const LIB_RS: &str = include_str!("../lib.rs");
const FINALIZE_RS: &str = include_str!("../finalize.rs");
const DOCS_README: &str = include_str!("../../../../docs/escrow/README.md");
const DOCS_CONTRACT: &str = include_str!("../../../../docs/escrow/contract.md");
const CONTRACT_README: &str = include_str!("../../README.md");
const ROOT_README: &str = include_str!("../../../../README.md");

const IMPLEMENTED_ENTRYPOINTS: [&str; 19] = [
    "initialize",
    "get_admin",
    "pause",
    "unpause",
    "is_paused",
    "activate_emergency_pause",
    "resolve_emergency",
    "is_emergency",
    "get_mainnet_readiness_info",
    "create_contract",
    "deposit_funds",
    "release_milestone",
    "issue_reputation",
    "cancel_contract",
    "finalize_contract",
    "get_contract",
    "get_finalization_record",
    "get_reputation",
    "get_pending_reputation_credits",
];

const PLANNED_ENTRYPOINTS: [&str; 14] = [
    "withdraw_leftover",
    "refund_unreleased_milestones",
    "dispute_contract",
    "approve_milestone",
    "approve_milestone_release",
    "initialize_protocol_governance",
    "initialize_governance",
    "update_protocol_parameters",
    "propose_governance_admin",
    "accept_governance_admin",
    "get_governance_admin",
    "get_pending_governance_admin",
    "withdraw_protocol_fees",
    "migrate_state",
];

#[test]
fn implemented_entrypoint_list_matches_lib_rs_public_surface() {
    let mut public_count = 0;

    for source in [LIB_RS, FINALIZE_RS] {
        for line in source.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("pub fn ") {
                public_count += 1;
                let after_prefix = &trimmed["pub fn ".len()..];
                let name_end = after_prefix
                    .find('(')
                    .expect("public function should include an argument list");
                let name = &after_prefix[..name_end];
                assert!(
                    IMPLEMENTED_ENTRYPOINTS.contains(&name),
                    "documented API guard is missing public function `{}`",
                    name
                );
            }
        }
    }

    assert_eq!(
        public_count,
        IMPLEMENTED_ENTRYPOINTS.len(),
        "implemented entrypoint list must match lib.rs pub fn count"
    );
}

#[test]
fn canonical_docs_list_every_implemented_entrypoint() {
    for entrypoint in IMPLEMENTED_ENTRYPOINTS {
        assert!(
            DOCS_README.contains(entrypoint),
            "docs/escrow/README.md must document `{}`",
            entrypoint
        );
        assert!(
            DOCS_CONTRACT.contains(entrypoint),
            "docs/escrow/contract.md must document `{}`",
            entrypoint
        );
        assert!(
            CONTRACT_README.contains(entrypoint),
            "contracts/escrow/README.md must document `{}`",
            entrypoint
        );
    }
}

#[test]
fn canonical_docs_mark_unimplemented_entrypoints_as_planned() {
    for entrypoint in PLANNED_ENTRYPOINTS {
        assert_not_live_api(DOCS_README, entrypoint, "docs/escrow/README.md");
        assert_not_live_api(DOCS_CONTRACT, entrypoint, "docs/escrow/contract.md");
        assert_not_live_api(CONTRACT_README, entrypoint, "contracts/escrow/README.md");
        assert_not_live_api(ROOT_README, entrypoint, "README.md");
    }
}

#[test]
fn release_milestone_docs_do_not_claim_caller_authorization() {
    for (doc_name, doc) in [
        ("docs/escrow/README.md", DOCS_README),
        ("docs/escrow/contract.md", DOCS_CONTRACT),
        ("contracts/escrow/README.md", CONTRACT_README),
        ("README.md", ROOT_README),
    ] {
        assert!(
            doc.contains("does not authenticate")
                || doc.contains("does not yet authenticate")
                || doc.contains("caller authorization is not yet implemented"),
            "{} must document the current release_milestone authorization gap",
            doc_name
        );
        assert!(
            !doc.contains("release_milestone` | Client")
                && !doc.contains("releases require the recorded client"),
            "{} must not claim release_milestone is client-authorized",
            doc_name
        );
    }
}

fn assert_not_live_api(doc: &str, entrypoint: &str, doc_name: &str) {
    let live_signature = {
        let mut s = std::string::String::from("- `");
        s.push_str(entrypoint);
        s.push('(');
        s
    };
    let fn_label = {
        let mut s = std::string::String::from("**Function:** `");
        s.push_str(entrypoint);
        s.push('`');
        s
    };

    assert!(
        !doc.contains(&live_signature) && !doc.contains(&fn_label),
        "{} must not list `{}` as an implemented entrypoint",
        doc_name,
        entrypoint
    );

    if doc.contains(entrypoint) {
        assert!(
            doc.contains("Planned")
                || doc.contains("planned")
                || doc.contains("not implemented")
                || doc.contains("not live"),
            "{} mentions `{}` and must label it as planned/not implemented",
            doc_name,
            entrypoint
        );
    }
}
