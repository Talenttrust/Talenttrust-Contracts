#![no_std]

//! ## Mainnet readiness
//!
//! - [`Escrow::get_mainnet_readiness_info`] returns protocol version, the non-governable per-contract
//!   total cap, and governed validation fields (same as [`ProtocolParameters`], flattened for Soroban).
//! - Contract events use topic prefix `tt_esc` with `create`, `deposit`, or `release` for indexer hooks.
//! - Reviewer checklist and residual risks: `docs/escrow/mainnet-readiness.md`.

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec};

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

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
    Refunded = 4,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowError {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    ContractNotFound = 4,
    InvalidDepositAmount = 5,
    DepositExceedsTotal = 6,
    InvalidMilestone = 7,
    MilestoneAlreadyReleased = 8,
    MilestoneAlreadyRefunded = 9,
    InsufficientEscrowBalance = 10,
    InvalidStatus = 11,
    EmptyRefundRequest = 12,
    DuplicateMilestone = 13,
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
    pub refunded: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub status: ContractStatus,
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
    Contract(u32),
    Milestones(u32),
    ContractCount,
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

#[contractimpl]
impl Escrow {
    /// Creates a new escrow agreement with milestone amounts that can later be
    /// released to the freelancer or refunded back to the client.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
    ) -> u32 {
        client.require_auth();

        if client == freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }
        if milestone_amounts.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }

        let mut total_amount = 0_i128;
        let mut milestones = Vec::new(&env);

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

        let contract_id = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::ContractCount)
            .unwrap_or(0)
            + 1;

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
            .set(&DataKey::Milestones(contract_id), &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::ContractCount, &contract_id);

        id
    }

    /// Deposits additional client funds into escrow.
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        if amount <= 0 {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        let mut contract = Self::get_contract_data(&env, contract_id);
        Self::assert_client_auth(&contract);
        Self::assert_open_status(&env, contract.status);

        let updated_amount = contract.funded_amount + amount;
        if updated_amount > contract.total_amount {
            env.panic_with_error(EscrowError::DepositExceedsTotal);
        }

        contract.funded_amount = updated_amount;
        contract.status =
            Self::derive_status(&contract, &Self::get_milestones_data(&env, contract_id));
        Self::save_contract(&env, contract_id, &contract);

        true
    }

    /// Releases a funded milestone to the freelancer. Only the client may
    /// authorize a release.
    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        let mut contract = Self::get_contract_data(&env, contract_id);
        Self::assert_client_auth(&contract);
        Self::assert_open_status(&env, contract.status);

        let mut milestones = Self::get_milestones_data(&env, contract_id);
        let index = Self::milestone_index(&env, &milestones, milestone_id);
        let mut milestone = milestones.get(index).unwrap();

        if milestone.released {
            env.panic_with_error(EscrowError::MilestoneAlreadyReleased);
        }
        if milestone.refunded {
            env.panic_with_error(EscrowError::MilestoneAlreadyRefunded);
        }
        if Self::escrow_balance(&contract) < milestone.amount {
            env.panic_with_error(EscrowError::InsufficientEscrowBalance);
        }

        milestone.released = true;
        contract.released_amount += milestone.amount;
        milestones.set(index, milestone);

        contract.status = Self::derive_status(&contract, &milestones);
        Self::save_contract(&env, contract_id, &contract);
        Self::save_milestones(&env, contract_id, &milestones);

        true
    }

    /// Refunds selected unreleased milestone balances back to the client.
    ///
    /// The caller must be the contract client. Each requested milestone must be
    /// unique, unreleased, and not previously refunded.
    pub fn refund_unreleased_milestones(
        env: Env,
        contract_id: u32,
        milestone_ids: Vec<u32>,
    ) -> i128 {
        if milestone_ids.is_empty() {
            env.panic_with_error(EscrowError::EmptyRefundRequest);
        }

        let mut contract = Self::get_contract_data(&env, contract_id);
        Self::assert_client_auth(&contract);
        Self::assert_open_status(&env, contract.status);

        let mut milestones = Self::get_milestones_data(&env, contract_id);
        let mut refund_amount = 0_i128;
        let mut seen_ids = Vec::new(&env);

        for milestone_id in milestone_ids.iter() {
            if seen_ids.contains(milestone_id) {
                env.panic_with_error(EscrowError::DuplicateMilestone);
            }
            seen_ids.push_back(milestone_id);

            let index = Self::milestone_index(&env, &milestones, milestone_id);
            let milestone = milestones.get(index).unwrap();

            if milestone.released {
                env.panic_with_error(EscrowError::MilestoneAlreadyReleased);
            }
            if milestone.refunded {
                env.panic_with_error(EscrowError::MilestoneAlreadyRefunded);
            }

            refund_amount += milestone.amount;
        }
        milestone.released = true;
        data.milestones.set(milestone_id, milestone);

        if Self::escrow_balance(&contract) < refund_amount {
            env.panic_with_error(EscrowError::InsufficientEscrowBalance);
        }

        for milestone_id in milestone_ids.iter() {
            let index = Self::milestone_index(&env, &milestones, milestone_id);
            let mut milestone = milestones.get(index).unwrap();
            milestone.refunded = true;
            milestones.set(index, milestone);
        }

        contract.refunded_amount += refund_amount;
        contract.status = Self::derive_status(&contract, &milestones);

        Self::save_contract(&env, contract_id, &contract);
        Self::save_milestones(&env, contract_id, &milestones);

        refund_amount
    }

    /// Returns the full contract state for external inspection and tests.
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::get_contract_data(&env, contract_id)
    }

    /// Returns the milestone list for the specified escrow.
    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<Milestone> {
        Self::get_milestones_data(&env, contract_id)
    }

    /// Returns the currently refundable balance for unreleased milestones.
    pub fn get_refundable_balance(env: Env, contract_id: u32) -> i128 {
        let contract = Self::get_contract_data(&env, contract_id);
        Self::escrow_balance(&contract)
    }

    /// Issue a reputation credential for the freelancer after contract completion.
    pub fn issue_reputation(_env: Env, _freelancer: Address, _rating: i128) -> bool {
        true
    }
  
    #[test]
    #[should_panic(expected = "total_available != total_funded - total_released")]
    fn test_funding_invariants_negative_available() {
        let funding = FundingAccount {
            total_funded: 1000,
            total_released: 400,
            total_available: -100,
        };

    // get_admin already defined above

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    fn get_contract_data(env: &Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    fn get_milestones_data(env: &Env, contract_id: u32) -> Vec<Milestone> {
        env.storage()
            .persistent()
            .get(&DataKey::Milestones(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound))
    }

    fn save_contract(env: &Env, contract_id: u32, contract: &EscrowContractData) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), contract);
    }

    fn save_milestones(env: &Env, contract_id: u32, milestones: &Vec<Milestone>) {
        env.storage()
            .persistent()
            .set(&DataKey::Milestones(contract_id), milestones);
    }

    fn assert_client_auth(contract: &EscrowContractData) {
        contract.client.require_auth();
    }

    fn assert_open_status(env: &Env, status: ContractStatus) {
        if matches!(
            status,
            ContractStatus::Completed | ContractStatus::Disputed | ContractStatus::Refunded
        ) {
            env.panic_with_error(EscrowError::InvalidStatus);
        }
    }

    fn milestone_index(env: &Env, milestones: &Vec<Milestone>, milestone_id: u32) -> u32 {
        if milestone_id >= milestones.len() {
            env.panic_with_error(EscrowError::InvalidMilestone);
        }

        milestone_id
    }

    fn escrow_balance(contract: &EscrowContractData) -> i128 {
        contract.funded_amount - contract.released_amount - contract.refunded_amount
    }

    fn derive_status(contract: &EscrowContractData, milestones: &Vec<Milestone>) -> ContractStatus {
        let mut all_released = true;
        let mut all_resolved = true;
        let mut any_refunded = false;

        for milestone in milestones.iter() {
            if !milestone.released {
                all_released = false;
            }
            if !milestone.released && !milestone.refunded {
                all_resolved = false;
            }
            if milestone.refunded {
                any_refunded = true;
            }
        }

        if all_released {
            ContractStatus::Completed
        } else if all_resolved && any_refunded {
            ContractStatus::Refunded
        } else if contract.funded_amount == contract.total_amount {
            ContractStatus::Funded
        } else {
            ContractStatus::Created
        }
    }
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