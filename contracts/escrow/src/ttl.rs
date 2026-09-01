//! Deterministic TTL / expiration policy for transient and persistent storage.
//!
//! This module defines all time‑to‑live (TTL) constants used by the escrow contract and provides
//! helper utilities for storing, reading and extending entries. The constants are expressed in
//! **ledger counts** – on Stellar mainnet a ledger is ~5 seconds. For readability we also expose the
//! equivalent number of days.
//!
//! | Constant                              | Ledger count | Days (≈) | Governs
//! |--------------------------------------|--------------|----------|------------------------------------------------------------
//! | `LEDGERS_PER_DAY`                    | 17_280       | 1        | conversion factor
//! | `PENDING_APPROVAL_TTL_LEDGERS`       | 120_960      | 7        | transient approvals stored in `temporary()`
//! | `PENDING_MIGRATION_TTL_LEDGERS`      | 362_880      | 21       | transient migration requests in `temporary()`
//! | `PERSISTENT_TTL_LEDGERS`             | 518_400      | 30       | persistent contract data stored in `persistent()`
//! | `PENDING_APPROVAL_BUMP_THRESHOLD`    | 17_280       | 1        | when a read occurs within this many ledgers of expiry, its TTL is bumped
//! | `PENDING_MIGRATION_BUMP_THRESHOLD`   | 51_840       | 3        | same, but for migrations
//! | `PERSISTENT_BUMP_THRESHOLD`          | 120_960      | 7        | bump threshold for persistent entries
//! | `ADMIN_ROTATION_MIN_DELAY_LEDGERS`   | 34_560       | 2        | minimum delay before a pending admin proposal can be accepted
//! | `ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS`| 155_520      | 9        | total lifetime of a pending admin proposal before it expires
//!
//! **Bump‑on‑read strategy** – The `extend_if_below_threshold` helper is used by entry‑point
//! implementations to extend the TTL of a transient entry when it is accessed and the remaining
//! lifetime falls below the corresponding *bump threshold*. This ensures that active approvals or
//! migrations survive a series of reads without being evicted, while still allowing them to expire
//! if they become stale.
//!
//! **Eviction risk** – If a contract (or its milestone vector) is never accessed for more than
//! `PERSISTENT_TTL_LEDGERS` (30 days) the Soroban host will evict the persistent storage entry. The
//! contract then becomes inaccessible; any subsequent reads will return `None`. This is a deliberate
//! safety measure – stale contracts are archived automatically.
//!
//! **`read_if_live` semantics** – The `read_if_live` helper reads from `temporary()` storage and
//! returns `None` for two distinct cases:
//!   1. The key was never set ("absent").
//!   2. The key was set but its TTL has expired and the entry was evicted.
//! This "fail‑closed" behaviour is important for approvals and migrations: a missing entry is
//! interpreted as not approved/not migrated, preventing any stale permission from being honored.
//!
//! Storage ownership: this module owns TTL policy and helper access patterns,
//! not business records. It extends caller-provided keys, with first-class
//! helpers for `DataKey::Contract(contract_id)`, the paired milestone vector
//! key `(DataKey::Contract(contract_id), "milestones")`, `NextContractId`,
//! participant index keys, pending approvals, and pending migrations.
//!
use crate::{types::Error, ContractStatus, DataKey, Milestone};
use soroban_sdk::{Env, IntoVal, Symbol, TryFromVal, Val, Vec};

pub const LEDGERS_PER_DAY: u32 = 17_280;

pub const PENDING_APPROVAL_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 7;
pub const PENDING_APPROVAL_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY;
pub const MIN_APPROVAL_TTL: u32 = 17_280;

/// Minimum ledgers that must elapse between proposing and finalising an
/// admin rotation. At ~5 s per ledger this is roughly 2 days, giving
/// stakeholders time to react to an unexpected proposal.
pub const ADMIN_ROTATION_MIN_DELAY_LEDGERS: u32 = LEDGERS_PER_DAY * 2;

