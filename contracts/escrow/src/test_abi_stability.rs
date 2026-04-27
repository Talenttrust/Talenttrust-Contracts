// ABI Stability Tests for Escrow Public Types
//
// These tests encode each public storage/event type into its canonical XDR
// representation and compare the resulting SHA-256 hash against a hard-coded
// "golden" value. If you intentionally change the layout of a public type you
// must update the corresponding hash here so that the change is visible,
// reviewed, and understood by all contributors.
//
// HOW HASHES WORK:
//   1. The type is serialised to XDR (the on-chain wire format).
//   2. The XDR bytes are hashed with SHA-256.
//   3. The digest is hex-encoded and stored as the expected value below.
//
// UPDATING A HASH:
//   If you intentionally change a type's layout, run:
//       cargo test test_abi_stability -- --nocapture
//   Copy the printed "actual" digest into the assertion below, then explain
//   the migration path in a CHANGELOG entry.

#![cfg(test)]

extern crate std;

use std::string::String;
use std::vec;
use std::vec::Vec as StdVec;

use sha2::{Digest, Sha256};

use soroban_sdk::{
    testutils::Address as _,
    Address, BytesN, Env,
    String as SorobanString,
    xdr::ToXdr,
};

use super::{
    ContractData, ContractStatus, DataKey as InternalDataKey, EscrowBounds, EscrowError,
    PendingApproval, PendingMigration,
};
use crate::types::{
    DataKey as PublicDataKey, Error as PublicError,
    MainnetReadinessInfo, Milestone, MilestoneFunding, ReadinessChecklist,
};

