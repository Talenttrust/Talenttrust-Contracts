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

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

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

/// Represents a payment milestone in the escrow contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    /// Payment amount in stroops (1 XLM = 10_000_000 stroops).
    pub amount: i128,
    /// Whether the client has released this milestone's funds to the freelancer.
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
    pub refunded_amount: i128,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Contract(u32),
    Milestones(u32),
    ContractCount,
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
            status: ContractStatus::Created,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
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

        contract_id
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
