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

pub const MAINNET_PROTOCOL_VERSION: u32 = 1u32;
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000_000i128;

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
    /// Returns the protocol-wide hard-coded bounds used by validation paths.
    ///
    /// Callers and off-chain indexers should query this endpoint to discover
    /// the limits enforced by `create_contract` without relying on hard-coded
    /// constants:
    ///
    /// - `max_milestones`: maximum number of milestones per contract.
    /// - `max_single_milestone_stroops`: maximum amount for any single milestone.
    /// - `max_total_escrow_stroops`: maximum sum of all milestone amounts.
    /// - `max_fee_bps`: protocol fee ceiling in basis points (10 000 = 100 %).
    ///
    /// These are compile-time constants — the return value never changes
    /// between calls on the same contract binary. The function is read-only
    /// and requires no authorization.
    pub fn get_bounds(_env: Env) -> crate::ContractBounds {
        crate::ContractBounds {
            max_milestones: MAX_MILESTONES,
            max_single_milestone_stroops: crate::MAX_SINGLE_AMOUNT_STROOPS,
            max_total_escrow_stroops: MAX_TOTAL_ESCROW_STROOPS,
            max_fee_bps: 10_000,
        }
    }

    /// Checks whether a contract with the given ID exists in storage.
    ///
    /// This is a cheap, non-panicking existence probe that returns `true` if
    /// the contract record is present and `false` otherwise. Unlike `get_contract`,
    /// this function does **not** panic with `ContractNotFound` for missing IDs,
    /// making it safe for indexers and clients iterating over ID ranges.
    ///
    /// # Security
    /// This is a read-only operation that does **not** extend the contract's TTL.
    /// Probing for contract existence cannot be abused to keep entries alive.
    /// Only actual contract operations (reads/writes) extend TTL.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID to check
    ///
    /// # Returns
    /// * `true` if the contract exists
    /// * `false` if the contract does not exist
    ///
    /// # Examples
    /// ```
    /// // Safe iteration over a range of IDs
    /// for id in 1..=100 {
    ///     if escrow.contract_exists(id) {
    ///         let contract = escrow.get_contract(id);
    ///         // process contract
    ///     }
    /// }
    /// ```
    pub fn contract_exists(env: Env, contract_id: u32) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Contract(contract_id))
    }

    /// Retrieves contract information.
    pub fn get_contract(env: Env, contract_id: u32) -> Contract {
        let contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        // Extend TTL on contract read
        ttl::extend_contract_ttl(&env, contract_id);
        contract
    }

    /// Returns the next contract ID to be allocated (the high-water mark).
    ///
    /// This reader returns the current value of `NextContractId`, which represents
    /// the next ID that will be assigned when `create_contract` is called.
    /// Indexers can use this to determine the allocation high-water mark and
    /// safely iterate over the allocated ID range `[1, get_next_contract_id() - 1]`.
    ///
    /// # Security
    /// This is a read-only operation that does not mutate contract state or extend TTL.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// The next contract ID to be allocated (always ≥ 1)
    ///
    /// # Examples
    /// ```
    /// // Get the high-water mark
    /// let next_id = escrow.get_next_contract_id();
    /// // All allocated IDs are in the range [1, next_id - 1]
    /// for id in 1..next_id {
    ///     if escrow.contract_exists(id) {
    ///         let contract = escrow.get_contract(id);
    ///         // process contract
    ///     }
    /// }
    /// ```
    pub fn get_next_contract_id(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    }

    /// Returns a structured summary of the contract and its milestones.
    ///
    /// Extends contract and milestone TTL on read without requiring caller auth.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    ///
    /// # Returns
    /// The detailed `ContractSummary` for off-chain consumption
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    pub fn get_contract_summary(env: Env, contract_id: u32) -> ContractSummary {
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Extend TTL on contract and milestones read
        ttl::extend_contract_and_milestones_ttl(&env, contract_id);

        let milestones = ttl::load_milestones(&env, contract_id);
        let total_amount: i128 =
            crate::amount_validation::accumulate_amounts(milestones.iter().map(|m| m.amount))
                .unwrap_or_else(|_| env.panic_with_error(EscrowError::PotentialOverflow));
        let released_milestone_count = milestones.iter().filter(|m| m.released).count() as u32;

        let mut milestone_summaries = Vec::new(&env);
        for (idx, m) in milestones.iter().enumerate() {
            milestone_summaries.push_back(MilestoneSummary {
                index: idx as u32,
                amount: m.amount,
                released: m.released,
                refunded: m.refunded,
            });
        }

        let reputation_issued = env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::ReputationIssued(contract_id))
            .unwrap_or(contract.reputation_issued);

        let refundable_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;

        ContractSummary {
            schema_version: CONTRACT_SUMMARY_SCHEMA_VERSION,
            client: contract.client,
            freelancer: contract.freelancer,
            arbiter: contract.arbiter,
            status: contract.status,
            reputation_issued,
            total_amount,
            funded_amount: contract.funded_amount,
            released_amount: contract.released_amount,
            refundable_balance,
            released_milestone_count,
            milestones: milestone_summaries,
        }
    }

    /// Retrieves all milestones for a contract.
    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<crate::Milestone> {
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_milestone_ttl(&env, contract_id);
        milestones
    }

    /// Retrieves a single milestone by index for a contract.
    ///
    /// This is the bounds-checked single-item counterpart to
    /// `get_milestones`. Off-chain callers that only need one milestone's
    /// state (amount, funded/released/refunded flags, deadline, work evidence)
    /// can avoid fetching and decoding the full `Vec<Milestone>`.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_index` - The zero-based index of the milestone to read
    ///
    /// # Returns
    /// * `Some(Milestone)` if `milestone_index` is in bounds
    /// * `None` if `milestone_index` is out of bounds
    ///
    /// # Panics
    /// Panics with `ContractNotFound` if the contract's milestones were never
    /// allocated (i.e. the contract id is unknown), matching
    /// `get_milestones`.
    ///
    /// # Side effects
    /// Extends the milestones vector TTL on a successful read, consistent with
    /// `get_milestones`. Auth-free and otherwise non-mutating.
    pub fn get_milestone(
        env: Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<crate::Milestone> {
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<crate::Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_milestone_ttl(&env, contract_id);
        milestones.get(milestone_index)
    }

    /// Returns funded minus released minus refunded for `contract_id`.
    pub fn get_refundable_balance(env: Env, contract_id: u32) -> i128 {
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_contract_ttl(&env, contract_id);
        contract.funded_amount - contract.released_amount - contract.refunded_amount
    }

    /// Checks if a specific milestone is overdue based on its deadline.
    ///
    /// A milestone is considered overdue if:
    /// - It has a deadline set (Some value)
    /// - The current time is strictly greater than the deadline (now > deadline)
    /// - The milestone has not been released
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_index` - The index of the milestone to check
    ///
    /// # Returns
    /// `true` if the milestone is overdue, `false` otherwise
    ///
    /// # Note
    /// - Returns `false` if milestone has no deadline (None)
    /// - Returns `false` if milestone is already released
    /// - Boundary condition: at exactly the deadline (now == deadline), returns `false`
    ///   because the deadline hasn't passed yet (uses strictly > comparison)
    ///
    /// # Security
    /// Uses `now_seconds(&env)` which is the single source of truth for ledger time.
    /// Time cannot be manipulated by contract callers.
    pub fn is_milestone_overdue(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        let _contract: Contract = match env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
        {
            Some(c) => c,
            None => return false, // Contract not found, not overdue
        };

        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<crate::Milestone> = match env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
        {
            Some(m) => m,
            None => return false, // No milestones, not overdue
        };

        if milestone_index >= milestones.len() {
            return false; // Index out of bounds, not overdue
        }

        let milestone = milestones.get(milestone_index).unwrap();

        // Return false if already released
        if milestone.released {
            return false;
        }

        // Return false if no deadline set
        match milestone.deadline {
            None => false,
            Some(deadline) => {
                // Overdue if now > deadline (strictly greater)
                crate::utils::now_seconds(&env) > deadline
            }
        }
    }

    /// Returns the mainnet readiness info for the escrow contract.
    pub fn get_mainnet_readiness_info(env: Env) -> MainnetReadinessInfo {
        let checklist = Self::load_checklist(&env);
        MainnetReadinessInfo {
            initialized: checklist.initialized,
            governed_params_set: checklist.governed_params_set,
            emergency_controls_enabled: checklist.emergency_controls_enabled,
            caps_set: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS > 0,
            protocol_version: MAINNET_PROTOCOL_VERSION,
            max_escrow_total_stroops: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
        }
    }

    // ── Admin: set arbiter ───────────────────────────────────────────────────

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

    // ─── Configurable limits ──────────────────────────────────────────────────

    pub fn set_max_milestones(env: Env, max_milestones: u32) -> bool {
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

        env.storage()
            .persistent()
            .set(&DataKey::MaxMilestones, &max_milestones);

        env.events().publish(
            (symbol_short!("limits"), Symbol::new(&env, "max_milestones")),
            (max_milestones, env.ledger().timestamp()),
        );
        true
    }

    pub fn get_max_milestones(env: Env) -> u32 {
        Self::effective_max_milestones(&env)
    }

    pub fn set_max_escrow_stroops(env: Env, max_escrow_stroops: i128) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
        admin.require_auth();

        if max_escrow_stroops < MIN_MAX_ESCROW_STROOPS
            || max_escrow_stroops > MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS
        {
            env.panic_with_error(EscrowError::LimitOutOfRange);
        }

        env.storage()
            .persistent()
            .set(&DataKey::MaxEscrowStroops, &max_escrow_stroops);

        env.events().publish(
            (symbol_short!("limits"), Symbol::new(&env, "max_escrow")),
            (max_escrow_stroops, env.ledger().timestamp()),
        );
        true
    }

    pub fn get_max_escrow_stroops(env: Env) -> i128 {
        Self::effective_max_escrow_stroops(&env)
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    pub(crate) fn load_checklist(env: &Env) -> crate::ReadinessChecklist {
        env.storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default()
    }

    pub(crate) fn effective_max_milestones(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::MaxMilestones)
            .unwrap_or(DEFAULT_MAX_MILESTONES)
    }

    pub(crate) fn effective_max_escrow_stroops(env: &Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::MaxEscrowStroops)
            .unwrap_or(DEFAULT_MAX_TOTAL_ESCROW_STROOPS)
    }

    /// Validates that the given contract_id is within the valid range.
    /// Panics with `InvalidContractId` if the id is 0.
    pub(crate) fn validate_contract_id_bounds(env: &Env, contract_id: u32) {
        if contract_id == 0 {
            env.panic_with_error(EscrowError::InvalidContractId);
        }
    }
}
