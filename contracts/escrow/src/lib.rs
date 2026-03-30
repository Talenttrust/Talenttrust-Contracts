#![no_std]

//! ## Mainnet readiness
//!
//! - [`Escrow::get_mainnet_readiness_info`] returns protocol version, the non-governable per-contract
//!   total cap, and governed validation fields (same as [`ProtocolParameters`], flattened for Soroban).
//! - Contract events use topic prefix `tt_esc` with `create`, `deposit`, or `release` for indexer hooks.
//! - Reviewer checklist and residual risks: `docs/escrow/mainnet-readiness.md`.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, String,
    Symbol, Vec,  symbol_short,
};
#![no_std]

const DEFAULT_MIN_MILESTONE_AMOUNT: i128 = 1;
const DEFAULT_MAX_MILESTONES: u32 = 16;
const DEFAULT_MIN_REPUTATION_RATING: i128 = 1;
const DEFAULT_MAX_REPUTATION_RATING: i128 = 5;

/// Persistent storage keys used by the Escrow contract.
///
/// Security notes:
/// - Only `Created -> Funded -> Completed` transitions are currently supported.
/// - `Disputed` is reserved for future dispute resolution flows and is not reachable
///   in the current implementation.

/// Maximum fee basis points (100% = 10000 basis points)
pub const MAX_FEE_BASIS_POINTS: u32 = 10000;

/// Default protocol fee: 2.5% = 250 basis points
pub const DEFAULT_FEE_BASIS_POINTS: u32 = 250;

/// Default timeout duration: 30 days in seconds (30 * 24 * 60 * 60)
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 2_592_000;

/// Minimum timeout duration: 1 day in seconds
pub const MIN_TIMEOUT_SECONDS: u64 = 86_400;

/// Maximum timeout duration: 365 days in seconds
pub const MAX_TIMEOUT_SECONDS: u64 = 31_536_000;

/// Data keys for contract storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    // Pause controls (instance storage)
    Admin,
    Paused,
    EmergencyPaused,

    // Escrow state (persistent storage)
    NextContractId,
    Contract(u32),

    // Dispute state (persistent storage)
    Dispute(u32),
    MilestoneComplete(u32, u32),
    Paused,
    EmergencyPaused,
    Reputation(Address),
    PendingReputationCredits(Address),
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
}
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
/// Defines the security authorization scheme required to approve and release milestones.
/// Carefully review the threat model associated with each scheme.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<Milestone>,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub released_milestones: u32,
    /// Current lifecycle status of the contract.
    pub status: ContractStatus,
    pub reputation_issued: bool,
}

/// Reputation state derived from completed escrow contracts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub completed_contracts: u32,
    pub total_rating: i128,
    pub last_rating: i128,
}

/// Governed protocol parameters used by the escrow validation logic.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolParameters {
    pub min_milestone_amount: i128,
    pub max_milestones: u32,
    pub min_reputation_rating: i128,
    pub max_reputation_rating: i128,
}

/// Custom errors for the escrow contract.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    InvalidContractId = 1,
    InvalidMilestoneId = 2,
    InvalidAmount = 3,
    InvalidRating = 4,
    EmptyMilestones = 5,
    InvalidParticipants = 6,

    ContractNotFound = 7,
    AmountMustBePositive = 8,
    FundingExceedsRequired = 9,
    InvalidState = 10,
    InsufficientEscrowBalance = 11,
    MilestoneNotFound = 12,
    MilestoneAlreadyReleased = 13,
    ReputationAlreadyIssued = 14,

    DisputeAlreadyOpen = 15,
    NoDisputeActive = 16,
    DisputeNotResolved = 17,
    DisputeAlreadyPaidOut = 18,
    Unauthorized = 19,
}

