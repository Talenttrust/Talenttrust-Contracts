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

// ─── Regression tests: released_milestone_count parity (fixes #416) ──────────
//
// `release_milestone` sets `ms.released = true` in the persisted milestone
// vector. `summarize_contract` reads that flag directly — it is the single
// source of truth. `DataKey::MilestoneReleased` was a never-written variant
// and has been removed from `types.rs`.
//
// These tests assert that `released_milestone_count` in the finalization
// summary always equals the number of milestones with `released == true` in
// the vector, across zero / partial / full / mixed-refund scenarios.

#[cfg(test)]
mod released_count_parity {
    use crate::{ContractStatus, Escrow, EscrowClient, ReleaseAuthorization};
    use soroban_sdk::{testutils::Address as _, vec, Address, Env};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let addr = env.register(Escrow, ());
        (env, addr)
    }

    fn client<'a>(env: &'a Env, addr: &Address) -> EscrowClient<'a> {
        EscrowClient::new(env, addr)
    }

    /// Assert count in summary equals count of `released` flags in milestone summaries.
    fn assert_parity(count: u32, milestones: &soroban_sdk::Vec<crate::types::MilestoneSummary>) {
        let from_vec = milestones.iter().filter(|m| m.released).count() as u32;
        assert_eq!(
            count, from_vec,
            "released_milestone_count must equal milestone vector released flags"
        );
    }

    /// Zero released: dispute → finalize without releasing any milestone.
    #[test]
    fn summary_count_zero_when_no_milestones_released() {
        let (env, addr) = setup();
        let c = client(&env, &addr);
        let cl = Address::generate(&env);
        let fr = Address::generate(&env);
        let arb = Address::generate(&env);
        let id = c.create_contract(
            &cl, &fr, &Some(arb.clone()),
            &vec![&env, 100_i128, 200_i128],
            &ReleaseAuthorization::ClientOnly,
        );
        c.deposit_funds(&id, &cl, &300_i128);
        c.raise_dispute(&id, &cl);
        c.finalize_contract(&id, &arb);
        let record = c.get_finalization_record(&id).unwrap();
        assert_eq!(record.summary.released_milestone_count, 0);
        assert_parity(record.summary.released_milestone_count, &record.summary.milestones);
    }

    /// Partial release (2 of 3): third milestone refunded to reach Completed.
    #[test]
    fn summary_count_matches_partial_release() {
        let (env, addr) = setup();
        let c = client(&env, &addr);
        let cl = Address::generate(&env);
        let fr = Address::generate(&env);
        let id = c.create_contract(
            &cl, &fr, &None,
            &vec![&env, 100_i128, 200_i128, 300_i128],
            &ReleaseAuthorization::ClientOnly,
        );
        c.deposit_funds(&id, &cl, &600_i128);
        c.approve_milestone_release(&id, &cl, &0);
        c.release_milestone(&id, &cl, &0);
        c.approve_milestone_release(&id, &cl, &1);
        c.release_milestone(&id, &cl, &1);
        c.refund_unreleased_milestones(&id, &vec![&env, 2_u32]);
        assert_eq!(c.get_contract(&id).status, ContractStatus::Completed);
        c.finalize_contract(&id, &cl);
        let record = c.get_finalization_record(&id).unwrap();
        assert_eq!(record.summary.released_milestone_count, 2);
        assert_parity(record.summary.released_milestone_count, &record.summary.milestones);
    }

    /// Full release: all 3 milestones released.
    #[test]
    fn summary_count_matches_full_release() {
        let (env, addr) = setup();
        let c = client(&env, &addr);
        let cl = Address::generate(&env);
        let fr = Address::generate(&env);
        let id = c.create_contract(
            &cl, &fr, &None,
            &vec![&env, 100_i128, 200_i128, 300_i128],
            &ReleaseAuthorization::ClientOnly,
        );
        c.deposit_funds(&id, &cl, &600_i128);
        for idx in [0_u32, 1, 2] {
            c.approve_milestone_release(&id, &cl, &idx);
            c.release_milestone(&id, &cl, &idx);
        }
        assert_eq!(c.get_contract(&id).status, ContractStatus::Completed);
        c.finalize_contract(&id, &cl);
        let record = c.get_finalization_record(&id).unwrap();
        assert_eq!(record.summary.released_milestone_count, 3);
        assert_parity(record.summary.released_milestone_count, &record.summary.milestones);
    }

    /// Mixed release+refund: refunded milestones must not inflate the count.
    #[test]
    fn summary_count_excludes_refunded_milestones() {
        let (env, addr) = setup();
        let c = client(&env, &addr);
        let cl = Address::generate(&env);
        let fr = Address::generate(&env);
        let id = c.create_contract(
            &cl, &fr, &None,
            &vec![&env, 100_i128, 200_i128],
            &ReleaseAuthorization::ClientOnly,
        );
        c.deposit_funds(&id, &cl, &300_i128);
        c.approve_milestone_release(&id, &cl, &0);
        c.release_milestone(&id, &cl, &0);
        c.refund_unreleased_milestones(&id, &vec![&env, 1_u32]);
        assert_eq!(c.get_contract(&id).status, ContractStatus::Completed);
        c.finalize_contract(&id, &cl);
        let record = c.get_finalization_record(&id).unwrap();
        // 1 released, 1 refunded — count must be 1, not 2.
        assert_eq!(record.summary.released_milestone_count, 1);
        assert_parity(record.summary.released_milestone_count, &record.summary.milestones);
    }
}
