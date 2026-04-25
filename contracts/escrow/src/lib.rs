#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
    Symbol, Vec,
};

mod ttl;

pub use ttl::{
    LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS,
    PENDING_MIGRATION_BUMP_THRESHOLD, PENDING_MIGRATION_TTL_LEDGERS,
};

use types::ContractStatus;

mod types;

// ─── Bounds constants ─────────────────────────────────────────────────────────
//
// Policy decision: bounds are HARD-CODED for the initial release rather than
// governed on-chain. Rationale:
//   • Governance machinery adds upgrade-path complexity and new attack surface.
//   • Hard limits give the strongest security guarantee with zero runtime cost.
//   • A future governance proposal can introduce adjustable parameters if
//     operational experience shows the defaults need revisiting.
//
// MAX_MILESTONES: limits worst-case per-contract storage and loop cost.
//   10 milestones covers the overwhelming majority of real freelance contracts.
//
// MAX_TOTAL_ESCROW_STROOPS: caps the maximum value locked in a single contract
//   to 1 000 000 tokens (7-decimal stroops) to bound worst-case griefing impact.

/// Maximum number of milestones allowed per contract.
pub const MAX_MILESTONES: u32 = 10;

/// Hard cap on the total escrow value per contract, in stroops (7 decimal places).
/// Equals 1 000 000 tokens.
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = 1_000_000_0000000; // 1 M tokens × 10^7 = 10^13

pub const MAINNET_PROTOCOL_VERSION: u32 = 1u32;
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000_000i128;

mod types;
pub use crate::types::{MainnetReadinessInfo, ReadinessChecklist};
use crate::types::DataKey as ReadinessDataKey;

