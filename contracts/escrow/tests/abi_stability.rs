// ABI Stability Tests — Escrow Public Storage / Event Types
//
// Each test serialises a value of a public #[contracttype] to its canonical
// XDR wire format, SHA-256 hashes the bytes, and compares the digest against
// a hard-coded "golden" string baked into the source code.
//
// WHY HARD-CODED HASHES (NOT SNAPSHOTS)?
//   A snapshot file can be silently regenerated; a hash literal requires an
//   intentional source edit and is visible in every diff.
//
// HOW TO UPDATE A HASH after an intentional type change:
//   1. Run:  cargo test --test abi_stability -- --nocapture 2>&1 | grep "actual"
//   2. Copy the printed digest into the assertion below.
//   3. Add a CHANGELOG entry with the migration path for existing ledger data.
//
// COVERAGE — every #[contracttype] / #[contracterror] that touches on-chain
// storage or events is tested here.

use escrow::{
    ContractData, ContractStatus, DataKey, EscrowBounds, EscrowError, MainnetReadinessInfo,
    Milestone, MilestoneFunding, PendingApproval, PendingMigration, ReadinessChecklist,
    StorageError, StorageKey,
};
use sha2::{Digest, Sha256};
use hex;
use soroban_sdk::{testutils::Address as _, xdr::ToXdr, Address, BytesN, Env, String as SStr};

// ─── helpers ──────────────────────────────────────────────────────────────────

fn sha256_xdr<T: ToXdr + Clone>(env: &Env, v: &T) -> String {
    // Serialize the value to XDR. Clone is required because ToXdr::to_xdr consumes the value.
    let xdr_bytes = v.clone().to_xdr(env);
    // Copy the Bytes into a Vec<u8> so we can hash it
    let mut buf = vec![0u8; xdr_bytes.len() as usize];
    xdr_bytes.copy_into_slice(&mut buf);
    let digest = Sha256::digest(&buf);
    hex::encode(digest)
}


/// Assert that `val`'s XDR hash equals `expected`.
/// On failure the actual digest is printed so the developer can copy it.
fn check<T: ToXdr + Clone>(env: &Env, label: &str, val: &T, expected: &str) {
    let actual = sha256_xdr(env, val);
    if actual != expected {
        eprintln!(
            "\n[ABI STABILITY] {label}\n  expected : {expected}\n  actual   : {actual}\n\
             Copy the actual value into tests/abi_stability.rs, then document the\n\
             breaking change in CHANGELOG.\n"
        );
    }
    assert_eq!(actual, expected, "ABI change in `{label}`");
}

fn env() -> Env {
    Env::default()
}

fn addr(env: &Env, _seed: u8) -> Address {
    Address::generate(env)
}

