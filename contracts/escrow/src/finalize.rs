//! Immutable close records for finished escrow contracts.
//!
//! # Finalization invariants
//!
//! Finalization is the terminal, one-shot state transition of a contract. The
//! following invariants hold for every execution of
//! [`finalize_contract_impl`] and are relied upon by every other mutating
//! entrypoint (they all guard through [`Escrow::require_not_finalized`]):
//!
//! 1. **Write-once** — at most one [`FinalizationRecord`] exists per
//!    `contract_id`. The record is written through
//!    [`crate::settlement::write_finalization`], which refuses to overwrite an
//!    existing record, and the duplicate-work guard runs before any mutation.
//! 2. **First writer wins / deterministic losers** — of two racing or repeated
//!    calls for the same contract, exactly one succeeds. Every other attempt
//!    panics with [`Error::AlreadyFinalized`] *before* touching state, so it
//!    publishes no event, clears no rollback record and cannot be partially
//!    applied. Retrying is therefore safe and idempotent: the observable state
//!    after N attempts equals the state after one.
//! 3. **Durable guard** — the record's TTL is set to
//!    [`crate::ttl::PERSISTENT_TTL_LEDGERS`] when written, refreshed whenever
//!    the guard is consulted ([`Escrow::is_finalized`]) and refreshed whenever
//!    the contract entry it guards is extended (see
//!    [`crate::ttl::extend_contract_ttl`]). The record can therefore never be
//!    evicted while the contract entry is still live, which keeps
//!    `AlreadyFinalized` and `get_finalization_record` consistent for the whole
//!    life of the contract.
//! 4. **No reentrancy surface** — this path performs no token transfer and no
//!    cross-contract call, so there is no external code that could re-enter the
//!    escrow between the guard check and the record write. The record write is
//!    the terminal claim and every other effect follows it.
//! 5. **Freeze before mutation** — the pause/emergency gate runs before any
//!    state is written, so a paused or emergency-frozen escrow cannot be
//!    closed. Authorization runs before the finalizer role is evaluated.
//! 6. **Checked accounting** — the summary snapshot uses checked arithmetic, so
//!    a contract whose `funded - released - refunded` accounting is inconsistent
//!    fails loudly with [`Error::AccountingInvariantViolated`] instead of
//!    writing a silently wrong `refundable_balance`.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

use crate::{
    settlement,
    ttl::{self, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS},
    Contract, ContractStatus, ContractSummary, DataKey, Error, Escrow, EscrowError, Milestone,
    MilestoneSummary, CONTRACT_SUMMARY_SCHEMA_VERSION,
};

/// Immutable metadata written when an escrow contract is closed.
///
/// The record is stored once under `DataKey::Finalization(contract_id)`.
/// After it exists, all contract-specific mutating entrypoints reject with
/// `Error::AlreadyFinalized`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizationRecord {
    /// Authorized client, freelancer, or assigned arbiter that finalized.
    pub finalizer: Address,
    /// Ledger timestamp at finalization time.
    pub timestamp: u64,
    /// Snapshot of participant, milestone, and accounting state.
    pub summary: ContractSummary,
}

/// Statuses a contract may hold when it is sealed.
///
/// `Completed` and `Disputed` are the only two states in which every milestone
/// has already been dispositioned, so a close summary is meaningful.
fn is_sealable_status(status: ContractStatus) -> bool {
    status == ContractStatus::Completed || status == ContractStatus::Disputed
}

impl Escrow {
    fn finalization_key(contract_id: u32) -> DataKey {
        DataKey::Finalization(contract_id)
    }

    /// Load the contract record for a finalization attempt and refresh its TTL.
    ///
    /// The bump is unconditional: an entry that is still live is renewed
    /// according to the standard policy, and one that has already been evicted
    /// is left evicted.  This keeps a contract that is being closed from
    /// lapsing while the (longer) summary computation runs.
    fn load_contract_for_finalization(env: &Env, contract_id: u32) -> Contract {
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        ttl::extend_contract_ttl(env, contract_id);
        contract
    }