#[contract]
pub struct Escrow;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    UnauthorizedRole = 6,
    InvalidStatusTransition = 7,
    AlreadyCancelled = 8,
    ContractNotFound = 9,
    MilestonesAlreadyReleased = 10,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<i128>,
    pub status: ContractStatus,
    pub total_deposited: i128,
    pub released_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingApproval {
    pub approver: Address,
    pub contract_id: u32,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingMigration {
    pub proposer: Address,
    pub new_wasm_hash: BytesN<32>,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

/// Per-contract lifecycle checklist persisted alongside the contract record.
///
/// Each field is set to `true` exactly once by the internal `update_checklist`
/// helper. No public entry-point accepts a `ContractChecklist` argument —
/// external callers can only read it via `get_checklist`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ContractChecklist {
    /// Set when `create_contract` succeeds.
    pub created: bool,
    /// Set on the first successful `deposit_funds` call.
    pub funded: bool,
    /// Set when at least one milestone has been released.
    pub milestone_released: bool,
    /// Set when all milestones have been released (contract completed).
    pub all_milestones_released: bool,
    /// Set when `cancel_contract` transitions the contract to `Cancelled`.
    pub cancelled: bool,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Contract(u32),
    MilestoneReleased(u32, u32),
    RefundableBalance(u32),
    /// Per-contract lifecycle checklist. Keyed by contract ID.
    /// Only written by the private `update_checklist` helper.
    Checklist(u32),
}

fn update_readiness_checklist<F>(env: &Env, f: F)
where
    F: FnOnce(&mut ReadinessChecklist),
{
    let mut checklist: ReadinessChecklist = env
        .storage()
        .instance()
        .get(&ReadinessDataKey::ReadinessChecklist)
        .unwrap_or_default();
    f(&mut checklist);
    env.storage()
        .instance()
        .set(&ReadinessDataKey::ReadinessChecklist, &checklist);
}

/// Internal-only helper: load, mutate, and persist the checklist for `id`.
/// Never exposed as a contract entry-point; callers cannot invoke it directly.
fn update_checklist<F>(env: &Env, id: u32, f: F)
where
    F: FnOnce(&mut ContractChecklist),
{
    let key = DataKey::Checklist(id);
    let mut cl: ContractChecklist = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_default();
    f(&mut cl);
    env.storage().persistent().set(&key, &cl);
}

#[contractimpl]
impl Escrow {
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Returns the hard-coded bounds enforced by this contract.
    /// Useful for client-side pre-validation and monitoring dashboards.
    pub fn get_bounds(_env: Env) -> EscrowBounds {
        EscrowBounds {
            max_milestones: MAX_MILESTONES,
            max_total_escrow_stroops: MAX_TOTAL_ESCROW_STROOPS,
        }
    }

    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestones: Vec<i128>,
        terms_hash: Option<Bytes>,
        grace_period_seconds: Option<u64>,
    ) -> u32 {
        client.require_auth();

        if client == freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }

        // Validate arbiter doesn't overlap with client/freelancer
        if let Some(ref a) = arbiter {
            if *a == client || *a == freelancer {
                env.panic_with_error(EscrowError::InvalidParticipant);
            }
        }

        if milestones.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }
        if milestones.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        let mut total_amount: i128 = 0;
        let mut milestones: Vec<Milestone> = Vec::new(&env);
        for amount in milestone_amounts.iter() {
            if amount <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
            total_amount += amount;
            milestones.push_back(Milestone {
                amount,
                released: false,
                refunded: false,
            });
        }

        let id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ContractCount)
            .unwrap_or(0u32);

        let data = EscrowContractData {
            client,
            freelancer,
            arbiter,
            milestones,
            status: ContractStatus::Created,
            total_deposited: 0,
            released_amount: 0,
        };

        env.storage().persistent().set(&DataKey::Contract(id), &data);
        env.storage()
            .persistent()
            .set(&DataKey::Milestones(id), &milestones);
        env.storage().persistent().set(&DataKey::ContractCount, &(id + 1));

        update_checklist(&env, id, |cl| cl.created = true);

        id
    }

    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        if amount <= 0 {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, ContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        contract.total_deposited += amount;

        // Update status to Funded if not already
        if contract.status == ContractStatus::Created {
            contract.status = ContractStatus::Funded;
        }

        env.storage().persistent().set(&contract_key, &contract);

        update_checklist(&env, contract_id, |cl| cl.funded = true);

        true
    }

    pub fn approve_milestone(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        // Store approval time using ledger timestamp
        let approval_time = env.ledger().timestamp();
        env.storage().persistent().set(
            &DataKey::MilestoneApprovalTime(contract_id, milestone_index),
            &approval_time,
        );
        true
    }

    pub fn release_milestone(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, ContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Mark this milestone as released
        let milestone_key = DataKey::MilestoneReleased(contract_id, milestone_index);
        env.storage().persistent().set(&milestone_key, &true);

        // Update released amount
        if let Some(amount) = contract.milestones.get(milestone_index) {
            contract.released_amount += amount;
        }

        env.storage().persistent().set(&contract_key, &contract);

        // Check whether every milestone is now released.
        let total = contract.milestones.len();
        let all_released = (0..total).all(|i| {
            env.storage()
                .persistent()
                .get::<_, bool>(&DataKey::MilestoneReleased(contract_id, i))
                .unwrap_or(false)
        });
        update_checklist(&env, contract_id, |cl| {
            cl.milestone_released = true;
            if all_released {
                cl.all_milestones_released = true;
            }
        });

        true
    }

    /// Get contract details
    pub fn get_contract(env: Env, contract_id: u32) -> ContractData {
        env.storage()
            .persistent()
            .get::<_, ContractData>(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    /// Get milestones for a contract
    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<i128> {
        let contract = Self::get_contract(env.clone(), contract_id);
        contract.milestones
    }

    /// Returns the lifecycle checklist for `contract_id`.
    ///
    /// Read-only. Intended for monitoring and ops tooling to verify that a
    /// contract has progressed through the expected lifecycle stages.
    /// Panics with `ContractNotFound` if no checklist exists for the given ID.
    pub fn get_checklist(env: Env, contract_id: u32) -> ContractChecklist {
        env.storage()
            .persistent()
            .get(&DataKey::Checklist(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    /// Cancel an escrow contract under strict authorization and state constraints
    pub fn cancel_contract(env: Env, contract_id: u32, caller: Address) -> bool {
        // 1. Require cryptographic authorization
        caller.require_auth();

        // 2. Load contract data
        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, ContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // 3. Check if already cancelled (idempotency guard)
        if contract.status == ContractStatus::Cancelled {
            env.panic_with_error(EscrowError::AlreadyCancelled);
        }

        // 4. Block cancellation in terminal states
        if contract.status == ContractStatus::Completed {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        // 5. Role-based authorization with state checks
        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref().is_some_and(|a| *a == caller);

        match contract.status {
            ContractStatus::Created => {
                // Client or freelancer can cancel before funding
                if !is_client && !is_freelancer {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ContractStatus::Funded => {
                // Calculate released milestones
                let released_amount = Self::calculate_released_amount(&env, contract_id, &contract);

                if is_client {
                    // Client can cancel only if NO milestones released
                    if released_amount > 0 {
                        env.panic_with_error(EscrowError::MilestonesAlreadyReleased);
                    }
                } else if is_freelancer {
                    // Freelancer can cancel (economic deterrent - funds return to client)
                    // No additional checks needed
                } else if is_arbiter {
                    // Arbiter can cancel in funded state (dispute resolution)
                } else {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ContractStatus::Disputed => {
                // Only arbiter can cancel disputed contracts
                if !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            _ => {
                env.panic_with_error(EscrowError::InvalidStatusTransition);
            }
        }

        // 6. Transition to Cancelled state
        contract.status = ContractStatus::Cancelled;
        env.storage().persistent().set(&contract_key, &contract);

        update_checklist(&env, contract_id, |cl| cl.cancelled = true);

        // 7. Emit indexer-friendly event
        env.events().publish(
            (Symbol::new(&env, "contract_cancelled"), contract_id),
            (caller, contract.status, env.ledger().timestamp()),
        );

        true
    }

    /// Helper: Calculate total released amount for a contract
    fn calculate_released_amount(env: &Env, contract_id: u32, contract: &ContractData) -> i128 {
        let mut released = 0i128;
        for (idx, amount) in contract.milestones.iter().enumerate() {
            let milestone_key = DataKey::MilestoneReleased(contract_id, idx as u32);
            if env
                .storage()
                .persistent()
                .get::<_, bool>(&milestone_key)
                .unwrap_or(false)
            {
                released += amount;
            }
        }
        released
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_lifecycle_checklist;

#[cfg(test)]
mod proptest;
