//! Typed storage keys and read/write helpers for settlement entries.
//!
//! This module replaces ad-hoc key construction for settlement-related
//! persistent storage with a single, auditable layer. Every settlement
//! read or write in the contract goes through the helpers defined here,
//! guaranteeing that the correct `DataKey` variant and storage bucket
//! (persistent vs. temporary) are always used.
//!
//! # Storage keys
//!
//! | Entry | `DataKey` variant | Bucket |
//! | --- | --- | --- |
//! | Settlement token address | `SettlementToken` | `persistent()` |
//! | Finalization record | `Finalization(contract_id)` | `persistent()` |
//!
//! # Round-trip guarantee
//!
//! Every `write_*` followed by the corresponding `read_*` returns the
//! same value.  The `test_settlement_storage` module in `test/` verifies
//! this invariant plus absent-key behaviour.

use crate::{finalize::FinalizationRecord, DataKey, Error};
use soroban_sdk::{Address, Env};

// ── Settlement token ────────────────────────────────────────────────────────

/// Read the bound settlement token address from persistent storage.
///
/// Returns `None` when no token has been bound yet (`bind_settlement_token`
/// has not been called).
pub fn read_settlement_token(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&DataKey::SettlementToken)
}

/// Persist the settlement token address under the canonical storage key.
///
/// Callers must ensure write-once semantics: a second bind must be
/// rejected *before* calling this helper.
pub fn write_settlement_token(env: &Env, token: &Address) {
    env.storage()
        .persistent()
        .set(&DataKey::SettlementToken, token);
}

/// Return `true` when a settlement token has been bound.
pub fn is_settlement_token_bound(env: &Env) -> bool {
    read_settlement_token(env).is_some()
}

/// Read the bound settlement token, panicking with `SettlementTokenNotConfigured`
/// when absent.  Use this in money-flow paths that require a bound token.
pub fn require_settlement_token(env: &Env) -> Address {
    read_settlement_token(env)
        .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured))
}

// ── Finalization record ─────────────────────────────────────────────────────

/// Construct the canonical `DataKey` for a finalization record.
pub fn finalization_key(contract_id: u32) -> DataKey {
    DataKey::Finalization(contract_id)
}

/// Read a finalization record for `contract_id`, if it exists.
pub fn read_finalization(env: &Env, contract_id: u32) -> Option<FinalizationRecord> {
    env.storage()
        .persistent()
        .get(&finalization_key(contract_id))
}

/// Return `true` when a finalization record already exists for `contract_id`.
pub fn is_finalized(env: &Env, contract_id: u32) -> bool {
    env.storage()
        .persistent()
        .has(&finalization_key(contract_id))
}

/// Persist a finalization record.  Callers must guard against double-
/// finalization (`is_finalized`) before calling this helper.
pub fn write_finalization(env: &Env, contract_id: u32, record: &FinalizationRecord) {
    env.storage()
        .persistent()
        .set(&finalization_key(contract_id), record);
}

/// Panic with `AlreadyFinalized` if a record already exists for `contract_id`.
pub fn require_not_finalized(env: &Env, contract_id: u32) {
    if is_finalized(env, contract_id) {
        env.panic_with_error(Error::AlreadyFinalized);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finalize::FinalizationRecord;
    use crate::{ContractStatus, ContractSummary, Escrow, CONTRACT_SUMMARY_SCHEMA_VERSION};
    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn setup_contract(env: &Env) -> Address {
        env.register(Escrow, ())
    }

    fn dummy_summary(env: &Env) -> ContractSummary {
        ContractSummary {
            schema_version: CONTRACT_SUMMARY_SCHEMA_VERSION,
            client: Address::generate(env),
            freelancer: Address::generate(env),
            arbiter: None,
            status: ContractStatus::Completed,
            reputation_issued: false,
            total_amount: 1_000,
            funded_amount: 1_000,
            released_amount: 1_000,
            refundable_balance: 0,
            released_milestone_count: 1,
            milestones: soroban_sdk::Vec::new(env),
        }
    }

    // ── Settlement token round-trip ────────────────────────────────────────

    #[test]
    fn settlement_token_absent_returns_none() {
        let env = Env::default();
        let contract = setup_contract(&env);
        env.as_contract(&contract, || {
            assert!(read_settlement_token(&env).is_none());
            assert!(!is_settlement_token_bound(&env));
        });
    }

    #[test]
    fn settlement_token_round_trip() {
        let env = Env::default();
        let contract = setup_contract(&env);
        let token = Address::generate(&env);

        env.as_contract(&contract, || {
            write_settlement_token(&env, &token);
            assert_eq!(read_settlement_token(&env), Some(token));
            assert!(is_settlement_token_bound(&env));
        });
    }

    // ── Finalization round-trip ────────────────────────────────────────────

    #[test]
    fn finalization_absent_returns_none() {
        let env = Env::default();
        let contract = setup_contract(&env);
        env.as_contract(&contract, || {
            assert!(!is_finalized(&env, 1));
            assert!(read_finalization(&env, 1).is_none());
        });
    }

    #[test]
    fn finalization_round_trip() {
        let env = Env::default();
        let contract = setup_contract(&env);
        let record = FinalizationRecord {
            finalizer: Address::generate(&env),
            timestamp: 12345,
            summary: dummy_summary(&env),
        };

        env.as_contract(&contract, || {
            write_finalization(&env, 42, &record);
            assert!(is_finalized(&env, 42));
            let loaded = read_finalization(&env, 42).unwrap();
            assert_eq!(loaded.finalizer, record.finalizer);
            assert_eq!(loaded.timestamp, 12345);
        });
    }

    #[test]
    fn finalization_different_ids_are_independent() {
        let env = Env::default();
        let contract = setup_contract(&env);

        let record_a = FinalizationRecord {
            finalizer: Address::generate(&env),
            timestamp: 100,
            summary: dummy_summary(&env),
        };
        let record_b = FinalizationRecord {
            finalizer: Address::generate(&env),
            timestamp: 200,
            summary: dummy_summary(&env),
        };

        env.as_contract(&contract, || {
            write_finalization(&env, 1, &record_a);
            write_finalization(&env, 2, &record_b);

            assert_eq!(read_finalization(&env, 1).unwrap().timestamp, 100);
            assert_eq!(read_finalization(&env, 2).unwrap().timestamp, 200);
        });
    }

    #[test]
    fn require_not_finalized_passes_when_absent() {
        let env = Env::default();
        let contract = setup_contract(&env);
        env.as_contract(&contract, || {
            require_not_finalized(&env, 99);
        });
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #46)")]
    fn require_not_finalized_panics_when_present() {
        let env = Env::default();
        let contract = setup_contract(&env);
        let record = FinalizationRecord {
            finalizer: Address::generate(&env),
            timestamp: 1,
            summary: dummy_summary(&env),
        };
        env.as_contract(&contract, || {
            write_finalization(&env, 1, &record);
            require_not_finalized(&env, 1);
        });
    }
}
