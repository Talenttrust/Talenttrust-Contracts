#![no_std]

//! ## Mainnet readiness
//!
//! - [`Escrow::get_mainnet_readiness_info`] returns protocol version, the non-governable per-contract
//!   total cap, and governed validation fields (same as [`ProtocolParameters`], flattened for Soroban).
//! - Contract events use topic prefix `tt_esc` with `create`, `deposit`, or `release` for indexer hooks.
//! - Reviewer checklist and residual risks: `docs/escrow/mainnet-readiness.md`.

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec};

const DEFAULT_MIN_MILESTONE_AMOUNT: i128 = 1;
const DEFAULT_MAX_MILESTONES: u32 = 16;
const DEFAULT_MIN_REPUTATION_RATING: i128 = 1;
const DEFAULT_MAX_REPUTATION_RATING: i128 = 5;

/// Reported deployment version for operators (`major * 1_000_000 + minor * 1_000 + patch`).
pub const MAINNET_PROTOCOL_VERSION: u32 = 1_000_000;

/// Hard ceiling on the sum of milestone amounts per escrow (stroops). Not governed; change only via wasm upgrade.
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000;

/// Persistent lifecycle state for an escrow agreement.
///
/// Security notes:
/// - Only `Created -> Funded -> Completed` transitions are currently supported.
/// - `Disputed` is reserved for future dispute resolution flows and is not reachable
///   in the current implementation.

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
}

/// Individual milestone tracked inside an escrow agreement.
///
/// Invariant:
/// - `released == true` is irreversible.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
}

/// Stored escrow state for a single agreement.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<Milestone>,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub status: ContractStatus,
    /// Total amount deposited by the client so far.
    pub deposited_amount: i128,
}

// ---------------------------------------------------------------------------
// Release-readiness checklist
// ---------------------------------------------------------------------------

/// Tracks whether each deployment, verification, and post-deploy monitoring
/// gate has been satisfied for a specific escrow contract.
///
/// Items are **automatically** updated by contract operations -- no external
/// caller may set them directly, preventing unauthorized state manipulation.
///
/// # Phases
///
/// **Deployment**
/// - `contract_created` -- set when `create_contract` succeeds.
/// - `funds_deposited`  -- set when `deposit_funds` succeeds with amount > 0.
///
/// **Verification**
/// - `parties_authenticated` -- set at contract creation (both addresses recorded).
/// - `milestones_defined`    -- set at contract creation when >= 1 milestone exists.
///
/// **Post-Deploy Monitoring**
/// - `all_milestones_released` -- set when the final milestone is released.
/// - `reputation_issued`       -- set when `issue_reputation` is called.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ReleaseChecklist {
    // -- Deployment --
    /// Contract has been successfully created and persisted.
    pub contract_created: bool,
    /// Client has deposited a positive amount into escrow.
    pub funds_deposited: bool,

    // -- Verification --
    /// Both client and freelancer addresses have been recorded.
    pub parties_authenticated: bool,
    /// At least one milestone amount has been defined.
    pub milestones_defined: bool,

    // -- Post-Deploy Monitoring --
    /// Every milestone in the agreement has been released.
    pub all_milestones_released: bool,
    /// A reputation credential has been issued for the freelancer.
    pub reputation_issued: bool,
}

