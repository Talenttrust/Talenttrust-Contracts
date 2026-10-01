//! Guarded rollback of an unchanged, unresolved dispute.
//!
//! `raise_dispute` snapshots the contract and its milestones under
//! [`DataKey::DisputeRollback`]. `rollback_dispute` lets the admin restore the
//! pre-dispute status from that snapshot, but only while nothing else about the
//! contract has changed since the dispute was raised.
//!
//! # Compatibility contract
//!
//! The following behaviour is public and must be preserved across upgrades.
//! Tests in `test/rollback.rs` pin each item.
//!
//! **Entrypoint.** `rollback_dispute(env, contract_id: u32) -> bool`. Returns
//! `true` on success; every rejection is a panic with a contract error. It never
//! returns `false`.
//!
//! **Guard order and errors.** Guards run in this fixed order, so a request that
//! fails several of them always reports the first:
//!
//! | # | Guard                                   | Error                       |
//! |---|-----------------------------------------|-----------------------------|
//! | 1 | `contract_id != 0`                      | `InvalidContractId` (4)     |
//! | 2 | system initialized                      | `NotInitialized` (36)       |
//! | 3 | not paused / not in emergency           | `ContractPaused` (37) / `EmergencyActive` (38) |
//! | 4 | admin authorizes                        | host auth error             |
//! | 5 | contract exists                         | `ContractNotFound` (10)     |
//! | 6 | contract not finalized (sealed)         | `AlreadyFinalized` (46)     |
//! | 7 | contract is `Disputed`                  | `RollbackNotAllowed` (55)   |
//! | 8 | a snapshot exists                       | `RollbackNotAllowed` (55)   |
//! | 9 | snapshot status is `Funded`/`PartiallyFunded` | `RollbackNotAllowed` (55) |
//! | 10| milestone vector exists                 | `ContractNotFound` (10)     |
//! | 11| contract and milestones unchanged since the snapshot (other than status) | `RollbackNotAllowed` (55) |
//!
//! **Empty and malformed data.** A missing snapshot (never raised, already
//! rolled back, or already resolved) is guard 8. A stored admin, contract,
//! milestone vector, or snapshot whose bytes do not decode as the expected type
//! — for example a snapshot written under an older type layout — reports
//! `StorageInvariantViolated` (87) instead of aborting with an untyped host
//! conversion error. A malformed snapshot is **not** removed: the failed call
//! rolls back, and the record stays for an admin migration to inspect. The
//! dispute can still be closed through the resolution path, which clears it.
//!
//! **Effects on success,** all in the same invocation:
//! 1. the contract's status is set to the snapshot's status;
//! 2. the snapshot is removed, so a second rollback reports guard 8;
//! 3. contract and milestone TTLs are extended;
//! 4. every outstanding milestone approval is voided;
//! 5. one event is published:
//!    topics `(symbol "rollback", contract_id: u32)`,
//!    data `(admin: Address, ContractStatus::Disputed, restored: ContractStatus, ledger_timestamp: u64)`.
//!
//! **Storage layout.** [`DisputeRollbackRecord`] is persisted, so its field
//! names and types are frozen: renaming, reordering, or retyping a field makes
//! every snapshot written before the upgrade undecodable. Add new data under a
//! new key instead.

use crate::storage::validate_contract_id_bounds;
use crate::ttl::{PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS};
use crate::{ttl, Contract, ContractStatus, DataKey, Error, Escrow, Milestone};
use soroban_sdk::{contracttype, symbol_short, Address, Env, IntoVal, TryFromVal, Val, Vec};

/// Pre-dispute snapshot. **Persisted — the layout is frozen**; see the module
/// docs.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRollbackRecord {
    pub contract: Contract,
    pub milestones: Vec<Milestone>,
}

/// Outcome of reading the snapshot for a contract.
// `Present` is much larger than the unit variants, but the value lives on the
// stack for one call and `no_std` has no `Box`, so the size gap is accepted.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RollbackRecordRead {
    Present(DisputeRollbackRecord),
    Missing,
    /// A record exists but does not decode as [`DisputeRollbackRecord`].
    Malformed,
}

fn rollback_key(contract_id: u32) -> DataKey {
    DataKey::DisputeRollback(contract_id)
}

/// Read a persistent entry, telling a missing entry apart from one that does
/// not decode as `T`. Never writes.
fn read_decoded<K, T>(env: &Env, key: &K) -> Result<Option<T>, Error>
where
    K: IntoVal<Env, Val>,
    T: TryFromVal<Env, Val>,
{
    match env.storage().persistent().get::<K, Val>(key) {
        None => Ok(None),
        Some(raw) => T::try_from_val(env, &raw)
            .map(Some)
            .map_err(|_| Error::StorageInvariantViolated),
    }
}

pub(crate) fn store_dispute_rollback(
    env: &Env,
    contract_id: u32,
    contract: &Contract,
    milestones: &Vec<Milestone>,
) {
    let key = rollback_key(contract_id);
    env.storage().persistent().set(
        &key,
        &DisputeRollbackRecord {
            contract: contract.clone(),
            milestones: milestones.clone(),
        },
    );
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS);
}

