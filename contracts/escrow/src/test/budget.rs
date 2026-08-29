//! Resource-budget assertion tests for the TalentTrust escrow contract.
//!
//! Each test measures the CPU instructions, memory, ledger-entry I/O, and
//! estimated transaction fee for a single contract invocation and asserts that
//! the measurement stays below a hard ceiling.  A test failure means a
//! regression has been introduced; see the inline `NOTE:` comments for known
//! over-budget paths.
//!
//! ## Baseline methodology
//!
//! Ceilings are set by running the suite against the current implementation,
//! recording the actual values, and adding a headroom margin:
//!
//! | Metric          | Headroom |
//! |-----------------|----------|
//! | Instructions    | 3×       |
//! | Memory bytes    | 3×       |
//! | Read entries    | 2×       |
//! | Write entries   | 2×       |
//! | Read bytes      | 4×       |
//! | Write bytes     | 4×       |
//! | Fee (total)     | 3×       |
//!
//! ## Coverage
//!
//! | Entrypoint                     | Typical (3 ms) | Max-load (10 ms) |
//! |-------------------------------|:--------------:|:----------------:|
//! | `create_contract`              |       ✓        |        ✓         |
//! | `deposit_funds`                |       ✓        |        ✓         |
//! | `approve_milestone_release`    |       ✓        |        ✓         |
//! | `release_milestone`            |       ✓        |        ✓         |
//! | `cancel_contract`              |       ✓        |        -         |
//! | `refund_unreleased_milestones` |       ✓        |        ✓         |
//! | `finalize_contract`            |       ✓        |        -         |
//! | `issue_reputation`             |       ✓        |        -         |
//! | `raise_dispute`                |       ✓        |        -         |
//! | `resolve_dispute`              |       ✓        |        -         |

use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, vec, Address, Env, String, Vec,
};

use crate::{ContractStatus, DisputeResolution, Escrow, EscrowClient, ReleaseAuthorization};

// ---------------------------------------------------------------------------
// Resource snapshot and baseline types
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of Soroban resource consumption.
#[derive(Clone, Copy, Debug)]
struct Resources {
    instructions: i64,
    mem_bytes: i64,
    read_entries: u32,
    write_entries: u32,
    read_bytes: u32,
    write_bytes: u32,
    fee_total: i64,
}

/// Hard ceilings for a single invocation.  All values are upper bounds;
/// exceeding any one trips a regression assertion.
#[derive(Clone, Copy, Debug)]
struct Ceiling {
    instructions: i64,
    mem_bytes: i64,
    read_entries: u32,
    write_entries: u32,
    read_bytes: u32,
    write_bytes: u32,
    fee_total: i64,
}

// ---------------------------------------------------------------------------
// Per-entrypoint ceilings  (3-milestone typical path)
// ---------------------------------------------------------------------------

const CREATE_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 9,
    read_bytes: 24_576,
    write_bytes: 49_152,
    fee_total: 6_000_000,
};

const DEPOSIT_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 6,
    read_bytes: 24_576,
    write_bytes: 32_768,
    fee_total: 6_000_000,
};

const APPROVE_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 6,
    read_bytes: 24_576,
    write_bytes: 32_768,
    fee_total: 6_000_000,
};

const RELEASE_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 9,
    read_bytes: 24_576,
    write_bytes: 49_152,
    fee_total: 6_000_000,
};

const CANCEL_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 6,
    read_bytes: 24_576,
    write_bytes: 32_768,
    fee_total: 6_000_000,
};

const REFUND_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 9,
    read_bytes: 24_576,
    write_bytes: 49_152,
    fee_total: 6_000_000,
};

const FINALIZE_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 9,
    read_bytes: 24_576,
    write_bytes: 49_152,
    fee_total: 6_000_000,
};

const REPUTATION_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 9,
    read_bytes: 24_576,
    write_bytes: 49_152,
    fee_total: 6_000_000,
};

const RAISE_DISPUTE_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 6,
    read_bytes: 24_576,
    write_bytes: 32_768,
    fee_total: 6_000_000,
};

const RESOLVE_DISPUTE_3MS: Ceiling = Ceiling {
    instructions: 30_000_000,
    mem_bytes: 3_000_000,
    read_entries: 12,
    write_entries: 9,
    read_bytes: 24_576,
    write_bytes: 49_152,
    fee_total: 6_000_000,
};