/// On-chain summary for mainnet deployment review and monitoring integration.
/// Fields mirror [`ProtocolParameters`] without nesting (Soroban SDK nesting limits).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainnetReadinessInfo {
    pub protocol_version: u32,
    pub max_escrow_total_stroops: i128,
    pub min_milestone_amount: i128,
    pub max_milestones: u32,
    pub min_reputation_rating: i128,
    pub max_reputation_rating: i128,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    NextContractId,
    Contract(u32),
    Reputation(Address),
    PendingReputationCredits(Address),
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Initializes protocol governance and stores the first guarded parameter set.
    ///
    /// Security properties:
    /// - Initialization is one-time only.
    /// - The initial admin must authorize the call.
    /// - Parameters are validated before storage.
    pub fn initialize_protocol_governance(
        env: Env,
        admin: Address,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        admin.require_auth();

        if env.storage().persistent().has(&DataKey::GovernanceAdmin) {
            panic!("protocol governance already initialized");
        }

        let parameters = Self::validated_protocol_parameters(
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );

        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &parameters);

        true
    }

    /// Updates governed protocol parameters.
    ///
    /// Security properties:
    /// - Only the current governance admin may update parameters.
    /// - Parameters are atomically replaced after validation.
    pub fn update_protocol_parameters(
        env: Env,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        let admin = Self::governance_admin(&env);
        admin.require_auth();

        let parameters = Self::validated_protocol_parameters(
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );

        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &parameters);

        true
    }

    /// Proposes a governance admin transfer. The new admin must later accept it.
    ///
    /// Security properties:
    /// - Only the current governance admin may nominate a successor.
    /// - The current admin cannot nominate itself.
    pub fn propose_governance_admin(env: Env, new_admin: Address) -> bool {
        let admin = Self::governance_admin(&env);
        admin.require_auth();

        if new_admin == admin {
            panic!("new admin must differ from current admin");
        }

        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &new_admin);

        true
    }

    /// Accepts a pending governance admin transfer.
    ///
    /// Security properties:
    /// - Only the nominated pending admin may accept the transfer.
    /// - Pending state is cleared when the transfer completes.
    pub fn accept_governance_admin(env: Env) -> bool {
        let pending_admin = Self::pending_governance_admin(&env)
            .unwrap_or_else(|| panic!("no pending governance admin"));
        pending_admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &pending_admin);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingGovernanceAdmin);

        true
    }

    /// Creates a new escrow contract and stores milestone funding requirements.
    ///
    /// Security properties:
    /// - The declared client must authorize creation.
    /// - Client and freelancer addresses must be distinct.
    /// - All milestones must have a strictly positive amount.
    /// - Funding amount is fixed at creation time by the milestone sum.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
    ) -> u32 {
        client.require_auth();

        if client == freelancer {
            panic!("client and freelancer must differ");
        }
        if milestone_amounts.is_empty() {
            panic!("at least one milestone is required");
        }

        let protocol_parameters = Self::protocol_parameters(&env);
        if milestone_amounts.len() > protocol_parameters.max_milestones {
            panic!("milestone count exceeds governed limit");
        }

        let mut milestones = Vec::new(&env);
        let mut total_amount = 0_i128;
        let mut index = 0_u32;
        while index < milestone_amounts.len() {
            let amount = milestone_amounts
                .get(index)
                .unwrap_or_else(|| panic!("missing milestone amount"));
            if amount < protocol_parameters.min_milestone_amount {
                panic!("milestone amount below governed minimum");
            }
            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic!("milestone total overflow"));
            milestones.push_back(Milestone {
                amount,
                released: false,
            });
            index += 1;
        }

        if total_amount > MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS {
            panic!("total escrow exceeds mainnet hard cap");
        }

        let contract_id = Self::next_contract_id(&env);
        let contract = EscrowContractData {
            client,
            freelancer,
            milestones,
            status: ContractStatus::Created,
            deposited_amount: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &data);

        // Initialise checklist -- deployment + verification items are satisfied
        // by the act of calling this function successfully.
        let checklist = ReleaseChecklist {
            contract_created: true,
            funds_deposited: false,
            parties_authenticated: true,
            milestones_defined: true,
            all_milestones_released: false,
            reputation_issued: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(contract_id + 1));

        env.events().publish(
            (symbol_short!("tt_esc"), symbol_short!("create")),
            (contract_id, total_amount),
        );

        id
    }

    /// Deposits the full escrow amount for a contract.
    ///
    /// Security properties:
    /// - Only the recorded client may fund the contract.
    /// - Funding is allowed exactly once.
    /// - Partial or excess funding is rejected to avoid ambiguous release logic.
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        if amount <= 0 {
            panic!("deposit amount must be positive");
        }

        let mut contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();

        if contract.status != ContractStatus::Created {
            panic!("contract is not awaiting funding");
        }
        if amount != contract.total_amount {
            panic!("deposit must match milestone total");
        }

        contract.funded_amount = amount;
        contract.status = ContractStatus::Funded;
        Self::save_contract(&env, contract_id, &contract);

        env.events().publish(
            (symbol_short!("tt_esc"), symbol_short!("deposit")),
            (contract_id, amount),
        );

        true
    }

    /// Releases a single milestone payment.
    ///
    /// Security properties:
    /// - Only the client may authorize a release.
    /// - Milestones can be released once.
    /// - Contract completion is derived from all milestones being released.
    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        let mut contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();

        if contract.status != ContractStatus::Funded {
            panic!("contract is not funded");
        }
        if milestone_id >= contract.milestones.len() {
            panic!("milestone id out of range");
        }

        let mut milestone = contract
            .milestones
            .get(milestone_id)
            .unwrap_or_else(|| panic!("missing milestone"));
        if milestone.released {
            panic!("milestone already released");
        }

        let released_stroops = milestone.amount;

        let next_released_amount = contract
            .released_amount
            .checked_add(milestone.amount)
            .unwrap_or_else(|| panic!("released total overflow"));
        if next_released_amount > contract.funded_amount {
            panic!("release exceeds funded amount");
        }
        milestone.released = true;
        data.milestones.set(milestone_id, milestone);

        milestone.released = true;
        contract.milestones.set(milestone_id, milestone);
        contract.released_amount = next_released_amount;

        if Self::all_milestones_released(&contract.milestones) {
            contract.status = ContractStatus::Completed;
            Self::add_pending_reputation_credit(&env, &contract.freelancer);
        }

        Self::save_contract(&env, contract_id, &contract);

        env.events().publish(
            (symbol_short!("tt_esc"), symbol_short!("release")),
            (contract_id, milestone_id, released_stroops),
        );

        true
    }

    /// Issues a bounded reputation rating for a freelancer after a completed contract.
    ///
    /// Security properties:
    /// - The freelancer must authorize the write to their own reputation record.
    /// - A reputation update is only possible after a completed contract grants a
    ///   pending reputation credit.
    /// - Ratings are limited to the inclusive range `1..=5`.
    ///
    /// Residual risk:
    /// - The current interface lets the freelancer self-submit the rating value.
    ///   The contract therefore treats this record as informational only and does
    ///   not use it for fund movement or access control.
    pub fn issue_reputation(env: Env, freelancer: Address, rating: i128) -> bool {
        freelancer.require_auth();

        let protocol_parameters = Self::protocol_parameters(&env);
        if rating < protocol_parameters.min_reputation_rating
            || rating > protocol_parameters.max_reputation_rating
        {
            panic!("rating is outside governed bounds");
        }

        let pending_key = DataKey::PendingReputationCredits(freelancer.clone());
        let pending_credits = env
            .storage()
            .persistent()
            .get::<_, u32>(&pending_key)
            .unwrap_or(0);
        if pending_credits == 0 {
            panic!("no completed contract available for reputation");
        }

        let rep_key = DataKey::Reputation(freelancer.clone());
        let mut record = env
            .storage()
            .persistent()
            .get::<_, ReputationRecord>(&rep_key)
            .unwrap_or(ReputationRecord {
                completed_contracts: 0,
                total_rating: 0,
                last_rating: 0,
            });

        record.completed_contracts += 1;
        record.total_rating = record
            .total_rating
            .checked_add(rating)
            .unwrap_or_else(|| panic!("rating total overflow"));
        record.last_rating = rating;

        env.storage().persistent().set(&rep_key, &record);
        env.storage()
            .persistent()
            .set(&pending_key, &(pending_credits - 1));

        true
    }

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Returns the stored contract state.
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::load_contract(&env, contract_id)
    }

    /// Returns the stored reputation record for a freelancer, if present.
    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    /// Returns the number of pending reputation updates that can be claimed.
    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    /// Returns the active protocol parameters.
    ///
    /// If governance has not been initialized yet, this returns the safe default
    /// parameters baked into the contract.
    pub fn get_protocol_parameters(env: Env) -> ProtocolParameters {
        Self::protocol_parameters(&env)
    }

    /// Returns the current governance admin, if governance has been initialized.
    pub fn get_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::GovernanceAdmin)
    }

    /// Returns the pending governance admin, if an admin transfer is in flight.
    pub fn get_pending_governance_admin(env: Env) -> Option<Address> {
        Self::pending_governance_admin(&env)
    }

    /// Aggregates immutable caps, protocol version, and current governed parameters for mainnet readiness review.
    pub fn get_mainnet_readiness_info(env: Env) -> MainnetReadinessInfo {
        let p = Self::protocol_parameters(&env);
        MainnetReadinessInfo {
            protocol_version: MAINNET_PROTOCOL_VERSION,
            max_escrow_total_stroops: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
            min_milestone_amount: p.min_milestone_amount,
            max_milestones: p.max_milestones,
            min_reputation_rating: p.min_reputation_rating,
            max_reputation_rating: p.max_reputation_rating,
        }
    }
}

