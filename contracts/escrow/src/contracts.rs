//! Escrow contract entity management.
//!
//! This module owns the core escrow contract CRUD operations: querying
//! contract state, reading milestones, managing configurable limits, and
//! protocol bounds. Financial operations (deposit, release, refund, cancel),
//! dispute resolution, reputation, settlement-token binding, and governance
//! remain in their respective modules.
//!
//! ## Module responsibilities
//!
//! | Entrypoint | Mutating? | Notes |
//! | --- | --- | --- |
//! | `get_contract` | read | Returns stored `Contract` + TTL bump |
//! | `contract_exists` | read | Non-panicking existence probe |
//! | `get_next_contract_id` | read | Allocation high-water mark |
//! | `get_contract_summary` | read | Full `ContractSummary` for indexers |
//! | `get_milestones` | read | All `Milestone` entries for a contract |
//! | `get_milestone` | read | Single milestone by index |
//! | `get_refundable_balance` | read | `funded − released − refunded` |
//! | `is_milestone_overdue` | read | Deadline-based overdue check |
//! | `get_bounds` | read | Protocol-wide hard-coded limits |
//! | `get_mainnet_readiness_info` | read | Deployment-readiness snapshot |
//! | `set_arbiter` | write | Admin updates contract arbiter |
//! | `set_max_milestones` | write | Admin configures milestone cap |
//! | `get_max_milestones` | read | Returns effective milestone cap |
//! | `set_max_escrow_stroops` | write | Admin configures escrow cap |
//! | `get_max_escrow_stroops` | read | Returns effective escrow cap |

use soroban_sdk::{contractimpl, symbol_short, Address, Env, Symbol, Vec};

use crate::{
    ttl, Contract, ContractStatus, ContractSummary, DataKey, Error, Escrow, EscrowArgs,
    EscrowClient, EscrowError, MilestoneSummary, ReleaseAuthorization,
    CONTRACT_SUMMARY_SCHEMA_VERSION,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default maximum number of milestones allowed per contract.
pub const DEFAULT_MAX_MILESTONES: u32 = 10;

/// Default hard cap on the total escrow value per contract, in stroops.
pub const DEFAULT_MAX_TOTAL_ESCROW_STROOPS: i128 = 10_000_000_000_000;

/// Backward-compatible alias for the default max milestones.
pub const MAX_MILESTONES: u32 = DEFAULT_MAX_MILESTONES;

/// Backward-compatible alias for the default max escrow stroops.
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = DEFAULT_MAX_TOTAL_ESCROW_STROOPS;

/// Upper bound on the `limit` parameter of paginated read views.
///
/// Keeps per-call storage reads bounded and prevents callers from requesting
/// unbounded scans in a single invocation.
pub const PAGE_CEILING: u32 = 50;

/// Absolute minimum for the max milestones setting.
pub const MIN_MAX_MILESTONES: u32 = 1;

/// Absolute maximum for the max milestones setting.
pub const MAX_MAX_MILESTONES: u32 = 100;

/// Absolute minimum for the max escrow stroops setting (0.01 XLM).
pub const MIN_MAX_ESCROW_STROOPS: i128 = 1_000_000;

// ── Settlement (batch finalize) limit ────────────────────────────────────────

/// Default maximum number of contracts finalizable in a single batch settlement call.
pub const DEFAULT_MAX_BATCH_SETTLEMENT: u32 = 10;

/// Absolute minimum for the max batch settlement setting.
pub const MIN_MAX_BATCH_SETTLEMENT: u32 = 1;

/// Absolute maximum for the max batch settlement setting.
pub const MAX_MAX_BATCH_SETTLEMENT: u32 = 100;

/// Backward-compatible alias for the default max batch settlement.
pub const MAX_BATCH_SETTLEMENT: u32 = DEFAULT_MAX_BATCH_SETTLEMENT;

pub const MAINNET_PROTOCOL_VERSION: u32 = 1u32;
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000_000i128;

/// Default maximum number of arbiters allowed per contract.
pub const DEFAULT_MAX_ARBITERS: u32 = 1;

/// Absolute minimum for the max arbiters setting.
pub const MIN_MAX_ARBITERS: u32 = 1;

/// Absolute maximum for the max arbiters setting.
pub const MAX_MAX_ARBITERS: u32 = 10;

// ── Types ─────────────────────────────────────────────────────────────────────

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<i128>,
    pub status: ContractStatus,
    pub total_deposited: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub reputation_issued: bool,
}

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub completed_contracts: u32,
    pub total_rating: i128,
    pub last_rating: i128,
}

