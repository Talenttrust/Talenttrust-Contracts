//! Dispute rollback.
//!
//! [`rollback_dispute_impl`] restores an *unchanged, unresolved* dispute to the
//! exact pre-dispute state captured when the dispute was raised. It is the only
//! path that moves a contract out of [`ContractStatus::Disputed`] without
//! resolving it, so every guard below exists to make that reversal safe.
//!
//! # State invariants
//!
//! | # | Invariant | Enforced by | Test |
//! | --- | --- | --- | --- |
//! | R1 | Rollback is available only from `Disputed`, and only when the stored snapshot's pre-state is `Funded`/`PartiallyFunded`. | status + snapshot checks | `rollback_is_rejected_unless_contract_is_disputed` |
//! | R2 | A successful rollback restores the exact pre-dispute snapshot — status and milestone vector are the captured values, never a partial restore. | snapshot equality check + single state write | `rollback_restores_exact_pre_dispute_state` |
//! | R3 | Rollback cannot double-apply: the snapshot is cleared on success, so a second call fails closed. | `clear_dispute_rollback` | `rollback_cannot_double_apply` |
//! | R4 | Only the stored admin may roll back; a call without the admin's authorization is rejected before any mutation. | `admin.require_auth` | `rollback_requires_admin_authorization` |
//! | R5 | Finalized (terminal, irreversible) contracts cannot be rolled back. | `require_not_finalized` | `finalized_contract_cannot_be_rolled_back` |
//! | R6 | Rollback voids every outstanding approval so a pre-dispute consent cannot be spent once the pre-state is restored. | `approvals::clear_all_approvals` | `rollback_clears_related_approvals` |
//! | R7 | If the live contract or milestone vector has diverged from the snapshot, rollback is rejected and the dispute is left intact. | snapshot equality check | `rollback_rejects_diverged_contract_snapshot`, `rollback_rejects_diverged_milestone_snapshot` |

use crate::storage;
use crate::storage::validate_contract_id_bounds;
use crate::ttl::{PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS};
use crate::{ttl, Contract, ContractStatus, DataKey, Error, Escrow, Milestone};
use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRollbackRecord {
    pub contract: Contract,
    pub milestones: Vec<Milestone>,
}

fn rollback_key(contract_id: u32) -> DataKey {
    DataKey::DisputeRollback(contract_id)
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

pub(crate) fn clear_dispute_rollback(env: &Env, contract_id: u32) {
    env.storage()
        .persistent()
        .remove(&rollback_key(contract_id));
}

pub(crate) fn rollback_dispute_impl(env: &Env, contract_id: u32) -> bool {
    validate_contract_id_bounds(env, contract_id);
    Escrow::require_initialized(env);
    Escrow::require_not_paused(env);

    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
    admin.require_auth();

    let mut contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

    Escrow::require_not_finalized(env, contract_id);
    if contract.status != ContractStatus::Disputed {
        env.panic_with_error(Error::RollbackNotAllowed);
    }

    let record: DisputeRollbackRecord = env
        .storage()
        .persistent()
        .get(&rollback_key(contract_id))
        .unwrap_or_else(|| env.panic_with_error(Error::RollbackNotAllowed));

    if !matches!(
        record.contract.status,
        ContractStatus::Funded | ContractStatus::PartiallyFunded
    ) {
        env.panic_with_error(Error::RollbackNotAllowed);
    }

    let mut expected_contract = record.contract.clone();
    expected_contract.status = ContractStatus::Disputed;
    let milestones = ttl::load_milestones(env, contract_id);
    if contract != expected_contract || milestones != record.milestones {
        env.panic_with_error(Error::RollbackNotAllowed);
    }

    if record.contract.status == ContractStatus::Disputed {
        env.panic_with_error(Error::RollbackNotAllowed);
    }

    let restored_status = record.contract.status;
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