// ---------------------------------------------------------------------------
// Per-entrypoint ceilings  (10-milestone max-load path)
//
// Larger state means more read/write bytes; instruction counts grow only
// modestly because milestone iteration is O(n) over a small n.
// ---------------------------------------------------------------------------

const CREATE_10MS: Ceiling = Ceiling {
    instructions: 45_000_000,
    mem_bytes: 5_000_000,
    read_entries: 16,
    write_entries: 12,
    read_bytes: 40_960,
    write_bytes: 81_920,
    fee_total: 9_000_000,
};

const DEPOSIT_10MS: Ceiling = Ceiling {
    instructions: 45_000_000,
    mem_bytes: 5_000_000,
    read_entries: 16,
    write_entries: 8,
    read_bytes: 40_960,
    write_bytes: 65_536,
    fee_total: 9_000_000,
};

const APPROVE_10MS: Ceiling = Ceiling {
    instructions: 45_000_000,
    mem_bytes: 5_000_000,
    read_entries: 16,
    write_entries: 8,
    read_bytes: 40_960,
    write_bytes: 65_536,
    fee_total: 9_000_000,
};

const RELEASE_10MS: Ceiling = Ceiling {
    instructions: 45_000_000,
    mem_bytes: 5_000_000,
    read_entries: 16,
    write_entries: 12,
    read_bytes: 40_960,
    write_bytes: 81_920,
    fee_total: 9_000_000,
};

const REFUND_10MS: Ceiling = Ceiling {
    instructions: 45_000_000,
    mem_bytes: 5_000_000,
    read_entries: 16,
    write_entries: 12,
    read_bytes: 40_960,
    write_bytes: 81_920,
    fee_total: 9_000_000,
};

// ---------------------------------------------------------------------------
// Measurement helper
// ---------------------------------------------------------------------------

fn measure(env: &Env) -> Resources {
    let r = env.cost_estimate().resources();
    let f = env.cost_estimate().fee();
    Resources {
        instructions: r.instructions,
        mem_bytes: r.mem_bytes,
        read_entries: r.read_entries,
        write_entries: r.write_entries,
        read_bytes: r.read_bytes,
        write_bytes: r.write_bytes,
        fee_total: f.total,
    }
}

/// Assert that every resource dimension of `got` is within `ceiling`.
/// The `label` is included in every panic message so regressions are
/// immediately identifiable in CI output.
fn assert_within(label: &str, got: Resources, ceiling: Ceiling) {
    assert!(
        got.instructions <= ceiling.instructions,
        "[budget] {} instruction regression: got {} > ceiling {}",
        label,
        got.instructions,
        ceiling.instructions
    );
    assert!(
        got.mem_bytes <= ceiling.mem_bytes,
        "[budget] {} memory regression: got {} > ceiling {}",
        label,
        got.mem_bytes,
        ceiling.mem_bytes
    );
    assert!(
        got.read_entries <= ceiling.read_entries,
        "[budget] {} read-entry regression: got {} > ceiling {}",
        label,
        got.read_entries,
        ceiling.read_entries
    );
    assert!(
        got.write_entries <= ceiling.write_entries,
        "[budget] {} write-entry regression: got {} > ceiling {}",
        label,
        got.write_entries,
        ceiling.write_entries
    );
    assert!(
        got.read_bytes <= ceiling.read_bytes,
        "[budget] {} read-byte regression: got {} > ceiling {}",
        label,
        got.read_bytes,
        ceiling.read_bytes
    );
    assert!(
        got.write_bytes <= ceiling.write_bytes,
        "[budget] {} write-byte regression: got {} > ceiling {}",
        label,
        got.write_bytes,
        ceiling.write_bytes
    );
    assert!(
        got.fee_total <= ceiling.fee_total,
        "[budget] {} fee regression: got {} > ceiling {}",
        label,
        got.fee_total,
        ceiling.fee_total
    );
}

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

/// Returns `n` equal milestone amounts that sum to exactly `n * 100_0000000`.
fn milestones_n(env: &Env, n: u32) -> Vec<i128> {
    let mut v: Vec<i128> = Vec::new(env);
    for _ in 0..n {
        v.push_back(100_0000000_i128);
    }
    v
}

/// Total stroop value of `n` equal milestones.
fn total_n(n: u32) -> i128 {
    (n as i128) * 100_0000000_i128
}

/// A short comment satisfying the 1–200 char constraint.
fn comment(env: &Env) -> String {
    String::from_str(env, "Budget test: good work.")
}

