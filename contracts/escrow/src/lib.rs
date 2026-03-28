//! # TalentTrust Escrow Contract
//!
//! A Soroban smart contract implementing a milestone-based escrow protocol for
//! the TalentTrust decentralized freelancer platform on the Stellar network.
//!
//! ## Overview
//!
//! The escrow contract holds funds on behalf of a client and releases them to a
//! freelancer as individual milestones are approved. An optional arbiter can be
//! designated for dispute resolution. Four authorization schemes are supported:
//! `ClientOnly`, `ArbiterOnly`, `ClientAndArbiter`, and `MultiSig`.
//!
//! ## Lifecycle
//!
//! ```text
//! create_contract → deposit_funds → approve_milestone_release → release_milestone
//!                                                              ↑ (repeat per milestone)
//! ```
//!
//! When every milestone has been released the contract status transitions to
//! `Completed` automatically.
//!
//! ## Security Assumptions
//!
//! - All callers that mutate state must pass `require_auth()`.
//! - The contract stores a single escrow record keyed by `"contract"`. A
//!   production deployment should key by `contract_id`.
//! - No native token transfer is performed in this implementation; fund custody
//!   and transfer must be wired up via the Stellar asset contract.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, String,
    Symbol, Vec,
};
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String,
    Symbol, Vec,
};

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

/// The lifecycle status of an escrow contract.
///
/// Valid transitions:
/// ```text
/// Created -> Funded -> Completed
/// Funded  -> Disputed
/// ```
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    /// Contract created, awaiting client deposit.
    Created = 0,
    /// Funds deposited by client; work is in progress.
    Funded = 1,
    /// All milestones released and contract finalised by the client.
    Completed = 2,
    /// A dispute has been raised; milestone payments are paused.
    Disputed = 3,
}

/// Represents a payment milestone in the escrow contract.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    /// Payment amount in stroops (1 XLM = 10_000_000 stroops).
    pub amount: i128,
    /// Whether the client has released this milestone's funds to the freelancer.
    pub released: bool,
    /// The address that approved this milestone (Client/Arbiter)
    pub approved_by: Option<Address>,
    /// The ledger timestamp of the approval
    pub approval_timestamp: Option<u64>,
}

/// Stored escrow state for a single agreement.
/// Defines the security authorization scheme required to approve and release milestones.
/// Carefully review the threat model associated with each scheme.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly,
    ArbiterOnly,
    ClientAndArbiter,
    MultiSig,
}

/// The on-chain record for a single escrow agreement.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowContract {
    /// Address of the client who funds the escrow.
    pub client: Address,
    /// Address of the freelancer who receives milestone payments.
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub release_auth: ReleaseAuthorization,
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

/// Custom errors for the escrow contract
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowError {
    /// Treasury not initialized
    TreasuryNotInitialized = 1,
    /// Invalid fee percentage (exceeds 100%)
    InvalidFeePercentage = 2,
    /// Unauthorized access
    Unauthorized = 3,
    /// Contract not found
    ContractNotFound = 4,
    /// Milestone not found
    MilestoneNotFound = 5,
    /// Milestone already released
    MilestoneAlreadyReleased = 6,
    /// Insufficient funds
    InsufficientFunds = 7,
    /// Invalid amount
    InvalidAmount = 8,
    /// Treasury already initialized
    TreasuryAlreadyInitialized = 9,
    /// Arithmetic overflow
    ArithmeticOverflow = 10,
    /// Timeout not exceeded
    TimeoutNotExceeded = 11,
    /// Invalid timeout duration
    InvalidTimeout = 12,
    /// Milestone not marked complete
    MilestoneNotComplete = 13,
    /// Milestone already complete
    MilestoneAlreadyComplete = 14,
    /// Dispute not found
    DisputeNotFound = 15,
    /// Dispute already resolved
    DisputeAlreadyResolved = 16,
    /// Timeout already claimed
    TimeoutAlreadyClaimed = 17,
    /// No dispute active
    NoDisputeActive = 18,
    /// Contract ID is invalid
    InvalidContractId = 19,
    /// Participant is invalid
    InvalidParticipant = 20,
    /// Empty milestones provided
    EmptyMilestones = 21,
    /// Invalid rating
    InvalidRating = 22,
    /// Invalid milestone ID
    InvalidMilestoneId = 23,
}

// (ReleaseAuthorization enum moved)

/// Full on-chain state of an escrow contract.
#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneApproval {
    /// Index of the milestone this record belongs to.
    pub milestone_id: u32,
    /// Map from approver address to approval boolean.
    pub approvals: Map<Address, bool>,
    /// Number of approvals required before release is permitted.
    pub required_approvals: u32,
    /// Aggregated approval status derived from `approvals`.
    pub approval_status: Approval,
}

/// Aggregated approval state for a milestone under a multi-party scheme.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Approval {
    /// No approvals recorded yet.
    None = 0,
    /// Only the client has approved.
    Client = 1,
    /// Only the arbiter has approved.
    Arbiter = 2,
    /// Both client and arbiter have approved.
    Both = 3,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