/// Total ledgers a pending admin proposal remains acceptable, measured from
/// the ledger it was proposed on. Once this elapses `accept_admin` fails with
/// `Error::AdminProposalExpired` and the stale proposal is cleared, forcing a
/// fresh `propose_admin` call. This bounds the window during which a
/// forgotten or unaddressed proposal (e.g. from a since-remediated key
/// compromise) can still be accepted.
pub const ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 9;

pub const PENDING_MIGRATION_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 21;
pub const PENDING_MIGRATION_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 3;

/// Persistent storage TTL: extend to 30 days, renew when below 7 days.
pub const PERSISTENT_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 30;
pub const PERSISTENT_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;

// ── State-specific contract TTL thresholds ────────────────────────────────────
//
// Contract persistent entries use different TTL durations depending on the
// lifecycle state of the escrow. The goal is to keep live obligations alive
// for longer than any realistic workflow cycle, while letting closed records
// expire more quickly to free ledger space.
//
// All thresholds are expressed as ledger counts. At ~5 s/ledger on mainnet:
//   LEDGERS_PER_DAY = 17_280   (1 day)
//
// Security note: TTL is extended only on **meaningful writes** (state
// transitions and value-moving operations). Read-only entrypoints do not
// bump TTL so that an attacker cannot extend contract lifetime indefinitely
// by replaying read calls.

/// TTL for contracts in an active but non-disputed state:
/// `Created`, `PartiallyFunded`, `Funded`, `Accepted`.
///
/// 60 days covers the longest realistic funding-to-completion cycle and
/// ensures live obligations cannot disappear under normal network load.
pub const ACTIVE_CONTRACT_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 60;

/// Bump threshold for active contracts: extend when ≤ 15 days remain.
///
/// This gives a generous window before the entry would be evicted,
/// ensuring any mutating operation (deposit, release, refund) renews
/// the TTL with plenty of margin.
pub const ACTIVE_CONTRACT_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 15;

/// TTL for contracts in `Disputed` state.
///
/// 75 days is longer than active contracts because arbiter availability
/// and on-chain dispute resolution can take more time than normal operations.
/// The extra buffer prevents live disputes from expiring while awaiting
/// arbiter action.
pub const DISPUTED_CONTRACT_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 75;

/// Bump threshold for disputed contracts: extend when ≤ 20 days remain.
pub const DISPUTED_CONTRACT_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 20;

/// TTL for contracts in a closed (terminal) state:
/// `Completed`, `Cancelled`, `Refunded`.
///
/// 30 days retains closed records long enough for reputation issuance,
/// indexer queries, and audit, then allows them to expire naturally.
/// This is identical to the legacy flat `PERSISTENT_TTL_LEDGERS` so
/// existing closed contracts see no TTL reduction.
pub const CLOSED_CONTRACT_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 30;

/// Bump threshold for closed contracts: extend when ≤ 7 days remain.
pub const CLOSED_CONTRACT_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;
// ── Two-step governance proposal TTLs (#1221) ────────────────────────────────

/// Maximum ledgers a governance override proposal remains actionable.
///
/// A proposal that has not been approved and applied within this window (≈3 days
/// at 5 s/ledger) expires and can no longer be approved or applied.  The
/// requirement for a short window limits the time during which a pending
/// (but forgotten or compromised) proposal could be weaponised.
pub const GOVERNANCE_PROPOSAL_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 3;

/// Bump threshold for governance proposal persistent entries.
/// When a read access occurs within this many ledgers of expiry the TTL is renewed.
pub const GOVERNANCE_PROPOSAL_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY;

#[allow(dead_code)]
pub fn compute_expiry(env: &Env, ttl_ledgers: u32) -> u32 {
    env.ledger().sequence().saturating_add(ttl_ledgers)
}

#[allow(dead_code)]
pub fn store_with_ttl<K, V>(env: &Env, key: &K, value: &V, ttl_ledgers: u32)
where
    K: IntoVal<Env, Val>,
    V: IntoVal<Env, Val>,
{
    let storage = env.storage().temporary();
    storage.set(key, value);
    storage.extend_ttl(key, ttl_ledgers, ttl_ledgers);
}