// ═══════════════════════════════════════════════════════════════════════════
//  StorageKey — public DataKey alias (types.rs)
//  Discriminants are on-chain forever; never reorder or renumber.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn storage_key_client() {
    let env = env();
    check(
        &env,
        "StorageKey::Client",
        &StorageKey::Client,
        "eef93d5363a8616c7ccc431f386b53a6ed706682b85afeb04252b985abefcae0",
    );
}
#[test]
fn storage_key_freelancer() {
    let env = env();
    check(
        &env,
        "StorageKey::Freelancer",
        &StorageKey::Freelancer,
        "9f042ab0fddcbaaeafd61049fb5be0e24ca9fa1d22df3647364fdc8ce88174ff",
    );
}
#[test]
fn storage_key_milestones_v() {
    let env = env();
    check(
        &env,
        "StorageKey::Milestones",
        &StorageKey::Milestones,
        "981514714750c0e239460b54c227c7e0bf9c4b94f1dce86af22e49ec540b9aac",
    );
}
#[test]
fn storage_key_initialized() {
    let env = env();
    check(
        &env,
        "StorageKey::Initialized",
        &StorageKey::Initialized,
        "ce39091666b79ee80b45368a791f7104ec619de043b5807afa4d12e6752ed928",
    );
}
#[test]
fn storage_key_milestone_funded() {
    let env = env();
    let v = StorageKey::MilestoneFunded(0);
    check(
        &env,
        "StorageKey::MilestoneFunded(0)",
        &v,
        "cc631b204d37bc043585015ad39f2ad193726a117c21b42c13359c14f9305ae3",
    );
}
#[test]
fn storage_key_readiness_checklist_v() {
    let env = env();
    check(
        &env,
        "StorageKey::ReadinessChecklist",
        &StorageKey::ReadinessChecklist,
        "2cad4908aecbc55222352b9da9a31e29770b558616ca2ffa14b0e2fd2573ef2f",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
//  StorageError — public Error alias (types.rs)
//  repr(u32) values are on-chain; never change a discriminant.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn storage_error_variants() {
    let env = env();
    use StorageError::*;
    let cases: &[(&str, StorageError, &str)] = &[
        (
            "StorageError::AlreadyInitialized=1",
            AlreadyInitialized,
            "01251abbff7ee711b66504a49053feadef5082d7a905e4b3484c5433eb7da510",
        ),
        (
            "StorageError::NotInitialized=2",
            NotInitialized,
            "3501b40b70fb759c42c1ac6ba9f9a31647a4b25a000bab5660b11ef22a22dd3f",
        ),
        (
            "StorageError::IndexOutOfBounds=3",
            IndexOutOfBounds,
            "34c803b41d09933e77d4612ac4fb891f08f240c18b4dc11193f68842d6146198",
        ),
        (
            "StorageError::AlreadyReleased=4",
            AlreadyReleased,
            "ad584112864055384a2a11a7da56ced74b2d76e1cc89119fad8f5058a507d754",
        ),
        (
            "StorageError::InvalidStatusTransition=5",
            InvalidStatusTransition,
            "84b77c5935909a4c5a5f9d4a896cdfedad97378edd306405cd1760066d5e1bd1",
        ),
        (
            "StorageError::InsufficientMilestoneFunding=6",
            InsufficientMilestoneFunding,
            "85f191bef350f6d15c966ee6db97ad270ee71c1b8006210a40284094360f46ff",
        ),
    ];
    for (lbl, v, hash) in cases {
        check(&env, lbl, v, hash);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  ContractStatus (types.rs) — repr stored in ContractData
// ═══════════════════════════════════════════════════════════════════════════
 
#[test]
fn contract_status_variants() {
    let env = env();
    use ContractStatus::*;
    let cases: &[(&str, ContractStatus, &str)] = &[
        (
            "ContractStatus::Created=0",
            Created,
            "0e337b799779e966201b7e95a326e564267fa4d6573062e3de6f7835d0623d17",
        ),
        (
            "ContractStatus::Funded=1",
            Funded,
            "8f88362ee8034de8dfd2d681d099066cada58b40ee0bbbc2321309d0b4f4d43f",
        ),
        (
            "ContractStatus::Completed=2",
            Completed,
            "6e84f9a5bc0587b940f5ee0dcae0b2e952a871537ed8e36a1360f9de4805b2b2",
        ),
        (
            "ContractStatus::Disputed=3",
            Disputed,
            "4e01dbd20f0d807f0ad16d9d694590fe9f3775dfef33d2e11b76b79498c2326e",
        ),
        (
            "ContractStatus::Cancelled=4",
            Cancelled,
            "31688a3e82cd7fa7b1080231d7372270a30ed63e3d684a9d5429ffc996f8aa07",
        ),
        (
            "ContractStatus::Refunded=5",
            Refunded,
            "f025848a8bdba3228d9aea9d66a58eda6b2070bd56f5f5d4fa45df585f9de0f0",
        ),
    ];
    for (lbl, v, hash) in cases {
        check(&env, lbl, v, hash);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  Milestone (types.rs)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn milestone_with_evidence() {
    let env = env();
    let v = Milestone {
        amount: 500_0000000_i128,
        released: true,
        work_evidence: Some(SStr::from_str(&env, "ipfs://QmStable")),
        funded_amount: 500_0000000_i128,
    };
    check(
        &env,
        "Milestone (with work_evidence)",
        &v,
        "39145ce0f8dd743cc58fb022408f924f3a558271310cf664b74940644b9f1a53",
    );
}

#[test]
fn milestone_no_evidence() {
    let env = env();
    let v = Milestone {
        amount: 100_0000000_i128,
        released: false,
        work_evidence: None,
        funded_amount: 0,
    };
    check(
        &env,
        "Milestone (no work_evidence)",
        &v,
        "dace52150750906765feb9ca8532bb5e07d454add36b074aed32db2e94d075a3",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
//  MilestoneFunding (types.rs)
// ═══════════════════════════════════════════════════════════════════════════
 
#[test]
fn milestone_funding_struct() {
    let env = env();
    let v = MilestoneFunding {
        contract_id: 42,
        milestone_idx: 3,
        funded_amount: 300_0000000_i128,
    };
    check(
        &env,
        "MilestoneFunding",
        &v,
        "a969551851c49ecd12fd2678b58137f3133b823ba680be6836e893f63f49bc1d",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
//  ReadinessChecklist (types.rs)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn readiness_checklist_default() {
    let env = env();
    let v = ReadinessChecklist::default();
    check(
        &env,
        "ReadinessChecklist::default()",
        &v,
        "f281556a100609f8c07ec479c3646f03caaa54b024fa960f58c0e7725f6e73c7",
    );
}
#[test]
fn readiness_checklist_mixed() {
    let env = env();
    let v = ReadinessChecklist {
        initialized: true,
        governed_params_set: false,
        emergency_controls_enabled: true,
    };
    check(
        &env,
        "ReadinessChecklist (mixed)",
        &v,
        "ab56bfe28e6aa9045f7acfafcb44bfc6987ff01046ecd90b7e21b9996f6ad431",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
//  MainnetReadinessInfo (types.rs)
// ═══════════════════════════════════════════════════════════════════════════
 
#[test]
fn mainnet_readiness_info() {
    let env = env();
    let v = MainnetReadinessInfo {
        initialized: true,
        governed_params_set: true,
        emergency_controls_enabled: false,
        caps_set: true,
        protocol_version: 1,
        max_escrow_total_stroops: 1_000_000_000_000_000_i128,
    };
    check(
        &env,
        "MainnetReadinessInfo",
        &v,
        "233b6c467f296493b538c53fcc707d707f9815a2180387e668753a3295df1e5b",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
//  EscrowError (lib.rs) — repr(u32); discriminants are the on-chain values
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn escrow_error_variants() {
    let env = env();
    use EscrowError::*;
    let cases: &[(&str, EscrowError, &str)] = &[
        (
            "EscrowError::InvalidParticipant=1",
            InvalidParticipant,
            "01251abbff7ee711b66504a49053feadef5082d7a905e4b3484c5433eb7da510",
        ),
        (
            "EscrowError::EmptyMilestones=2",
            EmptyMilestones,
            "3501b40b70fb759c42c1ac6ba9f9a31647a4b25a000bab5660b11ef22a22dd3f",
        ),
        (
            "EscrowError::InvalidMilestoneAmount=3",
            InvalidMilestoneAmount,
            "34c803b41d09933e77d4612ac4fb891f08f240c18b4dc11193f68842d6146198",
        ),
        (
            "EscrowError::InvalidDepositAmount=4",
            InvalidDepositAmount,
            "ad584112864055384a2a11a7da56ced74b2d76e1cc89119fad8f5058a507d754",
        ),
        (
            "EscrowError::InvalidMilestone=5",
            InvalidMilestone,
            "84b77c5935909a4c5a5f9d4a896cdfedad97378edd306405cd1760066d5e1bd1",
        ),
        (
            "EscrowError::UnauthorizedRole=6",
            UnauthorizedRole,
            "85f191bef350f6d15c966ee6db97ad270ee71c1b8006210a40284094360f46ff",
        ),
        (
            "EscrowError::InvalidStatusTransition=7",
            InvalidStatusTransition,
            "6b4ddf1a198fa6c94070e5626b9e01c85af330c43cacae8355d8981567bfd03e",
        ),
        (
            "EscrowError::AlreadyCancelled=8",
            AlreadyCancelled,
            "848d4b14a8c9e8156e6141ac23c1edca85a4ddc8dfe2466535a65dee51c5007d",
        ),
        (
            "EscrowError::ContractNotFound=9",
            ContractNotFound,
            "ecd27a459ad9b798819cce1b8bdc802164b8f13d1ca596941bec6338e1cbfd95",
        ),
        (
            "EscrowError::MilestonesAlreadyReleased=10",
            MilestonesAlreadyReleased,
            "252c15deda1112d2ecd776a8708ed12ff7104ebabf50c5d0f8c06e04d555f9de",
        ),
        (
            "EscrowError::TooManyMilestones=11",
            TooManyMilestones,
            "8397c74955c8cb8081cf38328bc930662cd54a8ec78ea69c82b674b0f61dc04b",
        ),
    ];
    for (lbl, v, hash) in cases {
        check(&env, lbl, v, hash);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  EscrowBounds (lib.rs)
// ═══════════════════════════════════════════════════════════════════════════
 
#[test]
fn escrow_bounds_struct() {
    let env = env();
    let v = EscrowBounds {
        max_milestones: 10,
        max_total_escrow_stroops: 1_000_000_0000000_i128,
    };
    check(
        &env,
        "EscrowBounds",
        &v,
        "1315fad799f30c602f2eb91d34f3c3cae8894ebe4ad1f5ad729dd74a6eb6d16b",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
//  ContractData (lib.rs) — primary persistent storage struct
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn contract_data_created_no_arbiter() {
    let env = env();
    let client = addr(&env, 1);
    let freelancer = addr(&env, 2);
    let mut ms = soroban_sdk::Vec::new(&env);
    ms.push_back(200_0000000_i128);
    ms.push_back(400_0000000_i128);
    let v = ContractData {
        client,
        freelancer,
        arbiter: None,
        milestones: ms,
        status: ContractStatus::Created,
        total_deposited: 0,
        released_amount: 0,
    };
    check(
        &env,
        "ContractData (Created, no arbiter)",
        &v,
        "fe80c993ba68afc3e20d141fefa1c645cee8e302cd0a74155c8a013c7903e90b",
    );
}

#[test]
fn contract_data_funded_with_arbiter() {
    let env = env();
    let client = addr(&env, 1);
    let freelancer = addr(&env, 2);
    let arbiter = addr(&env, 3);
    let mut ms = soroban_sdk::Vec::new(&env);
    ms.push_back(300_0000000_i128);
    let v = ContractData {
        client,
        freelancer,
        arbiter: Some(arbiter),
        milestones: ms,
        status: ContractStatus::Funded,
        total_deposited: 300_0000000,
        released_amount: 0,
    };
    check(
        &env,
        "ContractData (Funded, with arbiter)",
        &v,
        "b4df96291651cf55e3aed2af7d8bd28a89e06e0b6028fb2ac9d28f7ede3e4cb3",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
//  PendingApproval (lib.rs)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pending_approval_struct() {
    let env = env();
    let v = PendingApproval {
        approver: addr(&env, 1),
        contract_id: 1,
        requested_at_ledger: 1000,
        expires_at_ledger: 1500,
    };
    check(
        &env,
        "PendingApproval",
        &v,
        "428f5f976d8e7815994b4301e56b8bded701f8ad132afcacc367700bd879e35f",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
//  PendingMigration (lib.rs)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pending_migration_struct() {
    let env = env();
    let v = PendingMigration {
        proposer: addr(&env, 1),
        new_wasm_hash: BytesN::from_array(&env, &[0xAB_u8; 32]),
        requested_at_ledger: 2000,
        expires_at_ledger: 2500,
    };
    check(
        &env,
        "PendingMigration",
        &v,
        "7bd139890f75b46aa62c1b9d8dcd2ae1b09190b9e8834979e3ffbb4f7e093e57",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
//  DataKey — internal storage key enum (lib.rs)
//  Variant order / XDR discriminants are permanently on-chain.
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn data_key_variants() {
    let env = env();
    let cases: &[(&str, DataKey, &str)] = &[
        (
            "DataKey::Contract(0)",
            DataKey::Contract(0),
            "c56651070d5876e55a9044a9d77c71b9fe5801206a855db914321d366a0a5a7e",
        ),
        (
            "DataKey::MilestoneReleased(0,0)",
            DataKey::MilestoneReleased(0, 0),
            "4070d56c4bbbf6c80034bcb0c2c5d596f4683e0902e0b556ea16ff94f7c818e3",
        ),
        (
            "DataKey::RefundableBalance(0)",
            DataKey::RefundableBalance(0),
            "babdba592778c6e8cff80dd99be37ac3b221ce68fd60bf1f8110bf53ec67be8f",
        ),
        (
            "DataKey::ContractCount",
            DataKey::ContractCount,
            "ce4d5d0143b50a11d2da4c00adbbc7583c46e9334c3d89f1447b95f77ed94b58",
        ),
        (
            "DataKey::Milestones(0)",
            DataKey::Milestones(0),
            "856369502fc0609c8e157e8c43b553e7b8811d10af3b4bd1cb4ae3b4232ff47c",
        ),
        (
            "DataKey::MilestoneApprovalTime(0,0)",
            DataKey::MilestoneApprovalTime(0, 0),
            "da13f24288a5fa0030bca6694411a7de5cbfef56e2eda42e25e7cab211f03156",
        ),
    ];
    for (lbl, v, hash) in cases {
        check(&env, lbl, v, hash);
    }
}
