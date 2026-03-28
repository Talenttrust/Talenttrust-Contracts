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

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String,
    Map, Symbol, Vec,
};

pub(crate) mod cross_contract;

/// Persistent storage keys used by the Escrow contract.
///
/// Each variant corresponds to a distinct piece of contract state:
/// - [`DataKey::Contract`] stores the full [`EscrowContract`] keyed by its numeric ID.
/// - [`DataKey::ReputationIssued`] is a boolean flag that prevents double-issuance of
///   reputation credentials for a given contract.
/// - [`DataKey::NextId`] is a monotonically increasing counter for assigning contract IDs.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Full escrow contract state, keyed by the numeric contract ID.
    Contract(u32),
    Milestone(u32, u32),
    ContractStatus(u32),
    NextContractId,
    ContractTimeout(u32),
    MilestoneDeadline(u32, u32),
    DisputeDeadline(u32),
    LastActivity(u32),
    Dispute(u32),
    MilestoneComplete(u32, u32),
    Paused,
    EmergencyPaused,
    Reputation(Address),
    PendingReputationCredits(Address),
    GovernanceAdmin,
    PendingGovernanceAdmin,
    ProtocolParameters,
    /// Registered SEP-41 token contract used for escrow transfers.
    TokenContract,
    /// Reentrancy lock set during guarded external calls.
    CallLock,
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
    /// Current lifecycle status of the contract.
    pub status: ContractStatus,
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

/// Timeout configuration for escrow contracts
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeoutConfig {
    /// Timeout duration in seconds
    pub duration: u64,
    /// Auto-resolve type: 0 = return to client, 1 = release to freelancer, 2 = split
    pub auto_resolve_type: u32,
}

/// Dispute structure for tracking disputes
#[contracttype]
#[derive(Clone, Debug)]
pub struct Dispute {
    /// Address that initiated the dispute
    pub initiator: Address,
    /// Reason for the dispute
    pub reason: Symbol,
    /// Timestamp when dispute was created
    pub created_at: u64,
    /// Whether dispute has been resolved
    pub resolved: bool,
}

/// Treasury configuration for protocol fee collection
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryConfig {
    /// Address where protocol fees are sent
    pub address: Address,
    /// Fee percentage in basis points (10000 = 100%)
    pub fee_basis_points: u32,
}

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
    /// Token contract address is not configured
    TokenContractNotSet = 24,
    /// Cross-contract call failed or returned an error
    ExternalCallFailed = 25,
    /// Reentrant external call attempt detected
    ReentrancyDetected = 26,
    /// External address points to this contract
    SelfReferentialAddress = 27,
    /// Transfer verification failed due to balance delta mismatch
    TransferVerificationFailed = 28,
    /// Transfer amount must be strictly positive
    AmountMustBePositive = 29,
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

/// Default approval/release deadline for each milestone after contract creation.
const DEFAULT_MILESTONE_TIMEOUT_SECS: u64 = 7 * 24 * 60 * 60;

#[contractimpl]
impl Escrow {
    /// @notice Register or rotate the SEP-41 token contract used for escrow transfers.
    /// @dev Security: the caller must authorize this operation.
    /// @param caller Address authorizing token contract configuration.
    /// @param token_contract External token contract address to persist.
    /// @return bool True when the token contract address is stored.
    pub fn set_token_contract(env: Env, caller: Address, token_contract: Address) -> bool {
        caller.require_auth();

        if token_contract == env.current_contract_address() {
            panic!("Token contract cannot be escrow contract");
        }

        env.storage()
            .instance()
            .set(&DataKey::TokenContract, &token_contract);

        true
    }

    /// @notice Return the configured SEP-41 token contract address, if present.
    /// @return Option<Address> Configured token address or `None`.
    pub fn get_token_contract(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::TokenContract)
    }

    /// @notice Perform a guarded token transfer using the configured token contract.
    ///
    /// @dev This helper centralizes external call assumptions and failure mapping:
    /// it requires a configured token contract, validates it is non-self,
    /// applies a reentrancy lock, and surfaces external failures as
    /// `EscrowError` values.
    /// @param from Token source address.
    /// @param to Token recipient address.
    /// @param amount Transfer amount in token base units.
    /// @return Result<bool, EscrowError> `Ok(true)` on successful guarded transfer.
    pub fn guarded_external_transfer(
        env: Env,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<bool, EscrowError> {
        let token_contract = cross_contract::get_required_token_contract(&env)?;
        cross_contract::safe_token_transfer(&env, &token_contract, &from, &to, amount)?;
        Ok(true)
    }

    /// Create a new escrow contract with milestone-based release authorization.
    ///
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
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
    ) -> u32 {
        if milestone_amounts.is_empty() {
            panic!("At least one milestone required");
        }

        let protocol_params = Self::protocol_parameters(&env);
        if milestone_amounts.len() > protocol_params.max_milestones {
            panic!("Exceeds maximum milestone count");
        }

        let mut total_amount: i128 = 0;
        let mut milestones = Vec::new(&env);

        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts.get(i).unwrap();
            if amount <= 0 {
                panic!("Milestone amounts must be positive");
            }
            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic!("Amount overflow"));

            milestones.push_back(Milestone {
                amount,
                released: false,
            });
            i += 1;
        }

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
            .set(&symbol_short!("contract"), &contract_data);

        contract_id
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

        if contract.status != ContractStatus::Created {
            panic!("Contract must be in Created status to deposit funds");
        }

        let mut total_required = 0i128;
        for i in 0..contract.milestones.len() {
            total_required += contract.milestones.get(i).unwrap().amount;
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
            panic!("Contract must be in Funded status to approve milestones");
        }

        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let available_balance = record
            .funded_amount
            .checked_sub(record.released_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;

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

        if milestone
            .approved_by
            .clone()
            .map_or(false, |addr| addr == caller)
        {
            panic!("Milestone already approved by this address");
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

        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to release milestones");
        }

        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

fn ensure_storage_layout(env: &Env) -> Result<(), EscrowError> {
    let storage = env.storage().persistent();
    let version_key = DataKey::Meta(MetaKey::LayoutVersion);

        if milestone.released {
            panic!("Milestone already released");
        }

        let has_sufficient_approval = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => milestone
                .approved_by
                .clone()
                .map_or(false, |addr| addr == contract.client),
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |arbiter| {
                    milestone
                        .approved_by
                        .clone()
                        .map_or(false, |addr| addr == arbiter)
                })
            }
            ReleaseAuthorization::ClientAndArbiter => {
                milestone.approved_by.clone().map_or(false, |addr| {
                    addr == contract.client
                        || contract
                            .arbiter
                            .clone()
                            .map_or(false, |arbiter| addr == arbiter)
                })
            }
            ReleaseAuthorization::MultiSig => milestone
                .approved_by
                .clone()
                .map_or(false, |addr| addr == contract.client),
        };

        // Should not panic
        Escrow::check_funding_invariants(funding);
    }

        let mut updated_milestone = milestone;
        updated_milestone.released = true;

        contract.milestones.set(milestone_id, updated_milestone);

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

        true
    }

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