#[allow(dead_code)]
pub fn read_if_live<K, V>(env: &Env, key: &K) -> Option<V>
where
    K: IntoVal<Env, Val>,
    V: TryFromVal<Env, Val>,
{
    env.storage().temporary().get(key)
}

/// Extends a live transient entry only when its remaining TTL is below `threshold`.
///
/// Returns `false` when `key` is absent or has already been evicted. Returns
/// `true` when the key is live; in that case Soroban performs the extension only
/// when the remaining TTL is below `threshold` and otherwise leaves the TTL
/// unchanged.
///
/// The boolean reports liveness, not whether Soroban changed the TTL. The host
/// intentionally does not expose a production API for observing an entry's TTL.
#[allow(dead_code)]
pub fn extend_if_below_threshold<K>(env: &Env, key: &K, threshold: u32, extend_to: u32) -> bool
where
    K: IntoVal<Env, Val>,
{
    let storage = env.storage().temporary();
    if !storage.has(key) {
        return false;
    }
    storage.extend_ttl(key, threshold, extend_to);
    true
}

/// Removes a transient entry if it exists.
///
/// This operation is idempotent: removing an absent or evicted key is a no-op.
#[allow(dead_code)]
pub fn remove_transient<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val>,
{
    env.storage().temporary().remove(key);
}

/// Returns whether a transient key is currently live in contract storage.
///
/// Expired temporary entries are auto-evicted by Soroban and therefore return
/// `false`, just like keys that were never stored.
#[allow(dead_code)]
pub fn has_transient<K>(env: &Env, key: &K) -> bool
where
    K: IntoVal<Env, Val>,
{
    env.storage().temporary().has(key)
}

/// Loads the milestone vector for a contract and extends its TTL.
pub fn load_milestones(env: &Env, contract_id: u32) -> Vec<Milestone> {
    let key = milestone_storage_key(env, contract_id);
    let milestones: Vec<Milestone> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
    extend_milestone_ttl(env, contract_id);
    milestones
}

/// Stores the milestone vector for a contract and extends its TTL.
pub fn store_milestones(env: &Env, contract_id: u32, milestones: &Vec<Milestone>) {
    let key = milestone_storage_key(env, contract_id);
    env.storage().persistent().set(&key, milestones);
    extend_milestone_ttl(env, contract_id);
}

pub(crate) fn milestone_storage_key(env: &Env, contract_id: u32) -> (DataKey, Symbol) {
    crate::keys::milestone_key(env, contract_id)
}

/// Extend TTL of the NextContractId counter.
pub fn extend_next_contract_id_ttl(env: &Env) {
    if env.storage().persistent().has(&DataKey::NextContractId) {
        env.storage().persistent().extend_ttl(
            &DataKey::NextContractId,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_TTL_LEDGERS,
        );
    }
}

/// Extend TTL of a single contract entry.
pub fn extend_contract_ttl(env: &Env, contract_id: u32) {
    env.storage().persistent().extend_ttl(
        &DataKey::Contract(contract_id),
        PERSISTENT_BUMP_THRESHOLD,
        PERSISTENT_TTL_LEDGERS,
    );
}

/// Extend TTL of the milestones vector for a given contract.
pub fn extend_milestone_ttl(env: &Env, contract_id: u32) {
    env.storage().persistent().extend_ttl(
        &milestone_storage_key(env, contract_id),
        PERSISTENT_BUMP_THRESHOLD,
        PERSISTENT_TTL_LEDGERS,
    );
}

/// Extend TTL of both the contract and its milestones vector.
pub fn extend_contract_and_milestones_ttl(env: &Env, contract_id: u32) {
    extend_contract_ttl(env, contract_id);
    extend_milestone_ttl(env, contract_id);
}

// ── State-aware TTL extension ─────────────────────────────────────────────────