/// Immutable record created when a dispute is initiated.
/// Written once to persistent storage and never overwritten.
/// Timeout configuration for escrow contracts
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRecord {
    pub initiator: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Evidence attached to an open dispute.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeEvidence {
    pub submitter: Address,
    pub uri: String,
    pub timestamp: u64,
}

/// Resolution outcomes for disputes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeOutcome {
    Client,
    Freelancer,
    Split(i128),
}

/// Resolution record written when the dispute is resolved.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeResolution {
    pub resolver: Address,
    pub outcome: DisputeOutcome,
    pub timestamp: u64,
}

/// Stored dispute state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeState {
    pub record: DisputeRecord,
    pub evidence: Vec<DisputeEvidence>,
    pub resolution: Vec<DisputeResolution>,
    pub paid_out: bool,
// (EscrowContract struct was deleted)
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

impl Escrow {
    fn read_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("Pause controls are not initialized"))
    }

    fn require_admin(env: &Env) {
        let admin = Self::read_admin(env);
        admin.require_auth();
    }

    fn is_paused_internal(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn is_emergency_internal(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::EmergencyPaused)
            .unwrap_or(false)
    }

    fn ensure_not_paused(env: &Env) {
        if Self::is_paused_internal(env) {
            panic!("Contract is paused");
        }
    }

    fn dispute_key(contract_id: u32) -> DataKey {
        DataKey::Dispute(contract_id)
    }

    fn load_dispute(env: &Env, contract_id: u32) -> Option<DisputeState> {
        env.storage().persistent().get(&Self::dispute_key(contract_id))
    }

    fn save_dispute(env: &Env, contract_id: u32, dispute: &DisputeState) {
        env.storage()
            .persistent()
            .set(&Self::dispute_key(contract_id), dispute);
    }
}
/// Default approval/release deadline for each milestone after contract creation.
const DEFAULT_MILESTONE_TIMEOUT_SECS: u64 = 7 * 24 * 60 * 60;

#[contractimpl]
impl Escrow {
    /// Create a new escrow contract with milestone-based release authorization.
    ///
    /// # Panics
    /// - If called more than once.
    pub fn initialize(env: Env, admin: Address) -> bool {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Pause controls already initialized");
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        true
    }

    /// Returns the configured pause-control administrator.
    pub fn get_admin(env: Env) -> Address {
        Self::read_admin(&env)
    }

    /// Pauses state-changing operations for incident response.
    pub fn pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    /// Lifts a normal pause.
    ///
    /// # Panics
    /// - If emergency mode is still active.
    /// - If contract is not paused.
    pub fn unpause(env: Env) -> bool {
        Self::require_admin(&env);

        if Self::is_emergency_internal(&env) {
            panic!("Emergency pause active");
        }
        if !Self::is_paused_internal(&env) {
            panic!("Contract is not paused");
        }

        env.storage().instance().set(&DataKey::Paused, &false);
        true
    }