/// The TalentTrust escrow contract entry point.
#[contract]
pub struct Escrow;

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
    ///
    /// A `u32` contract identifier (current ledger sequence number).
    ///
    /// # Panics
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
                amount,
                released: false,
            });
            i += 1;
        }

        let contract_id = Self::next_contract_id(&env);
        // Limit contract size conceptually: prevent massive state requirements by bounding total scale
        if total_amount > 1_000_000_000_000_i128 {
            panic!("Exceeds maximum contract funding size");
        }

        let contract_data = EscrowContractData {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            release_auth,
            milestones,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            status: ContractStatus::Created,
        };

        let contract_id = env.ledger().sequence();
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(contract_id + 1));

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
    ///
    /// | Name          | Type      | Description                                         |
    /// |---------------|-----------|-----------------------------------------------------|
    /// | `env`         | `Env`     | Soroban host environment.                           |
    /// | `_contract_id`| `u32`     | Identifier of the escrow contract (reserved).       |
    /// | `caller`      | `Address` | Must be the client address; auth is required.       |
    /// | `amount`      | `i128`    | Amount in stroops; must equal total milestone sum.  |
    ///
    /// # Returns
    ///
    /// `true` on success.
    ///
    /// # Panics
    ///
    /// | Condition                                      | Message                                                    |
    /// |------------------------------------------------|------------------------------------------------------------|
    /// | Contract record not found in storage           | `"Contract not found"`                                     |
    /// | `caller` is not the client                     | `"Only client can deposit funds"`                          |
    /// | Contract status is not `Created`               | `"Contract must be in Created status to deposit funds"`    |
    /// | `amount` ≠ sum of all milestone amounts        | `"Deposit amount must equal total milestone amounts"`      |
    pub fn deposit_funds(env: Env, _contract_id: u32, caller: Address, amount: i128) -> bool {
        caller.require_auth();

        let contract: EscrowContractData = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        if caller != contract.client {
            panic!("Only client can deposit funds");
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
        }

        // Update contract status to Funded
        let mut updated_contract = contract;
        updated_contract.transition_status(ContractStatus::Funded);
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &updated_contract);
        record.funded_amount = updated_funded;
        if record.funded_amount > 0 {
            record.status = ContractStatus::Funded;
        }

        save_contract(&env, contract_id, &record);
        Ok(true)
    }

    fn ensure_valid_milestone_id(milestone_id: u32) -> Result<(), EscrowError> {
        // `u32::MAX` is reserved as an invalid sentinel in this placeholder implementation.
        if milestone_id == u32::MAX {
            return Err(EscrowError::InvalidMilestoneId);
        }
        Ok(())
    }
    /// Approve a milestone for release with proper authorization.
    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        milestone_id: u32,
    ) -> Result<bool, EscrowError> {
        ensure_storage_layout(&env)?;

        let mut contract: EscrowContractData = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

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
            panic!("Milestone already released");
        }

        let is_authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => caller == contract.client,
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::ClientAndArbiter => {
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::MultiSig => {
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
        };

        if record.released_milestones == record.milestone_count {
            record.status = ContractStatus::Completed;
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

        contract.milestones.set(milestone_id, updated_milestone);
        env.storage()
            .persistent()
            .get::<_, Reputation>(&rep_key)
            .unwrap_or(Reputation {
                total_rating: 0,
                ratings_count: 0,
            });

        reputation.total_rating = reputation
            .total_rating
            .checked_add(rating)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        reputation.ratings_count = reputation
            .ratings_count
            .checked_add(1)
            .ok_or(EscrowError::ArithmeticOverflow)?;

        env.storage().persistent().set(&rep_key, &reputation);

        record.reputation_issued = true;
        save_contract(&env, contract_id, &record);
        Ok(true)
    }

    /// Release a milestone payment to the freelancer after sufficient approvals.
    ///
    /// Verifies that the required approvals are in place according to the
    /// contract's [`ReleaseAuthorization`] scheme, marks the milestone as
    /// released, and transitions the contract to `Completed` if all milestones
    /// have been released.
    ///
    /// > **Note:** Actual token transfer to the freelancer is not implemented
    /// > in this version and must be wired up via the Stellar asset contract.
    ///
    /// # Arguments
    ///
    /// | Name           | Type      | Description                                              |
    /// |----------------|-----------|----------------------------------------------------------|
    /// | `env`          | `Env`     | Soroban host environment.                                |
    /// | `_contract_id` | `u32`     | Identifier of the escrow contract (reserved).            |
    /// | `caller`       | `Address` | Caller triggering the release; auth is required.         |
    /// | `milestone_id` | `u32`     | Zero-based index of the milestone to release.            |
    ///
    /// # Returns
    ///
    /// `true` on success.
    ///
    /// # Panics
    ///
    /// | Condition                                          | Message                                                          |
    /// |----------------------------------------------------|------------------------------------------------------------------|
    /// | Contract record not found in storage               | `"Contract not found"`                                           |
    /// | Contract status is not `Funded`                    | `"Contract must be in Funded status to release milestones"`      |
    /// | `milestone_id` ≥ number of milestones              | `"Invalid milestone ID"`                                         |
    /// | Milestone has already been released                | `"Milestone already released"`                                   |
    /// | Required approvals are not present                 | `"Insufficient approvals for milestone release"`                 |
    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();

        let mut contract: EscrowContractData = env
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
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to dispute");
        }

        let allowed_caller = caller == contract.client
            || contract.arbiter.clone().map_or(false, |arb| arb == caller);

        if !allowed_caller {
            panic!("Only client or arbiter can dispute contract");
        }

        contract.transition_status(ContractStatus::Disputed);
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

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
}

    #[test]
    #[should_panic(expected = "total_contract_value < total_funded")]
    fn test_contract_invariants_over_funded() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let milestones = vec![
            &env,
            Milestone {
                amount: 500,
                released: false,
            },
            Milestone {
                amount: 500,
                released: false,
            },
        ];

        let state = EscrowState {
            client,
            freelancer,
            status: ContractStatus::Funded,
            milestones,
            funding: FundingAccount {
                total_funded: 2000, // More than total contract value (1000)
                total_released: 0,
                total_available: 2000,
            },
        };

        Escrow::check_contract_invariants(state);
    }
    Ok(())
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
    }
}

    // ============================================================================
    // CONTRACT CREATION TESTS
    // ============================================================================

    #[test]
    fn test_create_contract_valid() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let milestones = vec![&env, 200_0000000_i128, 400_0000000_i128, 600_0000000_i128];

        let id = Escrow::create_contract(env.clone(), client, freelancer, milestones);
        assert_eq!(id, 1);
    }

    #[test]
    #[should_panic(expected = "Must have at least one milestone")]
    fn test_create_contract_no_milestones() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let milestones = vec![&env];

        Escrow::create_contract(env.clone(), client, freelancer, milestones);
    }
}

    #[test]
    #[should_panic(expected = "Milestone amounts must be positive")]
    fn test_create_contract_zero_milestone() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let milestones = vec![&env, 100_i128, 0_i128, 200_i128];

        Escrow::create_contract(env.clone(), client, freelancer, milestones);
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

    // ============================================================================
    // DEPOSIT FUNDS TESTS
    // ============================================================================

    #[test]
    fn test_deposit_funds_valid() {
        let env = Env::default();
        let result = Escrow::deposit_funds(env.clone(), 1, 1_000_0000000);
        assert!(result);
    }

    #[test]
    #[should_panic(expected = "Deposit amount must be positive")]
    fn test_deposit_funds_zero_amount() {
        let env = Env::default();
        Escrow::deposit_funds(env.clone(), 1, 0);
    }

    #[test]
    #[should_panic(expected = "Deposit amount must be positive")]
    fn test_deposit_funds_negative_amount() {
        let env = Env::default();
        Escrow::deposit_funds(env.clone(), 1, -1_000_0000000);
    }

    // ============================================================================
    // EDGE CASE AND OVERFLOW TESTS
    // ============================================================================

    #[test]
    fn test_large_milestone_amounts() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let milestones = vec![&env, i128::MAX / 3, i128::MAX / 3, i128::MAX / 3];

        let id = Escrow::create_contract(env.clone(), client, freelancer, milestones);
        assert_eq!(id, 1);
    }

    #[test]
    fn test_single_milestone_contract() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let milestones = vec![&env, 1000_i128];

        let id = Escrow::create_contract(env.clone(), client, freelancer, milestones);
        assert_eq!(id, 1);
    }

    #[test]
    fn test_many_milestones_contract() {
        let env = Env::default();
        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);
        let mut milestones = vec![&env];

        for i in 1..=100 {
            milestones.push_back(i as i128 * 100);
        }

        let id = Escrow::create_contract(env.clone(), client, freelancer, milestones);
        assert_eq!(id, 1);
    }

    #[test]
    fn test_funding_invariants_boundary_values() {
        // Test with maximum safe values that satisfy the invariant
        let total_funded = 1_000_000_000_000_000_000_i128;
        let total_released = 500_000_000_000_000_000_i128;
        let total_available = total_funded - total_released;

        let funding = FundingAccount {
            total_funded,
            total_released,
            total_available,
        };

        Escrow::check_funding_invariants(funding);
    }

    // ============================================================================
    // ORIGINAL TESTS (PRESERVED)
    // ============================================================================

    #[test]
    fn test_hello() {
        let env = Env::default();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let result = client.hello(&symbol_short!("World"));
        assert_eq!(result, symbol_short!("World"));
    }

    #[test]
    fn test_release_milestone() {
        let env = Env::default();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let result = client.release_milestone(&1, &0);
        assert!(result);
    }
}
