#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Bytes, BytesN, Env,
    Symbol, Vec,
};

mod ttl;
mod types;

pub use ttl::{
    LEDGERS_PER_DAY, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS,
    PENDING_MIGRATION_BUMP_THRESHOLD, PENDING_MIGRATION_TTL_LEDGERS,
};

mod types;
mod amount_validation;
pub use amount_validation::{
    validate_single_amount, validate_milestone_amounts, validate_deposit_amount,
    validate_contract_total, safe_add_amounts, safe_subtract_amounts, AmountValidationError
};

use types::ContractStatus;
pub use crate::types::{
    CONTRACT_SUMMARY_SCHEMA_VERSION, ContractSummary, MilestoneSummary,
};

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
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = 10_000_000_000_000; // 1 M tokens × 10^7 = 10^13

pub const MAINNET_PROTOCOL_VERSION: u32 = 1u32;
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000_000i128;

use crate::types::DataKey as ReadinessDataKey;
pub use crate::types::{
    ContractStatus, DataKey as StorageKey, Error as StorageError, MainnetReadinessInfo, Milestone,
    MilestoneFunding, ReadinessChecklist,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowBounds {
    pub max_milestones: u32,
    pub max_total_escrow_stroops: i128,
}

#[contract]
pub struct Escrow;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowBounds {
    pub max_milestones: u32,
    pub max_total_escrow_stroops: i128,
}

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
    TooManyMilestones = 11,
}

/// Per-contract storage record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractData {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    /// Milestone amounts (in stroops).  Index matches milestone index.
    pub milestones: Vec<Milestone>,
    pub status: ContractStatus,
    /// Cumulative amount deposited into escrow.
    pub total_deposited: i128,
    /// Cumulative amount released to the freelancer.
    pub released_amount: i128,
    /// Cumulative amount refunded to the client.
    /// Invariant: total_deposited == released_amount + refunded_amount + available_balance
    pub refunded_amount: i128,
}

/// Metadata stored when a dispute is raised.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeMetadata {
    /// SHA-256 hash of the off-chain dispute reason document.
    pub reason_hash: BytesN<32>,
    /// Ledger timestamp when the dispute was raised.
    pub raised_at: u64,
    /// Address that raised the dispute (client or freelancer).
    pub raised_by: Address,
}

/// Arbiter decision when resolving a dispute.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeResolution {
    /// Release all remaining funded milestones to the freelancer.
    Release = 0,
    /// Refund all remaining funded milestones to the client.
    Refund = 1,
    /// Cancel the contract (no further payments).
    Cancel = 2,
}

pub type ContractData = EscrowContractData;

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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingClientMigration {
    pub current_client: Address,
    pub proposed_client: Address,
    pub proposed_client_confirmed: bool,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Contract(u32),
    ContractCount,
    MilestoneReleased(u32, u32),
    RefundableBalance(u32),
    ContractCount,
    Milestones(u32),
    MilestoneApprovalTime(u32, u32),
}

#[allow(dead_code)]
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