    /// Activates emergency mode and hard-pauses the contract.
    pub fn activate_emergency_pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &true);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    /// Resolves emergency mode and restores normal operations.
    pub fn resolve_emergency(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        env.storage().instance().set(&DataKey::Paused, &false);
        true
    }

    /// Read-only pause status.
    pub fn is_paused(env: Env) -> bool {
        Self::is_paused_internal(&env)
    }

    /// Read-only emergency status.
    pub fn is_emergency(env: Env) -> bool {
        Self::is_emergency_internal(&env)
    }

    pub fn create_contract(env: Env, client: Address, freelancer: Address, milestones: Vec<i128>) -> u32 {
        Self::ensure_not_paused(&env);
        client.require_auth();

        if client == freelancer {
            panic_with_error!(&env, EscrowError::InvalidParticipants);
        }
        if milestones.is_empty() {
            panic_with_error!(&env, EscrowError::EmptyMilestones);
        }

        let parameters = Self::protocol_parameters(&env);
        if milestones.len() > parameters.max_milestones {
            panic!("too many milestones");
        }

        let mut milestone_vec: Vec<Milestone> = Vec::new(&env);
        let mut total_amount: i128 = 0;
        let mut i = 0_u32;
        while i < milestones.len() {
            let amount = milestones.get(i).unwrap();
            if amount < parameters.min_milestone_amount {
                panic_with_error!(&env, EscrowError::InvalidAmount);
            }
            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount));
            milestone_vec.push_back(Milestone {
    /// Stores the contract record in persistent storage and returns a numeric
    /// identifier derived from the current ledger sequence number.
    ///
    /// # Arguments
    ///
    /// | Name                | Type                    | Description                                      |
    /// |---------------------|-------------------------|--------------------------------------------------|
    /// | `env`               | `Env`                   | Soroban host environment.                        |
    /// | `client`            | `Address`               | Client who will fund the escrow.                 |
    /// | `freelancer`        | `Address`               | Freelancer who will receive milestone payments.  |
    /// | `arbiter`           | `Option<Address>`       | Optional arbiter for disputes / multi-sig.       |
    /// | `milestone_amounts` | `Vec<i128>`             | Ordered list of milestone amounts in stroops.    |
    /// | `release_auth`      | `ReleaseAuthorization`  | Authorization scheme for milestone releases.     |
    ///
    /// # Returns
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
    /// | Condition                                      | Message                                          |
    /// |------------------------------------------------|--------------------------------------------------|
    /// | `milestone_amounts` is empty                   | `"At least one milestone required"`              |
    /// | `client == freelancer`                         | `"Client and freelancer cannot be the same address"` |
    /// | Any milestone amount is `<= 0`                 | `"Milestone amounts must be positive"`           |
//     pub fn create_contract(
//         env: Env,
//         client: Address,
//         freelancer: Address,
//         arbiter: Option<Address>,
//         milestone_amounts: Vec<i128>,
//         release_auth: ReleaseAuthorization,
//     ) -> u32 {
//         if milestone_amounts.is_empty() {
//             panic!("At least one milestone required");
//         }

//         let protocol_params = Self::protocol_parameters(&env);
//         if milestone_amounts.len() > protocol_params.max_milestones {
//             panic!("Exceeds maximum milestone count");
//         }

//         let mut total_amount: i128 = 0;
//         let mut milestones = Vec::new(&env);

//         for i in 0..milestone_amounts.len() {
//             let amount = milestone_amounts.get(i).unwrap();
//             if amount <= 0 {
//                 panic!("Milestone amounts must be positive");
//             }
//             total_amount = total_amount
//                 .checked_add(amount)
//                 .unwrap_or_else(|| panic!("Amount overflow"));

//             milestones.push_back(Milestone {
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

        let contract_id = Self::next_contract_id(&env);
        // Limit contract size conceptually: prevent massive state requirements by bounding total scale
        if total_amount > 1_000_000_000_000_i128 {
            panic!("Exceeds maximum contract funding size");
        if total_amount > MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS {
            panic!("total escrow exceeds mainnet hard cap");
        }

        let contract_id = Self::next_contract_id(&env);
        let contract = EscrowContractData {
            client,
            freelancer,
            milestones,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            status: ContractStatus::Created,
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

        let contract = EscrowContractData {
            client: client.clone(),
            freelancer,
            milestones: milestone_vec,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            released_milestones: 0,
            status: ContractStatus::Created,
            reputation_issued: false,
        };
        Self::save_contract(&env, contract_id, &contract);
        contract_id
    }

    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        Self::ensure_not_paused(&env);
        if amount <= 0 {
            panic_with_error!(&env, EscrowError::AmountMustBePositive);
      }
    /// Deposit the full escrow amount into the contract.
    ///
    /// Only the client may call this function. The deposited amount must equal
    /// the sum of all milestone amounts. On success the contract status
    /// transitions from `Created` to `Funded`.
    ///
    /// # Arguments
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
            panic_with_error!(&env, EscrowError::InvalidState);
        }

        let next_funded = contract
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount));
        if next_funded > contract.total_amount {
            panic_with_error!(&env, EscrowError::FundingExceedsRequired);
        }

        contract.funded_amount = next_funded;
        if contract.funded_amount == contract.total_amount {
            contract.status = ContractStatus::Funded;
        }

        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        Self::ensure_not_paused(&env);
        let mut contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();
      }
        if amount != total_required {
            panic!("Deposit amount must equal total milestone amounts");

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
            panic_with_error!(&env, EscrowError::InvalidState);
        }
        if milestone_id >= contract.milestones.len() {
            panic_with_error!(&env, EscrowError::MilestoneNotFound);
        }

        let mut milestone = contract.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic_with_error!(&env, EscrowError::MilestoneAlreadyReleased);
        let available_balance = record
            .funded_amount
            .checked_sub(record.released_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
      }
  
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

        if contract
            .released_amount
            .checked_add(milestone.amount)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount))
            > contract.funded_amount
        {
            panic_with_error!(&env, EscrowError::InsufficientEscrowBalance);
        }

        let milestone_amount = milestone.amount;
        milestone.released = true;
        contract.milestones.set(milestone_id, milestone);

        contract.released_amount = contract
            .released_amount
            .checked_add(milestone_amount)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::InvalidAmount));
        contract.released_milestones += 1;

        if Self::all_milestones_released(&contract.milestones) {
            contract.status = ContractStatus::Completed;
            Self::add_pending_reputation_credit(&env, &contract.freelancer);
        }

        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn issue_reputation(env: Env, contract_id: u32, rating: i128) -> bool {
        Self::ensure_not_paused(&env);
        let contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();

        if contract.status != ContractStatus::Completed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }
        if contract.reputation_issued {
            panic_with_error!(&env, EscrowError::ReputationAlreadyIssued);
        }
        let mut updated_milestone = milestone;
        updated_milestone.approved_by = Some(caller);
        updated_milestone.approval_timestamp = Some(env.ledger().timestamp());
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
            .get::<_, Reputation>(&DataKey::V1(V1Key::Reputation(freelancer)))
            .unwrap_or(Reputation {
                total_rating: 0,
                ratings_count: 0,
            }))
    }

        let params = Self::protocol_parameters(&env);
        if rating < params.min_reputation_rating || rating > params.max_reputation_rating {
            panic_with_error!(&env, EscrowError::InvalidRating);
        }

        let freelancer = contract.freelancer.clone();
        let credits = Self::get_pending_reputation_credits(env.clone(), freelancer.clone());
        if credits == 0 {
            panic!("no reputation credits available");
        }

        let key = DataKey::Reputation(freelancer.clone());
        let mut record = env
            .storage()
            .persistent()
            .get::<_, ReputationRecord>(&key)
            .unwrap_or(ReputationRecord {
                completed_contracts: 0,
                total_rating: 0,
                last_rating: 0,
            });