/// Remove the snapshot. Idempotent: clearing a missing snapshot is a no-op.
pub(crate) fn clear_dispute_rollback(env: &Env, contract_id: u32) {
    env.storage()
        .persistent()
        .remove(&rollback_key(contract_id));
}

/// Read the snapshot without trapping on a malformed entry.
pub(crate) fn read_dispute_rollback(env: &Env, contract_id: u32) -> RollbackRecordRead {
    match read_decoded::<_, DisputeRollbackRecord>(env, &rollback_key(contract_id)) {
        Ok(Some(record)) => RollbackRecordRead::Present(record),
        Ok(None) => RollbackRecordRead::Missing,
        Err(_) => RollbackRecordRead::Malformed,
    }
}

/// Guards 7–9 of the compatibility contract, as a pure function of the loaded
/// state. Returns the snapshot to restore from.
///
/// Guards 7–11 are kept free of storage and auth so every branch is directly
/// testable and the decision cannot depend on anything but its inputs.
pub(crate) fn check_rollback_preconditions<'a>(
    current: &Contract,
    record: &'a RollbackRecordRead,
) -> Result<&'a DisputeRollbackRecord, Error> {
    // Guard 7: only a live dispute can be rolled back.
    if current.status != ContractStatus::Disputed {
        return Err(Error::RollbackNotAllowed);
    }

    // Guard 8: a snapshot must exist. A malformed one is reported distinctly.
    let record = match record {
        RollbackRecordRead::Present(record) => record,
        RollbackRecordRead::Missing => return Err(Error::RollbackNotAllowed),
        RollbackRecordRead::Malformed => return Err(Error::StorageInvariantViolated),
    };

    // Guard 9: only a funded pre-dispute state may be restored. This also
    // rules out restoring `Disputed` itself.
    if !matches!(
        record.contract.status,
        ContractStatus::Funded | ContractStatus::PartiallyFunded
    ) {
        return Err(Error::RollbackNotAllowed);
    }

    Ok(record)
}

/// Guard 11: apart from the status flip, nothing may have changed since the
/// snapshot — otherwise restoring the old status could contradict the new
/// accounting. Returns the status to restore.
pub(crate) fn check_rollback_unchanged(
    current: &Contract,
    current_milestones: &Vec<Milestone>,
    record: &DisputeRollbackRecord,
) -> Result<ContractStatus, Error> {
    let mut expected = record.contract.clone();
    expected.status = ContractStatus::Disputed;
    if *current != expected || *current_milestones != record.milestones {
        return Err(Error::RollbackNotAllowed);
    }
    Ok(record.contract.status)
}

pub(crate) fn rollback_dispute_impl(env: &Env, contract_id: u32) -> bool {
    // Guards 1–3.
    validate_contract_id_bounds(env, contract_id);
    Escrow::require_initialized(env);
    Escrow::require_not_paused(env);

    // Guard 4.
    let admin: Address = read_decoded(env, &DataKey::Admin)
        .unwrap_or_else(|error| env.panic_with_error(error))
        .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
    admin.require_auth();

    // Guard 5.
    let mut contract: Contract = read_decoded(env, &DataKey::Contract(contract_id))
        .unwrap_or_else(|error| env.panic_with_error(error))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    // Guard 6.
    Escrow::require_not_finalized(env, contract_id);

    // Guards 7–9.
    let read = read_dispute_rollback(env, contract_id);
    let record = check_rollback_preconditions(&contract, &read)
        .unwrap_or_else(|error| env.panic_with_error(error));

    // Guard 10.
    let milestones: Vec<Milestone> =
        read_decoded(env, &ttl::milestone_storage_key(env, contract_id))
            .unwrap_or_else(|error| env.panic_with_error(error))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    // Guard 11.
    let restored_status = check_rollback_unchanged(&contract, &milestones, record)
        .unwrap_or_else(|error| env.panic_with_error(error));

    contract.status = restored_status;
    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), &contract);
    clear_dispute_rollback(env, contract_id);
    ttl::extend_contract_and_milestones_ttl(env, contract_id);

    // Void every outstanding approval when the pre-dispute status is restored.
    //
    // This is the load-bearing half of the stale-approval fix. Restoring
    // `Funded` would otherwise make any approval recorded *before* the dispute
    // releasable again, letting funds move on a consent that predates the
    // dispute and that no party re-affirmed afterwards. Clearing here means a
    // rolled-back contract always requires fresh approvals.
    //
    // `raise_dispute` already clears, so this is normally a no-op; it is
    // retained as the guarantee for disputes raised before that clear existed.
    crate::approvals::clear_all_approvals(env, contract_id);

    env.events().publish(
        (symbol_short!("rollback"), contract_id),
        (
            admin,
            ContractStatus::Disputed,
            restored_status,
            env.ledger().timestamp(),
        ),
    );

    true
}