impl Default for ReputationRecord {
    fn default() -> Self {
        ReputationRecord {
            completed_contracts: 0,
            total_rating: 0,
            last_rating: 0,
        }
    }
}

#[soroban_sdk::contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainnetReadinessInfo {
    pub initialized: bool,
    pub governed_params_set: bool,
    pub emergency_controls_enabled: bool,
    pub caps_set: bool,
    pub protocol_version: u32,
    pub max_escrow_total_stroops: i128,
}

// ── Entrypoints ───────────────────────────────────────────────────────────────

#[contractimpl]
impl Escrow {
    pub fn set_arbiter(
        env: Env,
        contract_id: u32,
        admin: Address,
        new_arbiter: Option<Address>,
    ) -> bool {
        Self::require_initialized(&env);
        Self::require_not_paused(&env);
        Self::validate_contract_id_bounds(&env, contract_id);

        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        if admin != stored_admin {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        admin.require_auth();

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);

        if let Some(ref arb) = new_arbiter {
            if *arb == contract.client || *arb == contract.freelancer {
                env.panic_with_error(EscrowError::InvalidArbiter);
            }
        }

        if new_arbiter.is_none() {
            match contract.release_authorization {
                ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter => {
                    env.panic_with_error(EscrowError::MissingArbiter);
                }
                _ => {}
            }
        }

        let old_arbiter = contract.arbiter.clone();
        contract.arbiter = new_arbiter.clone();

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("arbiter"), contract_id),
            (old_arbiter, new_arbiter, env.ledger().timestamp()),
        );

        true
    }

    pub fn set_contracts_parameters(
        env: Env,
        max_milestones: u32,
        max_escrow_stroops: i128,
    ) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        admin.require_auth();

        if max_milestones < MIN_MAX_MILESTONES || max_milestones > MAX_MAX_MILESTONES {
            env.panic_with_error(EscrowError::LimitOutOfRange);
        }
        if max_escrow_stroops < MIN_MAX_ESCROW_STROOPS
            || max_escrow_stroops > MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
        {
            env.panic_with_error(EscrowError::LimitOutOfRange);
        }

        let params = crate::types::ContractsParameters {
            max_milestones,
            max_escrow_stroops,
        };

        env.storage()
            .persistent()
            .set(&DataKey::ContractsParameters, &params);

        env.events().publish(
            (symbol_short!("contracts"), Symbol::new(&env, "params")),
            (params, env.ledger().timestamp()),
        );
        true
    }

    pub fn get_contracts_parameters(env: Env) -> crate::types::ContractsParameters {
        env.storage()
            .persistent()
            .get(&DataKey::ContractsParameters)
            .unwrap_or_default()
    }
}

impl Escrow {
    pub(crate) fn load_checklist(env: &Env) -> crate::ReadinessChecklist {
        env.storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default()
    }

    pub(crate) fn effective_max_milestones(env: &Env) -> u32 {
        // Prefer the dedicated admin override key written by `set_max_milestones`.
        if let Some(v) = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::MaxMilestones)
        {
            return v;
        }
        env.storage()
            .persistent()
            .get::<_, crate::types::ContractsParameters>(&DataKey::ContractsParameters)
            .unwrap_or_default()
            .max_milestones
    }

    pub(crate) fn effective_max_escrow_stroops(env: &Env) -> i128 {
        // Prefer the dedicated admin override key written by `set_max_escrow_stroops`.
        if let Some(v) = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::MaxEscrowStroops)
        {
            return v;
        }
        env.storage()
            .persistent()
            .get::<_, crate::types::ContractsParameters>(&DataKey::ContractsParameters)
            .unwrap_or_default()
            .max_escrow_stroops
    }

    /// Validates that the given contract_id is within the valid range.
    /// Panics with `InvalidContractId` if the id is 0.
    pub(crate) fn validate_contract_id_bounds(env: &Env, contract_id: u32) {
        if contract_id == 0 {
            env.panic_with_error(EscrowError::InvalidContractId);
        }
    }
}