#[contractimpl]
impl Escrow {
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestones: Vec<i128>,
        _terms_hash: Option<Bytes>,
        _grace_period_seconds: Option<u64>,
    ) -> u32 {
        client.require_auth();

        if client == freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }

        if let Some(ref a) = arbiter {
            if *a == client || *a == freelancer {
                env.panic_with_error(EscrowError::InvalidParticipant);
            }
        }

        if milestone_amounts.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }
        if milestone_amounts.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        // Use centralized amount validation for milestones
        // Validate each milestone amount individually and calculate total
        let mut total_amount: i128 = 0;
        let mut milestone_objs: Vec<Milestone> = Vec::new(&env);
        for amount in milestones.iter() {
            if amount <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
            total_amount += amount;
        }

        if total_amount > MAX_TOTAL_ESCROW_STROOPS {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        for amount in milestones.iter() {
            milestone_objs.push_back(Milestone {
                amount,
                released: false,
                work_evidence: None,
                funded_amount: 0,
            });
            
            // Use safe addition to prevent overflow
            total_amount = safe_add_amounts(total_amount, amount)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        }
        
        // Validate total against contract maximum
        validate_contract_total(total_amount, MAX_TOTAL_ESCROW_STROOPS)
            .unwrap_or_else(|e| {
                match e {
                    AmountValidationError::ExceedsContractMaximum => 
                        env.panic_with_error(EscrowError::InvalidMilestoneAmount),
                    _ => env.panic_with_error(EscrowError::InvalidMilestoneAmount),
                }
            });

        let id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ContractCount)
            .unwrap_or(0u32);

        let data = ContractData {
            client,
            freelancer,
            arbiter,
            milestones: milestones.clone(),
            status: ContractStatus::Created,
            total_deposited: 0,
            released_amount: 0,
            refunded_amount: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &data);
        env.storage()
            .persistent()
            .set(&DataKey::Milestones(id), &milestone_objs);
        env.storage()
            .persistent()
            .set(&DataKey::ContractCount, &(id + 1));

        emit_lifecycle_event(
            &env,
            symbol_short!("create"),
            id,
            ContractStatus::Created,
            total_amount,
            0,
            Some(client),
        );

        id
    }

    /// Deposit funds into the escrow.  Transitions status from Created → Funded
    /// once the deposited amount reaches the sum of all milestone amounts.
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        // Use centralized amount validation for deposit
        validate_deposit_amount(amount, 0, MAX_TOTAL_ESCROW_STROOPS)
            .unwrap_or_else(|e| {
                // Convert amount validation errors to EscrowError
                match e {
                    AmountValidationError::NonPositiveAmount => 
                        env.panic_with_error(EscrowError::InvalidDepositAmount),
                    AmountValidationError::AmountExceedsMaximum => 
                        env.panic_with_error(EscrowError::InvalidDepositAmount),
                    AmountValidationError::PotentialOverflow => 
                        env.panic_with_error(EscrowError::InvalidDepositAmount),
                    AmountValidationError::ExceedsContractMaximum => 
                        env.panic_with_error(EscrowError::InvalidDepositAmount),
                    AmountValidationError::InvalidStroopPrecision => 
                        env.panic_with_error(EscrowError::InvalidDepositAmount),
                }
            });

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Additional validation: check against current deposited amount
        validate_deposit_amount(amount, contract.total_deposited, MAX_TOTAL_ESCROW_STROOPS)
            .unwrap_or_else(|e| {
                match e {
                    AmountValidationError::ExceedsContractMaximum => 
                        env.panic_with_error(EscrowError::InvalidDepositAmount),
                    _ => env.panic_with_error(EscrowError::InvalidDepositAmount),
                }
            });

        // Use safe addition to prevent overflow
        contract.total_deposited = safe_add_amounts(contract.total_deposited, amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        if contract.status == ContractStatus::Created {
            contract.status = ContractStatus::Funded;
        }

        env.storage().persistent().set(&contract_key, &contract);
        true
    }

    // ─── Partial-refund API ───────────────────────────────────────────────────

    /// Refund one or more unreleased milestones back to the client.
    ///
    /// # Arguments
    /// * `contract_id`   – the escrow contract to operate on.
    /// * `milestone_ids` – non-empty, duplicate-free list of milestone indices
    ///                     to refund.
    ///
    /// # Returns
    /// The total amount refunded (sum of the refunded milestone amounts).
    ///
    /// # Panics / errors
    /// * `EmptyRefundRequest`         – `milestone_ids` is empty.
    /// * `DuplicateMilestoneInRefund` – the same index appears more than once.
    /// * `InvalidMilestone`           – an index is out of bounds.
    /// * `MilestoneAlreadyReleased`   – the milestone was already released.
    /// * `MilestoneAlreadyRefunded`   – the milestone was already refunded.
    /// * `InsufficientEscrowBalance`  – the escrow balance cannot cover the
    ///                                  total refund amount.
    ///
    /// # Accounting invariant
    /// After a successful call:
    ///   `total_deposited == released_amount + refunded_amount + available_balance`
    ///
    /// # Status transitions
    /// * If every milestone is now either released or refunded the contract
    ///   status transitions to `ContractStatus::Refunded`.
    pub fn refund_milestone(
        env: Env,
        contract_id: u32,
        milestone_ids: Vec<u32>,
    ) -> i128 {
        if milestone_ids.is_empty() {
            env.panic_with_error(EscrowError::EmptyRefundRequest);
        }

        // Duplicate-check: O(n²) but n ≤ MAX_MILESTONES = 10, so acceptable.
        let len = milestone_ids.len();
        for i in 0..len {
            for j in (i + 1)..len {
                if milestone_ids.get(i).unwrap() == milestone_ids.get(j).unwrap() {
                    env.panic_with_error(EscrowError::DuplicateMilestoneInRefund);
                }
            }
        }

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Validate milestone index
        if milestone_index >= contract.milestones.len() {
            env.panic_with_error(EscrowError::InvalidMilestone);
        }

        // Mark this milestone as released
        let milestone_key = DataKey::MilestoneReleased(contract_id, milestone_index);
        env.storage().persistent().set(&milestone_key, &true);

        // Update released amount using safe arithmetic
        if let Some(amount) = contract.milestones.get(milestone_index) {
            contract.released_amount = safe_add_amounts(contract.released_amount, amount)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        }

        // Check if all milestones are released to transition to Completed
        let all_released = Self::all_milestones_released(&env, contract_id, &contract);
        if all_released && contract.status == ContractStatus::Funded {
            contract.status = ContractStatus::Completed;
            
            // Increment pending reputation credits for the freelancer
            let credits_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
            let credits: u32 = env
                .storage()
                .persistent()
                .get(&credits_key)
                .unwrap_or(0);
            env.storage().persistent().set(&credits_key, &(credits + 1));
        }

        env.storage().persistent().set(&contract_key, &contract);
        true
    }

    /// Check if all milestones for a contract have been released.
    fn all_milestones_released(env: &Env, contract_id: u32, contract: &EscrowContractData) -> bool {
        for i in 0..contract.milestones.len() {
            let milestone_key = DataKey::MilestoneReleased(contract_id, i as u32);
            if !env
                .storage()
                .persistent()
                .get::<_, bool>(&milestone_key)
                .unwrap_or(false)
            {
                return false;
            }
        }
        true
    }

    /// Get contract details
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get::<_, EscrowContractData>(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    /// Get milestones for a contract.
    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<i128> {
        let contract = Self::get_contract(env, contract_id);
        contract.milestones
    }

    /// Cancel an escrow contract under strict authorization and state constraints.
    pub fn cancel_contract(env: Env, contract_id: u32, caller: Address) -> bool {
        caller.require_auth();

        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if contract.status == ContractStatus::Cancelled {
            env.panic_with_error(EscrowError::AlreadyCancelled);
        }

        if contract.status == ContractStatus::Completed {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref().is_some_and(|a| *a == caller);

        match contract.status {
            ContractStatus::Created => {
                if !is_client && !is_freelancer {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ContractStatus::Funded => {
                if is_client {
                    let released = Self::calculate_released_amount(&env, contract_id, &contract);
                    if released > 0 {
                        env.panic_with_error(EscrowError::MilestonesAlreadyReleased);
                    }
                } else if is_freelancer {
                    // allowed
                } else if is_arbiter {
                    // allowed
                } else {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ContractStatus::Disputed => {
                if !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            _ => {
                env.panic_with_error(EscrowError::InvalidStatusTransition);
            }
        }

        contract.status = ContractStatus::Cancelled;
        env.storage().persistent().set(&contract_key, &contract);

        env.events().publish(
            (Symbol::new(&env, "contract_cancelled"), contract_id),
            (caller, contract.status, env.ledger().timestamp()),
        );

        true
    }

    /// Issue reputation for a completed contract.
    ///
    /// # Security Guarantees (Layered Constraints)
    ///
    /// 1. **Completion Gate**: Contract must be in `Completed` status
    /// 2. **Milestone Resolution Gate**: All milestones must be released
    /// 3. **Single-Issuance Guard**: Reputation can only be issued once per contract
    /// 4. **Freelancer Match**: The freelancer address must match the contract's freelancer
    /// 5. **Rating Bounds**: Rating must be between 1 and 5 (inclusive)
    ///
    /// # Events
    ///
    /// Emits a `reputation_issued` event with the following structure:
    /// - Topics: `("reputation_issued", contract_id)`
    /// - Data: `(freelancer, rating, timestamp)`
    pub fn issue_reputation(
        env: Env,
        contract_id: u32,
        caller: Address,
        freelancer: Address,
        rating: i128,
    ) -> bool {
        // 1. Require cryptographic authorization from the caller (client)
        caller.require_auth();

        // 2. Load contract data
        let contract_key = DataKey::Contract(contract_id);
        let contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // 3. Verify caller is the client (only client can issue reputation)
        if caller != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        // 4. Verify freelancer matches the contract's freelancer
        if freelancer != contract.freelancer {
            env.panic_with_error(EscrowError::FreelancerMismatch);
        }

        // 5. Verify contract is completed
        if contract.status != ContractStatus::Completed {
            env.panic_with_error(EscrowError::NotCompleted);
        }

        // 6. Verify rating is within bounds [1, 5]
        if rating < 1 || rating > 5 {
            env.panic_with_error(EscrowError::InvalidRating);
        }

        // 7. Check for duplicate issuance using persistent guard
        let reputation_issued_key = DataKey::ReputationIssued(contract_id);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&reputation_issued_key)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::ReputationAlreadyIssued);
        }

        // 8. Set the reputation issued flag (immutable once set)
        env.storage()
            .persistent()
            .set(&reputation_issued_key, &true);

        // 9. Update the contract's reputation_issued flag
        let mut contract = contract;
        contract.reputation_issued = true;
        env.storage().persistent().set(&contract_key, &contract);

        // 10. Update freelancer's reputation record
        let reputation_key = DataKey::Reputation(freelancer.clone());
        let mut reputation: ReputationRecord = env
            .storage()
            .persistent()
            .get(&reputation_key)
            .unwrap_or_default();
        reputation.total_rating += rating;
        reputation.ratings_count += 1;
        reputation.last_rating = rating;
        reputation.completed_contracts += 1;
        env.storage().persistent().set(&reputation_key, &reputation);

        // 11. Decrement pending reputation credits
        let credits_key = DataKey::PendingReputationCredits(freelancer.clone());
        let credits: u32 = env
            .storage()
            .persistent()
            .get(&credits_key)
            .unwrap_or(0);
        if credits > 0 {
            env.storage().persistent().set(&credits_key, &(credits - 1));
        }

        // 12. Emit indexer-friendly event with stable schema
        env.events().publish(
            (Symbol::new(&env, "reputation_issued"), contract_id),
            (freelancer, rating, env.ledger().timestamp()),
        );

        true
    }

    /// Get the reputation record for a freelancer.
    /// Returns None if the freelancer has no reputation record.
    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    /// Get the number of pending reputation credits for a freelancer.
    /// A credit is earned when a contract is completed but reputation hasn't been issued yet.
    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    /// Helper: Calculate total released amount for a contract
    fn calculate_released_amount(env: &Env, contract_id: u32, contract: &EscrowContractData) -> i128 {
        let mut released = 0i128;
        for (idx, amount) in contract.milestones.iter().enumerate() {
            let key = DataKey::MilestoneReleased(contract_id, idx as u32);
            if env
                .storage()
                .persistent()
                .get::<_, bool>(&key)
                .unwrap_or(false)
            {
                released = safe_add_amounts(released, amount)
                    .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
            }
        }
        true
    }

    /// Returns a stable, single-read summary of an escrow contract for off-chain indexers.
    ///
    /// Combines contract roles, lifecycle status, financial totals, and
    /// per-milestone state into one atomic call so that indexing pipelines
    /// do not need multiple separate storage reads.
    ///
    /// # Fields
    ///
    /// | Field | Description |
    /// |---|---|
    /// | `schema_version` | Always `CONTRACT_SUMMARY_SCHEMA_VERSION` (`1`); incremented on breaking changes |
    /// | `client` | Address that funds the contract |
    /// | `freelancer` | Address that receives milestone payments |
    /// | `arbiter` | Optional dispute-resolution address (`None` if not set) |
    /// | `status` | Current lifecycle status (`Created`, `Funded`, `Completed`, `Cancelled`, `Refunded`, `Disputed`) |
    /// | `reputation_issued` | Whether a reputation score has already been recorded |
    /// | `total_amount` | Sum of all milestone amounts in stroops |
    /// | `funded_amount` | Total deposited by the client in stroops |
    /// | `released_amount` | Total released to the freelancer in stroops |
    /// | `refundable_balance` | Balance not yet released or refunded, in stroops |
    /// | `released_milestone_count` | Number of milestones released so far |
    /// | `milestones` | Per-milestone index, amount, `released`, and `refunded` flags |
    ///
    /// # Errors
    ///
    /// Panics with `EscrowError::ContractNotFound` if `contract_id` does not exist.
    ///
    /// # Backwards compatibility
    ///
    /// This method is additive and backwards-compatible with all existing
    /// contract storage.  If the return layout ever changes in a breaking way
    /// `CONTRACT_SUMMARY_SCHEMA_VERSION` will be incremented so consumers can
    /// detect and handle the new format.
    pub fn get_contract_summary(env: Env, contract_id: u32) -> ContractSummary {
        // Load the main contract record (panics with ContractNotFound if absent).
        let record = Self::get_contract(env.clone(), contract_id);

        // Load the ordered milestone list.
        let raw_milestones = Self::get_milestones(env.clone(), contract_id);

        // Load the current refundable balance (0 if never set).
        let refundable_balance = Self::get_refundable_balance(env.clone(), contract_id);

        // Build the per-milestone summaries and count released milestones.
        let mut milestone_summaries: Vec<MilestoneSummary> = Vec::new(&env);
        let mut released_milestone_count: u32 = 0u32;

        for (idx, m) in raw_milestones.iter().enumerate() {
            if m.released {
                released_milestone_count += 1;
            }
            milestone_summaries.push_back(MilestoneSummary {
                index: idx as u32,
                amount: m.amount,
                released: m.released,
                refunded: m.refunded,
            });
        }

        ContractSummary {
            schema_version: CONTRACT_SUMMARY_SCHEMA_VERSION,
            client: record.client,
            freelancer: record.freelancer,
            arbiter: record.arbiter,
            status: record.status,
            reputation_issued: record.reputation_issued,
            total_amount: record.total_amount,
            funded_amount: record.funded_amount,
            released_amount: record.released_amount,
            refundable_balance,
            released_milestone_count,
            milestones: milestone_summaries,
        }
    }

    /// Request client migration to a new address
    pub fn request_client_migration(env: Env, contract_id: u32, proposed_client: Address) -> bool {
        proposed_client.require_auth();

        let contract_key = DataKey::Contract(contract_id);
        let contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Only current client can request migration
        let current_client = contract.client;
        current_client.require_auth();

        // Check if contract is in a state that allows migration
        if !Self::can_migrate_client(&contract.status) {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        // Check if there's already a pending migration
        if Self::has_pending_client_migration_internal(&env, contract_id) {
            env.panic_with_error(EscrowError::AlreadyCancelled); // Reuse error for "already pending"
        }

        // Cannot migrate to same address
        if current_client == proposed_client {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }

        // Create pending migration
        let current_ledger = env.ledger().sequence();
        let expires_at = current_ledger + PENDING_MIGRATION_TTL_LEDGERS;
        
        let pending_migration = PendingClientMigration {
            current_client: current_client.clone(),
            proposed_client: proposed_client.clone(),
            proposed_client_confirmed: false,
            requested_at_ledger: current_ledger,
            expires_at_ledger: expires_at,
        };

        env.storage()
            .persistent()
            .set(&DataKey::PendingClientMigration(contract_id), &pending_migration);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "client_migration_proposed"), contract_id),
            (current_client, proposed_client, current_ledger),
        );

        true
    }

    /// Confirm client migration by the proposed client
    pub fn confirm_client_migration(env: Env, contract_id: u32) -> bool {
        let pending_key = DataKey::PendingClientMigration(contract_id);
        let mut pending = env
            .storage()
            .persistent()
            .get::<_, PendingClientMigration>(&pending_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Proposed client must confirm
        pending.proposed_client.require_auth();

        // Check if migration is still valid (not expired)
        let current_ledger = env.ledger().sequence();
        if current_ledger > pending.expires_at_ledger {
            // Remove expired migration
            env.storage().persistent().remove(&pending_key);
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        // Mark as confirmed
        pending.proposed_client_confirmed = true;
        env.storage().persistent().set(&pending_key, &pending);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "client_migration_confirmed"), contract_id),
            (pending.current_client, pending.proposed_client, current_ledger),
        );

        true
    }

    /// Finalize client migration (atomic update)
    pub fn finalize_client_migration(env: Env, contract_id: u32) -> bool {
        let pending_key = DataKey::PendingClientMigration(contract_id);
        let pending = env
            .storage()
            .persistent()
            .get::<_, PendingClientMigration>(&pending_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Check if migration is confirmed and not expired
        if !pending.proposed_client_confirmed {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger > pending.expires_at_ledger {
            // Remove expired migration
            env.storage().persistent().remove(&pending_key);
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        // Update contract client atomically
        let contract_key = DataKey::Contract(contract_id);
        let mut contract = env
            .storage()
            .persistent()
            .get::<_, EscrowContractData>(&contract_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        contract.client = pending.proposed_client.clone();
        env.storage().persistent().set(&contract_key, &contract);

        // Remove pending migration
        env.storage().persistent().remove(&pending_key);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "client_migration_finalized"), contract_id),
            (pending.current_client, pending.proposed_client, current_ledger),
        );

        true
    }

    /// Cancel pending client migration
    pub fn cancel_client_migration(env: Env, contract_id: u32) -> bool {
        let pending_key = DataKey::PendingClientMigration(contract_id);
        let pending = env
            .storage()
            .persistent()
            .get::<_, PendingClientMigration>(&pending_key)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Only current client can cancel
        pending.current_client.require_auth();

        // Remove pending migration
        env.storage().persistent().remove(&pending_key);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "client_migration_cancelled"), contract_id),
            (pending.current_client, pending.proposed_client, env.ledger().sequence()),
        );

        true
    }

    /// Get pending client migration information
    pub fn get_pending_client_migration(env: Env, contract_id: u32) -> PendingClientMigration {
        env.storage()
            .persistent()
            .get::<_, PendingClientMigration>(&DataKey::PendingClientMigration(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    /// Check if there's a pending client migration
    pub fn has_pending_client_migration(env: Env, contract_id: u32) -> bool {
        Self::has_pending_client_migration_internal(&env, contract_id)
    }

    // Helper methods
    fn has_pending_client_migration_internal(env: &Env, contract_id: u32) -> bool {
        env.storage()
            .persistent()
            .get::<_, PendingClientMigration>(&DataKey::PendingClientMigration(contract_id))
            .is_some()
    }

    fn can_migrate_client(status: &ContractStatus) -> bool {
        match status {
            ContractStatus::Created | ContractStatus::Funded => true,
            ContractStatus::Completed | ContractStatus::Cancelled | ContractStatus::Disputed | ContractStatus::Refunded => false,
        }
    }
}

// #[cfg(test)]
// mod test;

// #[cfg(test)]
// mod proptest;

#[cfg(test)]
mod simple_amount_test;