    /// Load a contract for finalization without extending TTL. Finalization
    /// is a terminal transition; the record itself is what must outlive the
    /// contract, so we do not refresh the contract TTL here.
    fn load_contract_for_finalization_checked(
        env: &Env,
        contract_id: u32,
    ) -> Contract {
        Self::load_contract_for_finalization(env, contract_id)
    }

    /// Returns `true` when a finalization record already exists.
    ///
    /// Reading the guard refreshes the record's TTL so that observing a
    /// finalized contract also renews the guarantee that stays finalized. This
    /// is a bump-on-read strategy; it never creates or mutates the record
    /// itself.
    pub(crate) fn is_finalized(env: &Env, contract_id: u32) -> bool {
        let finalized = env
            .storage()
            .persistent()
            .has(&Self::finalization_key(contract_id));
        if finalized {
            crate::ttl::extend_finalization_ttl(env, contract_id);
        }
        finalized
    }

    pub(crate) fn require_not_finalized(env: &Env, contract_id: u32) {
        settlement::require_not_finalized(env, contract_id);
    }

    /// Returns true when the contract status is a terminal, non-mutable
    /// state that must never be resurrected by lifecycle entrypoints.
    pub(crate) fn is_terminal_status(status: ContractStatus) -> bool {
        matches!(
            status,
            ContractStatus::Cancelled | ContractStatus::Refunded
        )
    }

    /// Load a contract, verify it's in an active (mutable) state, and extend
    /// its TTL. Rejects `Cancelled`, `Refunded`, and finalized contracts.
    ///
    /// This is the canonical preamble for all lifecycle entrypoints that need a
    /// live, mutable contract. Calls `load_contract` from `storage.rs`, extends
    /// the TTL, checks finalization, and rejects terminal statuses.
    ///
    /// # Panics
    /// - `ContractNotFound` when `contract_id` is unknown.
    /// - `AlreadyFinalized` when the contract has been finalized.
    /// - `InvalidState` when the contract status is `Cancelled` or `Refunded`.
    ///
    /// # Returns
    /// The loaded `Contract`.
    pub(crate) fn require_active_contract(env: &Env, contract_id: u32) -> Contract {
        let contract = crate::storage::load_contract(env, contract_id);
        ttl::extend_contract_ttl(env, contract_id);
        Self::require_not_finalized(env, contract_id);
        if Self::is_terminal_status(contract.status) {
            env.panic_with_error(Error::InvalidState);
        }
        contract
    }