/// Return the `(bump_threshold, extend_to)` ledger pair for a given contract status.
///
/// This is the single source of truth for state-specific TTL policy. All
/// callers should use [`extend_contract_ttl_for_status`] rather than calling
/// `env.storage().persistent().extend_ttl(...)` directly with hard-coded values.
///
/// | Status                                      | TTL (ledgers) | Days |
/// |---------------------------------------------|---------------|------|
/// | Created / Accepted / PartiallyFunded / Funded | 1_036_800   | 60   |
/// | Disputed                                    | 1_296_000     | 75   |
/// | Completed / Cancelled / Refunded            | 518_400       | 30   |
pub fn ttl_for_status(status: ContractStatus) -> (u32, u32) {
    match status {
        ContractStatus::Disputed => (DISPUTED_CONTRACT_BUMP_THRESHOLD, DISPUTED_CONTRACT_TTL_LEDGERS),
        ContractStatus::Completed | ContractStatus::Cancelled | ContractStatus::Refunded => {
            (CLOSED_CONTRACT_BUMP_THRESHOLD, CLOSED_CONTRACT_TTL_LEDGERS)
        }
        // Created, Accepted, PartiallyFunded, Funded
        _ => (ACTIVE_CONTRACT_BUMP_THRESHOLD, ACTIVE_CONTRACT_TTL_LEDGERS),
    }
}

/// Extend the TTL of a contract entry using the **state-appropriate** threshold.
///
/// This is the canonical call-site for TTL extension on meaningful writes.
/// It replaces ad-hoc `extend_contract_ttl` calls at state-transition points
/// so that:
///
/// - Active obligations receive a 60-day window.
/// - Disputed records receive a 75-day window (arbitration buffer).
/// - Closed records receive a 30-day window (audit / reputation queries).
///
/// # Why bump only on writes?
///
/// Extending TTL on reads would allow an attacker to keep a dormant contract
/// alive indefinitely by polling it. Restricting bumps to meaningful writes
/// (deposits, releases, refunds, state transitions) makes TTL deterministic
/// and proportional to actual contract activity.
pub fn extend_contract_ttl_for_status(env: &Env, contract_id: u32, status: ContractStatus) {
    let (threshold, extend_to) = ttl_for_status(status);
    env.storage().persistent().extend_ttl(
        &DataKey::Contract(contract_id),
        threshold,
        extend_to,
    );
}

/// Extend TTL of the milestones vector using the state-appropriate threshold.
///
/// The milestones vector shares the same lifecycle as its parent contract, so
/// it uses the same state-specific TTL policy.
pub fn extend_milestone_ttl_for_status(env: &Env, contract_id: u32, status: ContractStatus) {
    let (threshold, extend_to) = ttl_for_status(status);
    env.storage().persistent().extend_ttl(
        &milestone_storage_key(env, contract_id),
        threshold,
        extend_to,
    );
}

/// Extend TTL of both the contract and its milestones using the state-appropriate threshold.
///
/// Convenience wrapper for the common pattern of bumping both entries at
/// state-transition points such as `deposit_funds`, `release_milestone`,
/// `cancel_contract`, and `resolve_dispute`.
pub fn extend_contract_and_milestones_ttl_for_status(
    env: &Env,
    contract_id: u32,
    status: ContractStatus,
) {
    extend_contract_ttl_for_status(env, contract_id, status);
    extend_milestone_ttl_for_status(env, contract_id, status);
}

/// Extend TTL for a participant contract index entry (e.g. client or freelancer id list).
pub fn extend_participant_contract_index_ttl(env: &Env, key: &crate::DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS);
}

/// Extend TTL for the governed parameters persistent storage entry.
pub fn extend_governed_parameters_ttl(env: &Env) {
    if env.storage().persistent().has(&DataKey::GovernedParameters) {
        env.storage().persistent().extend_ttl(
            &DataKey::GovernedParameters,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_TTL_LEDGERS,
        );
    }
}

/// Set the initial TTL for a newly created governance proposal entry.
///
/// Uses `GOVERNANCE_PROPOSAL_TTL_LEDGERS` so the entry is automatically
/// evicted after ~3 days if not explicitly removed first.
pub fn set_governance_proposal_ttl(env: &Env, proposal_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::GovernanceProposal(proposal_id),
        GOVERNANCE_PROPOSAL_BUMP_THRESHOLD,
        GOVERNANCE_PROPOSAL_TTL_LEDGERS,
    );
}