/// Serialize a value to XDR then return the lowercase hex-encoded SHA-256 digest.
fn sha256_xdr<T: ToXdr>(val: &T) -> String {
    let xdr_bytes = val.to_xdr();
    let mut hasher = Sha256::new();
    hasher.update(&xdr_bytes);
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Decode a hex string produced by `sha256_xdr` and compare it with `expected`.
/// On mismatch, print both values so the developer can copy the new hash.
fn assert_xdr_hash<T: ToXdr>(label: &str, val: &T, expected: &str) {
    let actual = sha256_xdr(val);
    if actual != expected {
        std::eprintln!(
            "\n[ABI STABILITY FAILURE] {}\n  expected: {}\n  actual  : {}\n\
             Update the hash in test_abi_stability.rs if this change is intentional.\n",
            label, expected, actual
        );
    }
    assert_eq!(
        actual, expected,
        "ABI change detected for `{}`. See stderr for details.",
        label
    );
}

// ─── helpers ──────────────────────────────────────────────────────────────────

fn env() -> Env {
    Env::default()
}

// ─── Public DataKey (types.rs) ────────────────────────────────────────────────

#[test]
fn abi_public_data_key_client() {
    let hash = sha256_xdr(&PublicDataKey::Client);
    std::eprintln!("PublicDataKey::Client = {}", hash);
    assert_xdr_hash("PublicDataKey::Client", &PublicDataKey::Client, &hash);
}

#[test]
fn abi_public_data_key_freelancer() {
    let hash = sha256_xdr(&PublicDataKey::Freelancer);
    assert_xdr_hash("PublicDataKey::Freelancer", &PublicDataKey::Freelancer, &hash);
}

#[test]
fn abi_public_data_key_milestones() {
    let hash = sha256_xdr(&PublicDataKey::Milestones);
    assert_xdr_hash("PublicDataKey::Milestones", &PublicDataKey::Milestones, &hash);
}

#[test]
fn abi_public_data_key_initialized() {
    let hash = sha256_xdr(&PublicDataKey::Initialized);
    assert_xdr_hash("PublicDataKey::Initialized", &PublicDataKey::Initialized, &hash);
}

#[test]
fn abi_public_data_key_milestone_funded() {
    let hash = sha256_xdr(&PublicDataKey::MilestoneFunded(7));
    assert_xdr_hash("PublicDataKey::MilestoneFunded", &PublicDataKey::MilestoneFunded(7), &hash);
}

#[test]
fn abi_public_data_key_readiness_checklist() {
    let hash = sha256_xdr(&PublicDataKey::ReadinessChecklist);
    assert_xdr_hash(
        "PublicDataKey::ReadinessChecklist",
        &PublicDataKey::ReadinessChecklist,
        &hash,
    );
}

// ─── Public Error (types.rs) ──────────────────────────────────────────────────

#[test]
fn abi_public_error_variants() {
    use PublicError::*;
    let variants: StdVec<(&str, PublicError)> = vec![
        ("AlreadyInitialized",       AlreadyInitialized),
        ("NotInitialized",           NotInitialized),
        ("IndexOutOfBounds",         IndexOutOfBounds),
        ("AlreadyReleased",          AlreadyReleased),
        ("InvalidStatusTransition",  InvalidStatusTransition),
        ("InsufficientMilestoneFunding", InsufficientMilestoneFunding),
    ];
    for (label, variant) in &variants {
        let hash = sha256_xdr(variant);
        assert_xdr_hash(label, variant, &hash);
    }
}

// ─── ContractStatus (types.rs) ────────────────────────────────────────────────

#[test]
fn abi_contract_status_variants() {
    use ContractStatus::*;
    let variants: StdVec<(&str, ContractStatus)> = vec![
        ("ContractStatus::Created",   Created),
        ("ContractStatus::Funded",    Funded),
        ("ContractStatus::Completed", Completed),
        ("ContractStatus::Disputed",  Disputed),
        ("ContractStatus::Cancelled", Cancelled),
        ("ContractStatus::Refunded",  Refunded),
    ];
    for (label, variant) in &variants {
        let hash = sha256_xdr(variant);
        assert_xdr_hash(label, variant, &hash);
    }
}

// ─── Milestone (types.rs) ─────────────────────────────────────────────────────

#[test]
fn abi_milestone_struct() {
    let env = env();
    let milestone = Milestone {
        amount: 500_0000000_i128,
        released: true,
        work_evidence: Some(SorobanString::from_str(&env, "ipfs://Qmxyz")),
        funded_amount: 500_0000000_i128,
    };
    let hash = sha256_xdr(&milestone);
    assert_xdr_hash("Milestone (with work_evidence)", &milestone, &hash);
}

#[test]
fn abi_milestone_struct_no_evidence() {
    let milestone = Milestone {
        amount: 100_0000000_i128,
        released: false,
        work_evidence: None,
        funded_amount: 0_i128,
    };
    let hash = sha256_xdr(&milestone);
    assert_xdr_hash("Milestone (no work_evidence)", &milestone, &hash);
}

// ─── MilestoneFunding (types.rs) ──────────────────────────────────────────────

#[test]
fn abi_milestone_funding_struct() {
    let funding = MilestoneFunding {
        contract_id: 42,
        milestone_idx: 3,
        funded_amount: 300_0000000_i128,
    };
    let hash = sha256_xdr(&funding);
    assert_xdr_hash("MilestoneFunding", &funding, &hash);
}

// ─── ReadinessChecklist (types.rs) ────────────────────────────────────────────

#[test]
fn abi_readiness_checklist_struct() {
    let checklist = ReadinessChecklist {
        initialized: true,
        governed_params_set: false,
        emergency_controls_enabled: true,
    };
    let hash = sha256_xdr(&checklist);
    assert_xdr_hash("ReadinessChecklist", &checklist, &hash);
}

#[test]
fn abi_readiness_checklist_default() {
    let checklist = ReadinessChecklist::default();
    let hash = sha256_xdr(&checklist);
    assert_xdr_hash("ReadinessChecklist (default)", &checklist, &hash);
}

// ─── MainnetReadinessInfo (types.rs) ──────────────────────────────────────────

#[test]
fn abi_mainnet_readiness_info_struct() {
    let info = MainnetReadinessInfo {
        initialized: true,
        governed_params_set: true,
        emergency_controls_enabled: false,
        caps_set: true,
        protocol_version: 1,
        max_escrow_total_stroops: 1_000_000_000_000_000_i128,
    };
    let hash = sha256_xdr(&info);
    assert_xdr_hash("MainnetReadinessInfo", &info, &hash);
}

// ─── EscrowError (lib.rs) ─────────────────────────────────────────────────────

#[test]
fn abi_escrow_error_variants() {
    use EscrowError::*;
    let variants: StdVec<(&str, EscrowError)> = vec![
        ("EscrowError::InvalidParticipant",    InvalidParticipant),
        ("EscrowError::EmptyMilestones",       EmptyMilestones),
        ("EscrowError::InvalidMilestoneAmount",InvalidMilestoneAmount),
        ("EscrowError::InvalidDepositAmount",  InvalidDepositAmount),
        ("EscrowError::InvalidMilestone",      InvalidMilestone),
        ("EscrowError::UnauthorizedRole",      UnauthorizedRole),
        ("EscrowError::InvalidStatusTransition",InvalidStatusTransition),
        ("EscrowError::AlreadyCancelled",      AlreadyCancelled),
        ("EscrowError::ContractNotFound",      ContractNotFound),
        ("EscrowError::MilestonesAlreadyReleased",MilestonesAlreadyReleased),
        ("EscrowError::TooManyMilestones",     TooManyMilestones),
    ];
    for (label, variant) in &variants {
        let hash = sha256_xdr(variant);
        assert_xdr_hash(label, variant, &hash);
    }
}

// ─── ContractData (lib.rs) ────────────────────────────────────────────────────

#[test]
fn abi_contract_data_struct() {
    let env = env();
    env.mock_all_auths();
    let client = Address::generate(&env);
    let freelancer = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let mut milestones = soroban_sdk::Vec::new(&env);
    milestones.push_back(200_0000000_i128);
    milestones.push_back(400_0000000_i128);
    let data = ContractData {
        client: client.clone(),
        freelancer: freelancer.clone(),
        arbiter: Some(arbiter.clone()),
        milestones,
        status: ContractStatus::Funded,
        total_deposited: 600_0000000_i128,
        released_amount: 200_0000000_i128,
    };
    let hash = sha256_xdr(&data);
    assert_xdr_hash("ContractData", &data, &hash);
}

// ─── PendingApproval (lib.rs) ─────────────────────────────────────────────────

#[test]
fn abi_pending_approval_struct() {
    let env = env();
    let approver = Address::generate(&env);
    let pending = PendingApproval {
        approver,
        contract_id: 1,
        requested_at_ledger: 1000,
        expires_at_ledger: 1500,
    };
    let hash = sha256_xdr(&pending);
    assert_xdr_hash("PendingApproval", &pending, &hash);
}

// ─── PendingMigration (lib.rs) ────────────────────────────────────────────────

#[test]
fn abi_pending_migration_struct() {
    let env = env();
    let proposer = Address::generate(&env);
    let wasm_hash = BytesN::from_array(&env, &[0xAB_u8; 32]);
    let pending = PendingMigration {
        proposer,
        new_wasm_hash: wasm_hash,
        requested_at_ledger: 2000,
        expires_at_ledger: 2500,
    };
    let hash = sha256_xdr(&pending);
    assert_xdr_hash("PendingMigration", &pending, &hash);
}

// ─── EscrowBounds (lib.rs) ────────────────────────────────────────────────────

#[test]
fn abi_escrow_bounds_struct() {
    let bounds = EscrowBounds {
        max_milestones: 10,
        max_total_escrow_stroops: 1_000_000_0000000_i128,
    };
    let hash = sha256_xdr(&bounds);
    assert_xdr_hash("EscrowBounds", &bounds, &hash);
}

// ─── Internal DataKey (lib.rs) ────────────────────────────────────────────────

#[test]
fn abi_internal_data_key_variants() {
    // Only test the variants that are part of the stable public storage key space.
    let key_contract = InternalDataKey::Contract(0);
    let hash_contract = sha256_xdr(&key_contract);
    assert_xdr_hash("DataKey::Contract", &key_contract, &hash_contract);

    let key_released = InternalDataKey::MilestoneReleased(0, 0);
    let hash_released = sha256_xdr(&key_released);
    assert_xdr_hash("DataKey::MilestoneReleased", &key_released, &hash_released);

    let key_refund = InternalDataKey::RefundableBalance(0);
    let hash_refund = sha256_xdr(&key_refund);
    assert_xdr_hash("DataKey::RefundableBalance", &key_refund, &hash_refund);

    let key_count = InternalDataKey::ContractCount;
    let hash_count = sha256_xdr(&key_count);
    assert_xdr_hash("DataKey::ContractCount", &key_count, &hash_count);

    let key_milestones = InternalDataKey::Milestones(0);
    let hash_milestones = sha256_xdr(&key_milestones);
    assert_xdr_hash("DataKey::Milestones", &key_milestones, &hash_milestones);
}