    pub(crate) fn require_not_paused(env: &Env) {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::ContractPaused);
        }
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Emergency)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::EmergencyActive);
        }
    }

    fn require_finalizer_role(env: &Env, contract: &Contract, finalizer: &Address) {
        let is_client = *finalizer == contract.client;
        let is_freelancer = *finalizer == contract.freelancer;
        let is_arbiter = contract.arbiter.clone().is_some_and(|a| a == *finalizer);
        if !is_client && !is_freelancer && !is_arbiter {
            env.panic_with_error(Error::UnauthorizedRole);
        }
    }

    /// Builds the immutable summary snapshot stored with the record.
    ///
    /// Reads the milestone vector through [`crate::ttl::load_milestones`] so the
    /// snapshot and the on-ledger milestones are read from a single, TTL-bumped
    /// source. Every derived value is computed with checked arithmetic: a
    /// contract whose accounting does not add up is rejected loudly instead of
    /// being frozen into the permanent record with a wrong balance.
    fn summarize_contract(env: &Env, contract_id: u32, contract: &Contract) -> ContractSummary {
        let milestones: Vec<Milestone> = crate::ttl::load_milestones(env, contract_id);

        let mut total_amount: i128 = 0;
        let mut released_milestone_count: u32 = 0;
        let mut milestone_summaries = Vec::new(env);

        for (index, ms) in milestones.iter().enumerate() {
            let idx = index as u32;

            // I4: `released` and `refunded` are mutually exclusive terminal
            // states. A milestone carrying both is unreachable and would make
            // the sealed record self-contradictory.
            if ms.released && ms.refunded {
                env.panic_with_error(Error::AccountingInvariantViolated);
            }

            total_amount = total_amount
                .checked_add(ms.amount)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

            // Invariant: a milestone cannot be both released and refunded.
            // Allowing both would double-count funds in the summary and
            // break downstream accounting.
            if ms.released && ms.refunded {
                env.panic_with_error(Error::InvalidState);
            }

            if ms.released {
                released_milestone_count = released_milestone_count
                    .checked_add(1)
                    .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
            }

            milestone_summaries.push_back(MilestoneSummary {
                index: idx,
                amount: ms.amount,
                released: ms.released,
                refunded: ms.refunded,
            });
        }

        let refundable_balance = contract
            .funded_amount
            .checked_sub(contract.released_amount)
            .and_then(|remaining| remaining.checked_sub(contract.refunded_amount))
            .unwrap_or_else(|| env.panic_with_error(Error::AccountingInvariantViolated));

        ContractSummary {
            schema_version: crate::types::CONTRACT_SUMMARY_SCHEMA_VERSION,
            client: contract.client.clone(),
            freelancer: contract.freelancer.clone(),
            arbiter: contract.arbiter.clone(),
            status: contract.status,
            reputation_issued: contract.reputation_issued,
            total_amount,
            funded_amount: contract.funded_amount,
            released_amount: contract.released_amount,
            refundable_balance,
            released_milestone_count,
            milestones: milestone_summaries,
        }
    }

    /// Refuse to seal accounting that cannot be reconciled.
    ///
    /// Runs after the summary is built and before anything is written, so a
    /// contract in an unreconcilable state stays mutable and an operator can
    /// still repair or refund it.  Refusing is the safe direction: the
    /// alternative is freezing a balance that does not add up into a record
    /// that can never be corrected.
    ///
    /// # Panics
    /// - `AccountingInvariantViolated` when any of I1, I2, I3, I5 is broken.
    /// - `PotentialOverflow` when the paid totals cannot be summed.
    fn require_sealable_accounting(env: &Env, contract: &Contract, summary: &ContractSummary) {
        // I1: ledger totals are non-negative.
        if contract.funded_amount < 0
            || contract.released_amount < 0
            || contract.refunded_amount < 0
        {
            env.panic_with_error(Error::AccountingInvariantViolated);
        }

        // I2: released + refunded <= funded.
        let paid = contract
            .released_amount
            .checked_add(contract.refunded_amount)
            .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
        if paid > contract.funded_amount {
            env.panic_with_error(Error::AccountingInvariantViolated);
        }

        // I3: a contract cannot have been funded beyond its milestone total,
        // and the milestone total itself cannot be negative.
        if summary.total_amount < 0 || contract.funded_amount > summary.total_amount {
            env.panic_with_error(Error::AccountingInvariantViolated);
        }

        // I5: the summary is a faithful projection of the contract it was
        // derived from. Guards against the two ever drifting apart.
        if summary.status != contract.status
            || summary.funded_amount != contract.funded_amount
            || summary.released_amount != contract.released_amount
            || summary.refundable_balance != contract.funded_amount - paid
        {
            env.panic_with_error(Error::AccountingInvariantViolated);
        }
    }
}