impl Escrow {
    fn next_contract_id(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    }

    fn load_contract(env: &Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| panic!("contract not found"))
    }

    fn save_contract(env: &Env, contract_id: u32, contract: &EscrowContractData) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), contract);
    }

    fn add_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let key = DataKey::PendingReputationCredits(freelancer.clone());
        let current = env.storage().persistent().get::<_, u32>(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + 1));
    }

    fn governance_admin(env: &Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::GovernanceAdmin)
            .unwrap_or_else(|| panic!("protocol governance is not initialized"))
    }

    fn pending_governance_admin(env: &Env) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingGovernanceAdmin)
    }

    fn protocol_parameters(env: &Env) -> ProtocolParameters {
        env.storage()
            .persistent()
            .get(&DataKey::ProtocolParameters)
            .unwrap_or_else(Self::default_protocol_parameters)
    }

    fn default_protocol_parameters() -> ProtocolParameters {
        ProtocolParameters {
            min_milestone_amount: DEFAULT_MIN_MILESTONE_AMOUNT,
            max_milestones: DEFAULT_MAX_MILESTONES,
            min_reputation_rating: DEFAULT_MIN_REPUTATION_RATING,
            max_reputation_rating: DEFAULT_MAX_REPUTATION_RATING,
        }
    }

    fn validated_protocol_parameters(
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> ProtocolParameters {
        if min_milestone_amount <= 0 {
            panic!("minimum milestone amount must be positive");
        }
        if max_milestones == 0 {
            panic!("maximum milestones must be positive");
        }
        if min_reputation_rating <= 0 {
            panic!("minimum reputation rating must be positive");
        }
        if min_reputation_rating > max_reputation_rating {
            panic!("reputation rating range is invalid");
        }

        ProtocolParameters {
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        }
    }

    fn all_milestones_released(milestones: &Vec<Milestone>) -> bool {
        let mut index = 0_u32;
        while index < milestones.len() {
            let milestone = milestones
                .get(index)
                .unwrap_or_else(|| panic!("missing milestone"));
            if !milestone.released {
                return false;
            }
            index += 1;
        }
        true
    }
}

#[cfg(test)]
mod test;