/// Builds a fresh, initialized escrow with a bound SAC settlement token.
/// Returns `(client, admin, token_address)`.
fn make_escrow(env: &Env) -> (EscrowClient<'_>, Address, Address) {
    let escrow_addr = env.register(Escrow, ());
    let escrow = EscrowClient::new(env, &escrow_addr);
    let admin = Address::generate(env);
    escrow.initialize(&admin);

    let token = env.register_stellar_asset_contract(admin.clone());
    escrow.bind_settlement_token(&admin, &token);

    (escrow, admin, token)
}

/// Mint `amount` tokens from `token` to `recipient`.
fn mint(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    // The SAC admin is whichever address registered the asset contract.
    // We use mock_all_auths so no explicit signer is required.
    StellarAssetClient::new(env, token).mint(recipient, &amount);
}

// ---------------------------------------------------------------------------
// TYPICAL PATH: 3-milestone contracts
// ---------------------------------------------------------------------------

/// Budget: `create_contract` with 3 milestones.
#[test]
fn budget_create_contract_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, _token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );

    assert_within("create_contract/3ms", measure(&env), CREATE_3MS);
}

/// Budget: `deposit_funds` with 3 milestones (SAC transfer included).
#[test]
fn budget_deposit_funds_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );

    let total = total_n(3);
    mint(&env, &token, &client_addr, total);

    escrow.deposit_funds(&id, &client_addr, &total);

    assert_within("deposit_funds/3ms", measure(&env), DEPOSIT_3MS);
}

/// Budget: `approve_milestone_release` for milestone 0 on a funded 3-ms contract.
#[test]
fn budget_approve_milestone_release_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(3));
    escrow.deposit_funds(&id, &client_addr, &total_n(3));

    escrow.approve_milestone_release(&id, &client_addr, &0);

    assert_within("approve_milestone_release/3ms", measure(&env), APPROVE_3MS);
}

/// Budget: `release_milestone` for milestone 0 on a funded 3-ms contract
/// (SAC transfer to freelancer included).
#[test]
fn budget_release_milestone_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(3));
    escrow.deposit_funds(&id, &client_addr, &total_n(3));
    escrow.approve_milestone_release(&id, &client_addr, &0);

    escrow.release_milestone(&id, &client_addr, &0);

    assert_within("release_milestone/3ms", measure(&env), RELEASE_3MS);
}

/// Budget: `cancel_contract` on a freshly-created (unfunded) 3-ms contract.
#[test]
fn budget_cancel_contract_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, _token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );

    escrow.cancel_contract(&id, &client_addr);

    assert_within("cancel_contract/3ms", measure(&env), CANCEL_3MS);
}

/// Budget: `refund_unreleased_milestones` – refund all 3 milestones at once
/// on a fully-funded contract.
#[test]
fn budget_refund_unreleased_milestones_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(3));
    escrow.deposit_funds(&id, &client_addr, &total_n(3));

    escrow.refund_unreleased_milestones(&id, &vec![&env, 0_u32, 1, 2]);

    assert_within(
        "refund_unreleased_milestones/3ms",
        measure(&env),
        REFUND_3MS,
    );
}

/// Budget: `finalize_contract` after all milestones have been released
/// (contract status = Completed).
#[test]
fn budget_finalize_contract_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(3));
    escrow.deposit_funds(&id, &client_addr, &total_n(3));
    for ms in 0..3_u32 {
        escrow.approve_milestone_release(&id, &client_addr, &ms);
        escrow.release_milestone(&id, &client_addr, &ms);
    }
    assert_eq!(escrow.get_contract(&id).status, ContractStatus::Completed);

    escrow.finalize_contract(&id, &client_addr);

    assert_within("finalize_contract/3ms", measure(&env), FINALIZE_3MS);
}

/// Budget: `issue_reputation` after a completed 3-ms contract.
#[test]
fn budget_issue_reputation_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(3));
    escrow.deposit_funds(&id, &client_addr, &total_n(3));
    for ms in 0..3_u32 {
        escrow.approve_milestone_release(&id, &client_addr, &ms);
        escrow.release_milestone(&id, &client_addr, &ms);
    }

    escrow.issue_reputation(&id, &client_addr, &5, &comment(&env));

    assert_within("issue_reputation/3ms", measure(&env), REPUTATION_3MS);
}

/// Budget: `raise_dispute` on a funded 3-ms contract with arbiter.
#[test]
fn budget_raise_dispute_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(3));
    escrow.deposit_funds(&id, &client_addr, &total_n(3));

    escrow.raise_dispute(&id, &client_addr);

    assert_within("raise_dispute/3ms", measure(&env), RAISE_DISPUTE_3MS);
}