fn ensure_storage_layout(env: &Env) -> Result<(), EscrowError> {
    let storage = env.storage().persistent();
    let version_key = DataKey::Meta(MetaKey::LayoutVersion);

        record.completed_contracts += 1;
        record.total_rating += rating;
        record.last_rating = rating;
        env.storage().persistent().set(&key, &record);

        env.storage().persistent().set(
            &DataKey::PendingReputationCredits(freelancer.clone()),
            &(credits - 1),
        );

        let mut updated = contract;
        updated.reputation_issued = true;
        Self::save_contract(&env, contract_id, &updated);

        true
    }

    /// Opens a dispute for a funded contract.
    pub fn open_dispute(env: Env, contract_id: u32, initiator: Address, reason: String) -> bool {
        Self::ensure_not_paused(&env);
        initiator.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Funded {
            panic_with_error!(&env, EscrowError::InvalidState);
        }
        if initiator != contract.client && initiator != contract.freelancer {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }
        if Self::load_dispute(&env, contract_id).is_some() {
            panic_with_error!(&env, EscrowError::DisputeAlreadyOpen);
        }

        contract.status = ContractStatus::Disputed;
        Self::save_contract(&env, contract_id, &contract);

        let dispute = DisputeState {
            record: DisputeRecord {
                initiator,
                reason,
                timestamp: env.ledger().timestamp(),
            },
            evidence: Vec::new(&env),
            resolution: Vec::new(&env),
            paid_out: false,
        };
        Self::save_dispute(&env, contract_id, &dispute);
        true
        // Should not panic
        Escrow::check_funding_invariants(funding);
    }

    /// Appends evidence to an open dispute.
    pub fn submit_dispute_evidence(
        env: Env,
        contract_id: u32,
        submitter: Address,
        uri: String,
    ) -> bool {
        Self::ensure_not_paused(&env);
        submitter.require_auth();

        let contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Disputed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }
        if submitter != contract.client && submitter != contract.freelancer {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        let mut dispute = Self::load_dispute(&env, contract_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NoDisputeActive));
        if dispute.resolution.len() > 0 {
            panic!("dispute already resolved");
        }

        dispute.evidence.push_back(DisputeEvidence {
            submitter,
            uri,
            timestamp: env.ledger().timestamp(),
        });
        // Check if all milestones are released
        let all_released = contract.milestones.iter().all(|m| m.released);
        if all_released {
            contract.transition_status(ContractStatus::Completed);
        }
    #[test]
    #[should_panic(expected = "total_released > total_funded")]
    fn test_funding_invariants_over_release() {
        let funding = FundingAccount {
            total_funded: 1000,
            total_released: 1500,
            total_available: -500,
        };
        Escrow::check_funding_invariants(funding);
    }

    #[test]
    #[should_panic(expected = "total_released > total_funded")]
    fn test_funding_invariants_negative_funded() {
        let funding = FundingAccount {
            total_funded: -100,
            total_released: 0,
            total_available: -100,
        };

        Escrow::check_funding_invariants(funding);
    }

    /// Mark a contract as disputed, guarded by allowed status transitions.
    ///
    /// # Errors
    /// Panics if:
    /// - Caller is not the client or arbiter
    /// - Contract is not in Funded status
    pub fn dispute_contract(env: Env, _contract_id: u32, caller: Address) -> bool {
        caller.require_auth();

        let mut contract: EscrowContract = env
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

        Self::save_dispute(&env, contract_id, &dispute);
        true
    }

    /// Resolves an open dispute.
    ///
    /// Security: resolution is restricted to the pause-control admin.
    pub fn resolve_dispute(
        env: Env,
        contract_id: u32,
        resolver: Address,
        outcome: DisputeOutcome,
    ) -> bool {
        Self::ensure_not_paused(&env);
        resolver.require_auth();

        let admin = Self::read_admin(&env);
        if resolver != admin {
            panic_with_error!(&env, EscrowError::Unauthorized);
        }

        let contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Disputed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }

        let mut dispute = Self::load_dispute(&env, contract_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NoDisputeActive));
        if dispute.resolution.len() > 0 {
            panic!("dispute already resolved");
        }

        dispute.resolution.push_back(DisputeResolution {
            resolver,
            outcome,
            timestamp: env.ledger().timestamp(),
        });
        Self::save_dispute(&env, contract_id, &dispute);
    /// Issue a reputation credential for a freelancer after contract completion.
    ///
    /// This is a stub for the on-chain reputation system. In a full
    /// implementation it would mint a verifiable credential or update a
    /// reputation ledger entry for `freelancer`.
    ///
    /// # Arguments
    ///
    /// | Name         | Type      | Description                                    |
    /// |--------------|-----------|------------------------------------------------|
    /// | `_env`       | `Env`     | Soroban host environment (unused).             |
    /// | `_freelancer`| `Address` | Freelancer receiving the credential (unused).  |
    /// | `_rating`    | `i128`    | Numeric rating value, e.g. 1–5 (unused).       |
    ///
    /// # Returns
    ///
    /// `true` (always, stub implementation).
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

    /// Applies the dispute resolution, marking the contract as completed.
    pub fn payout_dispute(env: Env, contract_id: u32) -> bool {
        Self::ensure_not_paused(&env);

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Disputed {
            panic_with_error!(&env, EscrowError::InvalidState);
        }

        let mut dispute = Self::load_dispute(&env, contract_id)
            .unwrap_or_else(|| panic_with_error!(&env, EscrowError::NoDisputeActive));

        if dispute.resolution.len() == 0 {
            panic_with_error!(&env, EscrowError::DisputeNotResolved);
        }
        let resolution = dispute.resolution.get(0).unwrap();
        if dispute.paid_out {
            panic_with_error!(&env, EscrowError::DisputeAlreadyPaidOut);
        }

        // This contract implementation does not move tokens; payout is recorded
        // as ledger state updates for testing / integration.
        match resolution.outcome {
            DisputeOutcome::Client => {
                // No additional release recorded.
            }
            DisputeOutcome::Freelancer => {
                contract.released_amount = contract.funded_amount;
            }
            DisputeOutcome::Split(freelancer_amount) => {
                if freelancer_amount < 0 || freelancer_amount > contract.funded_amount {
                    panic_with_error!(&env, EscrowError::InvalidAmount);
                }
                contract.released_amount = freelancer_amount;
            }
        }

        contract.status = ContractStatus::Completed;
        Self::save_contract(&env, contract_id, &contract);

        dispute.paid_out = true;
        Self::save_dispute(&env, contract_id, &dispute);
        true
    }
    // get_admin already defined above

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Returns the stored contract state.
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::load_contract(&env, contract_id)
    }

    pub fn get_dispute(env: Env, contract_id: u32) -> Option<DisputeState> {
        Self::load_dispute(&env, contract_id)
    }

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

        let params = Self::validated_protocol_parameters(
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );
        env.storage().persistent().set(&DataKey::GovernanceAdmin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &params);
        true
    }

    pub fn update_protocol_parameters(
        env: Env,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        let admin = Self::governance_admin(&env);
        admin.require_auth();

        let params = Self::validated_protocol_parameters(
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &params);
        true
    }

    pub fn propose_governance_admin(env: Env, pending_admin: Address) -> bool {
        let admin = Self::governance_admin(&env);
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &pending_admin);
        true
    }

    pub fn accept_governance_admin(env: Env) -> bool {
        let pending = Self::pending_governance_admin(&env)
            .unwrap_or_else(|| panic!("no pending governance admin"));
        pending.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &pending);
        env.storage().persistent().remove(&DataKey::PendingGovernanceAdmin);
        true
    }

    /// Returns the active protocol parameters.
    ///
    /// If governance has not been initialized yet, this returns the safe default
    /// parameters baked into the contract.
    pub fn get_protocol_parameters(env: Env) -> ProtocolParameters {
        Self::protocol_parameters(&env)
    #[test]
    fn test_contract_invariants_fully_released() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let milestones = vec![
            &env,
            Milestone {
                amount: 500,
                released: true,
            },
            Milestone {
                amount: 500,
                released: true,
            },
        ];

        let state = EscrowState {
            client,
            freelancer,
            status: ContractStatus::Completed,
            milestones,
            funding: FundingAccount {
                total_funded: 1000,
                total_released: 1000,
                total_available: 0,
            },
        };

        // Should not panic
        Escrow::check_contract_invariants(state);
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
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::ContractNotFound))
    #[test]
    #[should_panic(expected = "Milestone amounts must be positive")]
    fn test_create_contract_negative_milestone() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let milestones = vec![&env, 100_i128, -50_i128, 200_i128];

        Escrow::create_contract(env.clone(), client, freelancer, milestones);
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