/// Finalize an escrow contract by writing immutable close metadata.
///
/// `finalizer` must authorize the call and must be the stored client,
/// freelancer, or assigned arbiter. Finalization is allowed only while the
/// contract is in a terminal state: `Completed`, `Disputed`, `Refunded`,
/// or `Cancelled`. Once finalized, future contract-specific mutations
/// fail with `AlreadyFinalized`.
///
/// # Execution order
///
/// The checks below run in a fixed order so that the reported error always
/// names the first condition that actually failed, on every call and in every
/// build profile:
///
/// 1. `contract_id` is in bounds.
/// 2. No close record exists yet (`AlreadyFinalized`).
/// 3. Pause and emergency controls are clear.
/// 4. The contract exists and is in a sealable state.
/// 5. `finalizer` authorized the call and is a participant of this contract.
/// 6. The close summary reconciles with the contract record.
/// 7. The seal is written, the dispute snapshot is dropped if any, and the
///    `finalized` event is published.
///
/// Steps 1–6 are read-only. A panic anywhere in them aborts the invocation
/// before any storage is touched, so the contract remains unfinalized and
/// retryable and a competing finalizer cannot observe a partial seal.
///
/// Evaluation order is fixed so that every input maps to exactly one outcome:
/// duplicate-work guard → contract existence → status gate → pause/emergency
/// gate → authorization → finalizer role. The duplicate-work guard runs first
/// on purpose: a retried or racing call always reports `AlreadyFinalized`
/// rather than a status-dependent error, which is what makes retries
/// idempotent for callers.
///
/// # Errors
/// - `AlreadyFinalized` when a close record already exists (checked first; no
///   state is mutated and no event is published).
/// - `ContractNotFound` when `contract_id` is unknown.
/// - `InvalidStatusTransition` unless status is `Completed` or `Disputed`.
/// - `ContractPaused` when pause controls are active.
/// - `EmergencyActive` when the emergency stop is engaged.
/// - `UnauthorizedRole` when `finalizer` is not a contract participant.
/// - `AccountingInvariantViolated` when the derived refundable balance would
///   underflow.
///
/// # Events
/// Publishes `(Symbol "finalized", contract_id)` with `(finalizer, timestamp)`
/// exactly once, after the record is durable.
pub fn finalize_contract_impl(env: &Env, contract_id: u32, finalizer: Address) -> bool {
    // Invariant 2: reject duplicate work before anything observable happens.
    if Escrow::is_finalized(env, contract_id) {
        env.panic_with_error(Error::AlreadyFinalized);
    }

    let contract = Escrow::load_contract_for_finalization(env, contract_id);
    if contract.status != ContractStatus::Completed && contract.status != ContractStatus::Disputed {
        env.panic_with_error(EscrowError::InvalidStatusTransition);
    }

    Escrow::require_not_paused(env);
    finalizer.require_auth();
    Escrow::require_finalizer_role(env, &contract, &finalizer);

    // Keep the closed contract entry alive for the same window as every other
    // persistent entry it touches (the milestone vector is refreshed by
    // `summarize_contract`, which reads it through the TTL-aware loader).
    crate::ttl::extend_contract_ttl(env, contract_id);

    let record = FinalizationRecord {
        finalizer: finalizer.clone(),
        timestamp: env.ledger().timestamp(),
        summary: Escrow::summarize_contract(env, contract_id, &contract),
    };

    // Invariant 1 + 3: the record is written once and immediately given the
    // full persistent window so the guard cannot expire early.
    crate::settlement::write_finalization(env, contract_id, &record);
    crate::ttl::extend_finalization_ttl(env, contract_id);

    // A disputed contract carries a pre-dispute snapshot that `rollback_dispute`
    // would restore while the dispute is still untouched. Finalization is a
    // deliberate, authorized close, so the snapshot is superseded and dropped
    // here — but only after the summary has been fully validated, so the
    // recovery path is never discarded in exchange for a record that then
    // fails to reconcile. Removal is idempotent, so a contract sealed without
    // a snapshot is unaffected.
    if contract.status == ContractStatus::Disputed {
        crate::rollback::clear_dispute_rollback(env, contract_id);
    }

    // Publish the sealed summary alongside the finalizer and timestamp so an
    // indexer can reconcile the close from the event alone. The payload is
    // addresses, status flags and amounts already present in the record — no
    // evidence strings, keys or other sensitive data.
    env.events().publish(
        (symbol_short!("finalized"), contract_id),
        (finalizer, record.timestamp, record.summary),
    );

    true
}

/// Return immutable close metadata for `contract_id`, if it has been finalized.
///
/// Reading the record refreshes its TTL, so indexers that poll it keep the
/// closed contract from ever looking unfinalized (invariant 3).
pub fn get_finalization_record_impl(env: &Env, contract_id: u32) -> Option<FinalizationRecord> {
    let record: Option<FinalizationRecord> = env
        .storage()
        .persistent()
        .get(&Escrow::finalization_key(contract_id));
    if record.is_some() {
        crate::ttl::extend_finalization_ttl(env, contract_id);
    }
    record
}