/// Budget: `resolve_dispute` (FullRefund path) on a 3-ms contract.
#[test]
fn budget_resolve_dispute_3ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);
    let arbiter_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &Some(arbiter_addr.clone()),
        &milestones_n(&env, 3),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(3));
    escrow.deposit_funds(&id, &client_addr, &total_n(3));
    escrow.raise_dispute(&id, &client_addr);

    escrow.resolve_dispute(&id, &arbiter_addr, &DisputeResolution::FullRefund);

    assert_within("resolve_dispute/3ms", measure(&env), RESOLVE_DISPUTE_3MS);
}

// ---------------------------------------------------------------------------
// MAX-LOAD PATH: 10-milestone contracts (upper bound on input size)
//
// MAX_MILESTONES == 10 per the protocol constants.  These tests confirm that
// the worst-case input stays within the enlarged ceilings above and that no
// entrypoint has super-linear cost growth that would blow through the budget.
// ---------------------------------------------------------------------------

/// Budget: `create_contract` with maximum (10) milestones.
#[test]
fn budget_create_contract_10ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, _token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 10),
        &ReleaseAuthorization::ClientOnly,
    );

    assert_within("create_contract/10ms", measure(&env), CREATE_10MS);
}

/// Budget: `deposit_funds` – full deposit against a 10-milestone contract.
#[test]
fn budget_deposit_funds_10ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 10),
        &ReleaseAuthorization::ClientOnly,
    );
    let total = total_n(10);
    mint(&env, &token, &client_addr, total);

    escrow.deposit_funds(&id, &client_addr, &total);

    assert_within("deposit_funds/10ms", measure(&env), DEPOSIT_10MS);
}

/// Budget: `approve_milestone_release` for milestone 0 on a 10-ms contract.
#[test]
fn budget_approve_milestone_release_10ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 10),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(10));
    escrow.deposit_funds(&id, &client_addr, &total_n(10));

    escrow.approve_milestone_release(&id, &client_addr, &0);

    assert_within(
        "approve_milestone_release/10ms",
        measure(&env),
        APPROVE_10MS,
    );
}

/// Budget: `release_milestone` for milestone 0 on a 10-ms funded contract.
#[test]
fn budget_release_milestone_10ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 10),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(10));
    escrow.deposit_funds(&id, &client_addr, &total_n(10));
    escrow.approve_milestone_release(&id, &client_addr, &0);

    escrow.release_milestone(&id, &client_addr, &0);

    assert_within("release_milestone/10ms", measure(&env), RELEASE_10MS);
}

/// Budget: `refund_unreleased_milestones` – refund all 10 milestones at once.
///
/// This is the heaviest refund path: a single call touches all 10 milestone
/// slots.  The ceiling accounts for the extra write bytes.
#[test]
fn budget_refund_unreleased_milestones_10ms() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let (escrow, _admin, token) = make_escrow(&env);
    let client_addr = Address::generate(&env);
    let freelancer_addr = Address::generate(&env);

    let id = escrow.create_contract(
        &client_addr,
        &freelancer_addr,
        &None,
        &milestones_n(&env, 10),
        &ReleaseAuthorization::ClientOnly,
    );
    mint(&env, &token, &client_addr, total_n(10));
    escrow.deposit_funds(&id, &client_addr, &total_n(10));

    let indices = vec![&env, 0_u32, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    escrow.refund_unreleased_milestones(&id, &indices);

    assert_within(
        "refund_unreleased_milestones/10ms",
        measure(&env),
        REFUND_10MS,
    );
}

// ---------------------------------------------------------------------------
// REGRESSION DOCUMENTATION
//
// NOTE: the following paths are known to be heavier than the 3-ms typical
// path.  They are intentionally covered by the 10-ms max-load tests above
// with enlarged ceilings.
//
//   • refund_unreleased_milestones with 10 indices does O(n²) duplicate
//     detection; at n=10 this is 45 comparisons and stays within budget.
//     If MAX_MILESTONES ever increases, revisit the REFUND_10MS ceiling.
//
//   • release_milestone when it triggers the ContractStatus::Completed
//     transition writes an extra event.  The last-milestone release is
//     therefore slightly heavier than earlier releases; RELEASE_3MS and
//     RELEASE_10MS cover the first-milestone case (cheapest).
// ---------------------------------------------------------------------------
