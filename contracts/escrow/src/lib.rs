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
pub enum DataKey {
    /// Full escrow contract state, keyed by the numeric contract ID.
    Contract(u32),
    /// Whether a reputation credential has already been issued for the given contract ID.
    /// Immutably set to `true` on first issuance; prevents replay and double-issuance.
    ReputationIssued(u32),
    /// Auto-incrementing counter; incremented on every [`Escrow::create_contract`] call.
    NextId,
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
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    InvalidContractId = 1,
    InvalidMilestoneId = 2,
    InvalidAmount = 3,
    InvalidRating = 4,
    EmptyMilestones = 5,
    InvalidParticipant = 6,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum DataKey {
    Admin,
    Paused,
    EmergencyPaused,
}

/// Stored escrow state for a single agreement.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
/// Escrow record layout for storage version `V1`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowRecord {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<Milestone>,
    pub milestone_count: u32,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub released_milestones: u32,
    pub status: ContractStatus,
    pub reputation_issued: bool,
}

/// Reputation state derived from completed escrow contracts.
/// Freelancer reputation aggregate layout for storage version `V1`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reputation {
    pub total_rating: i128,
    pub ratings_count: u32,
}

/// Public description of the active storage namespaces.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolParameters {
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
pub struct StorageLayoutPlan {
    pub version: u32,
    pub meta_namespace: Symbol,
    pub contracts_namespace: Symbol,
    pub reputation_namespace: Symbol,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
enum StorageVersion {
    V1 = 1,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum MetaKey {
    LayoutVersion,
    NextContractId,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum V1Key {
    Contract(u32),
    Reputation(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum DataKey {
    Meta(MetaKey),
    V1(V1Key),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    InvalidParticipants = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    ContractNotFound = 4,
    AmountMustBePositive = 5,
    ArithmeticOverflow = 6,
    InvalidState = 7,
    MilestoneNotFound = 8,
    MilestoneAlreadyReleased = 9,
    InsufficientEscrowBalance = 10,
    FundingExceedsRequired = 11,
    InvalidRating = 12,
    ReputationAlreadyIssued = 13,
    UnsupportedStorageVersion = 14,
    UnsupportedMigrationTarget = 15,
}

/// Error types for milestone validation and contract logic.
#[derive(Debug, PartialEq, Eq)]
pub enum EscrowError {
    /// Milestone amount is zero or negative.
    InvalidMilestoneAmount,
    /// Milestone index is out of bounds.
    InvalidMilestoneIndex,
    /// No milestones provided.
    NoMilestones,
    /// Milestone already released.
    MilestoneAlreadyReleased,
}

/// Persistent record for a single escrow engagement.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ContractRecord {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<Milestone>,
    pub status: ContractStatus,
}

#[contracttype]
pub enum DataKey {
    Contract(u32),
    NextId,
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Initializes admin-managed pause controls.
    /// Returns the currently active storage layout version.
    ///
    /// If version metadata is missing, this initializes the contract metadata
    /// to layout `V1` and returns `1`.
    pub fn get_storage_version(env: Env) -> Result<u32, EscrowError> {
        ensure_storage_layout(&env)?;
        Ok(StorageVersion::V1 as u32)
    }

    /// Returns the storage namespace plan used by the contract.
    ///
    /// This serves as an explicit migration-safe contract between code and
    /// stored keys. Future versions can add `V2(...)` key variants without
    /// mutating `V1` data formats.
    pub fn storage_layout_plan(env: Env) -> Result<StorageLayoutPlan, EscrowError> {
        ensure_storage_layout(&env)?;
        Ok(StorageLayoutPlan {
            version: StorageVersion::V1 as u32,
            meta_namespace: symbol_short!("meta_v1"),
            contracts_namespace: symbol_short!("escrow_v1"),
            reputation_namespace: symbol_short!("rep_v1"),
        })
    }

    /// Migration entrypoint for future layouts.
    ///
    /// For now only `V1` exists. Migrating to `1` is a no-op and returns
    /// `true`. Any other target is rejected.
    pub fn migrate_storage(env: Env, target_version: u32) -> Result<bool, EscrowError> {
        ensure_storage_layout(&env)?;
        if target_version != StorageVersion::V1 as u32 {
            return Err(EscrowError::UnsupportedMigrationTarget);
        }
        Ok(true)
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

    /// Create a new escrow contract with milestone release authorization
    ///
    /// # Arguments
    /// * `client` - Address of the client who funds the escrow
    /// * `freelancer` - Address of the freelancer who receives payments
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestone_amounts` - Vector of milestone payment amounts
    /// * `release_auth` - Authorization scheme for milestone releases
    ///
    /// # Returns
    /// Contract ID for the newly created escrow
    ///
    /// # Errors
    /// Panics if:
    /// - Contract is paused
    /// - Milestone amounts vector is empty
    /// - Any milestone amount is zero or negative
    /// - Client and freelancer addresses are the same

    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
    ) -> Result<u32, EscrowError> {
        ensure_storage_layout(&env)?;
        client.require_auth();

        if client == freelancer {
            return Err(EscrowError::InvalidParticipants);
        }

        let milestone_count = milestone_amounts.len();
        if milestone_count == 0 {
            return Err(EscrowError::EmptyMilestones);
        }

        let mut milestones = Vec::new(&env);
        let mut total_amount = 0_i128;
        let mut i = 0_u32;
        while i < milestone_count {
            let amount = milestone_amounts
                .get(i)
                .ok_or(EscrowError::InvalidMilestoneAmount)?;
            if amount <= 0 {
                return Err(EscrowError::InvalidMilestoneAmount);
            }
            total_amount = total_amount
                .checked_add(amount)
                .ok_or(EscrowError::ArithmeticOverflow)?;
            milestones.push_back(Milestone {
                amount,
                released: false,
            });
            i += 1;
        }

        let id = next_contract_id(&env)?;
        let record = EscrowRecord {
            client,
            freelancer,
            milestones,
            milestone_count,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            released_milestones: 0,
            status: ContractStatus::Created,
            reputation_issued: false,
        };

        save_contract(&env, id, &record);
        Ok(id)
    }

    /// Deposits funds into escrow for a contract.
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> Result<bool, EscrowError> {
        ensure_storage_layout(&env)?;

        if amount <= 0 {
            return Err(EscrowError::AmountMustBePositive);
        }

        let mut record = load_contract(&env, contract_id)?;
        record.client.require_auth();

        if record.status == ContractStatus::Completed {
            return Err(EscrowError::InvalidState);
        }

        let updated_funded = record
            .funded_amount
            .checked_add(amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;

        if updated_funded > record.total_amount {
            return Err(EscrowError::FundingExceedsRequired);
        }

        record.funded_amount = updated_funded;
        if record.funded_amount > 0 {
            record.status = ContractStatus::Funded;
        }

        save_contract(&env, contract_id, &record);
        Ok(true)
    }

    /// Releases a milestone payment for a funded contract.
    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        milestone_id: u32,
    ) -> Result<bool, EscrowError> {
        ensure_storage_layout(&env)?;

        let mut record = load_contract(&env, contract_id)?;
        record.client.require_auth();

        if record.status != ContractStatus::Funded {
            return Err(EscrowError::InvalidState);
        }

        let mut milestone = record
            .milestones
            .get(milestone_id)
            .ok_or(EscrowError::MilestoneNotFound)?;

        if milestone.released {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }

        let available_balance = record
            .funded_amount
            .checked_sub(record.released_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;

        if milestone.amount > available_balance {
            return Err(EscrowError::InsufficientEscrowBalance);
        }

        milestone.released = true;
        record.milestones.set(milestone_id, milestone.clone());

        record.released_amount = record
            .released_amount
            .checked_add(milestone.amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        record.released_milestones = record
            .released_milestones
            .checked_add(1)
            .ok_or(EscrowError::ArithmeticOverflow)?;

        if record.released_milestones == record.milestone_count {
            record.status = ContractStatus::Completed;
        }

        save_contract(&env, contract_id, &record);
        Ok(true)
    }

    /// Issues reputation for a freelancer after contract completion.
    pub fn issue_reputation(env: Env, contract_id: u32, rating: i128) -> Result<bool, EscrowError> {
        ensure_storage_layout(&env)?;

        if !(1..=5).contains(&rating) {
            return Err(EscrowError::InvalidRating);
        }

        let mut record = load_contract(&env, contract_id)?;
        record.client.require_auth();

        if record.status != ContractStatus::Completed {
            return Err(EscrowError::InvalidState);
        }

        if record.reputation_issued {
            return Err(EscrowError::ReputationAlreadyIssued);
        }

        let rep_key = DataKey::V1(V1Key::Reputation(record.freelancer.clone()));
        let mut reputation = env
            .storage()
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

    /// Returns contract state for a given contract id.
    pub fn get_contract(env: Env, contract_id: u32) -> Result<EscrowRecord, EscrowError> {
        ensure_storage_layout(&env)?;
        load_contract(&env, contract_id)
    }

    /// Returns aggregate reputation for a freelancer.
    pub fn get_reputation(env: Env, freelancer: Address) -> Result<Reputation, EscrowError> {
        ensure_storage_layout(&env)?;
        Ok(env
            .storage()
            .persistent()
            .get::<_, Reputation>(&DataKey::V1(V1Key::Reputation(freelancer)))
            .unwrap_or(Reputation {
                total_rating: 0,
                ratings_count: 0,
            }))
    }

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Getter for milestones (useful for verification and UI)
    pub fn get_milestones(env: Env) -> Vec<Milestone> {
        env.storage()
            .instance()
            .get(&DataKey::Milestones)
            .unwrap_or(Vec::new(&env))
    }
}

fn ensure_storage_layout(env: &Env) -> Result<(), EscrowError> {
    let storage = env.storage().persistent();
    let version_key = DataKey::Meta(MetaKey::LayoutVersion);

    match storage.get::<_, u32>(&version_key) {
        Some(version) if version == StorageVersion::V1 as u32 => {}
        Some(_) => return Err(EscrowError::UnsupportedStorageVersion),
        None => storage.set(&version_key, &(StorageVersion::V1 as u32)),
    };

    let next_id_key = DataKey::Meta(MetaKey::NextContractId);
    if storage.get::<_, u32>(&next_id_key).is_none() {
        storage.set(&next_id_key, &1_u32);
    }
    Ok(())
}

fn next_contract_id(env: &Env) -> Result<u32, EscrowError> {
    let key = DataKey::Meta(MetaKey::NextContractId);
    let storage = env.storage().persistent();

    let id = storage.get::<_, u32>(&key).unwrap_or(1_u32);
    let next = id.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;

    storage.set(&key, &next);
    Ok(id)
}

fn load_contract(env: &Env, contract_id: u32) -> Result<EscrowRecord, EscrowError> {
    env.storage()
        .persistent()
        .get::<_, EscrowRecord>(&DataKey::V1(V1Key::Contract(contract_id)))
        .ok_or(EscrowError::ContractNotFound)
}

fn save_contract(env: &Env, contract_id: u32, record: &EscrowRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::V1(V1Key::Contract(contract_id)), record);
}

// Helper functions

fn get_next_contract_id(env: &Env) -> u32 {
    let mut next_id = env
        .storage()
        .persistent()
        .get(&NEXT_CONTRACT_ID)
        .unwrap_or(1u32);
    let current_id = next_id;
    next_id += 1;
    env.storage().persistent().set(&NEXT_CONTRACT_ID, &next_id);
    current_id
}

fn get_next_dispute_id(env: &Env) -> u32 {
    let mut next_id = env
        .storage()
        .persistent()
        .get(&NEXT_DISPUTE_ID)
        .unwrap_or(1u32);
    let current_id = next_id;
    next_id += 1;
    env.storage().persistent().set(&NEXT_DISPUTE_ID, &next_id);
    current_id
}

fn get_contracts_map(env: &Env) -> Map<u32, EscrowContract> {
    env.storage()
        .persistent()
        .get(&CONTRACTS)
        .unwrap_or(Map::new(env))
}

fn get_disputes_map(env: &Env) -> Map<u32, Dispute> {
    env.storage()
        .persistent()
        .get(&DISPUTES)
        .unwrap_or(Map::new(env))
}

fn require_contract_status(contract: &EscrowContract, expected_status: ContractStatus) {
    if contract.status != expected_status {
        panic!("invalid contract status");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

    // ============================================================================
    // FUNDING ACCOUNT INVARIANT TESTS
    // ============================================================================

    #[test]
    fn test_funding_invariants_valid_state() {
        let funding = FundingAccount {
            total_funded: 1000,
            total_released: 400,
            total_available: 600,
        };

        // Should not panic
        Escrow::check_funding_invariants(funding);
    }

    #[test]
    #[should_panic(expected = "total_available != total_funded - total_released")]
    fn test_funding_invariants_invalid_available() {
        let funding = FundingAccount {
            total_funded: 1000,
            total_released: 400,
            total_available: 500, // Should be 600
        };

        Escrow::check_funding_invariants(funding);
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

    #[test]
    #[should_panic(expected = "total_released < 0")]
    fn test_funding_invariants_negative_released() {
        let funding = FundingAccount {
            total_funded: 1000,
            total_released: -100,
            total_available: 1100,
        };

        Escrow::check_funding_invariants(funding);
    }
  
    #[test]
    #[should_panic(expected = "total_available != total_funded - total_released")]
    fn test_funding_invariants_negative_available() {
        let funding = FundingAccount {
            total_funded: 1000,
            total_released: 400,
            total_available: -100,
        };

        Escrow::check_funding_invariants(funding);
    }

    #[test]
    fn test_funding_invariants_zero_state() {
        let funding = FundingAccount {
            total_funded: 0,
            total_released: 0,
            total_available: 0,
        };

        // Should not panic
        Escrow::check_funding_invariants(funding);
    }

    #[test]
    fn test_funding_invariants_fully_released() {
        let funding = FundingAccount {
            total_funded: 1000,
            total_released: 1000,
            total_available: 0,
        };

        // Should not panic
        Escrow::check_funding_invariants(funding);
    }

    // ============================================================================
    // MILESTONE ACCOUNTING INVARIANT TESTS
    // ============================================================================

    #[test]
    fn test_milestone_invariants_no_releases() {
        let env = Env::default();
        let milestones = vec![
            &env,
            Milestone {
                amount: 100,
                released: false,
            },
            Milestone {
                amount: 200,
                released: false,
            },
        ];

        // Should not panic
        Escrow::check_milestone_invariants(milestones, 0);
    }

    #[test]
    fn test_milestone_invariants_partial_releases() {
        let env = Env::default();
        let milestones = vec![
            &env,
            Milestone {
                amount: 100,
                released: true,
            },
            Milestone {
                amount: 200,
                released: false,
            },
            Milestone {
                amount: 300,
                released: true,
            },
        ];

        // 100 + 300 = 400
        Escrow::check_milestone_invariants(milestones, 400);
    }

    #[test]
    #[should_panic(expected = "sum of released milestones != total_released")]
    fn test_milestone_invariants_mismatch_released_sum() {
        let env = Env::default();
        let milestones = vec![
            &env,
            Milestone {
                amount: 100,
                released: true,
            },
            Milestone {
                amount: 200,
                released: true,
            },
        ];

        // Sum is 300, but we claim 250
        Escrow::check_milestone_invariants(milestones, 250);
    }

    #[test]
    #[should_panic(expected = "milestone amount must be positive")]
    fn test_milestone_invariants_zero_amount() {
        let env = Env::default();
        let milestones = vec![
            &env,
            Milestone {
                amount: 0,
                released: false,
            },
        ];

        Escrow::check_milestone_invariants(milestones, 0);
    }

    #[test]
    #[should_panic(expected = "milestone amount must be positive")]
    fn test_milestone_invariants_negative_amount() {
        let env = Env::default();
        let milestones = vec![
            &env,
            Milestone {
                amount: -100,
                released: false,
            },
        ];

        Escrow::check_milestone_invariants(milestones, 0);
    }

    #[test]
    fn test_milestone_invariants_all_released() {
        let env = Env::default();
        let milestones = vec![
            &env,
            Milestone {
                amount: 100,
                released: true,
            },
            Milestone {
                amount: 200,
                released: true,
            },
            Milestone {
                amount: 300,
                released: true,
            },
        ];

        // All released: 100 + 200 + 300 = 600
        Escrow::check_milestone_invariants(milestones, 600);
    }

    // ============================================================================
    // CONTRACT STATE INVARIANT TESTS
    // ============================================================================

    #[test]
    fn test_contract_invariants_valid_state() {
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
            status: ContractStatus::Created,
            milestones,
            funding: FundingAccount {
                total_funded: 0,
                total_released: 0,
                total_available: 0,
            },
        };

        // Should not panic
        Escrow::check_contract_invariants(state);
    }

    #[test]
    fn test_contract_invariants_with_deposits() {
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
                total_funded: 1000,
                total_released: 0,
                total_available: 1000,
            },
        };

        // Should not panic
        Escrow::check_contract_invariants(state);
    }

    #[test]
    fn test_contract_invariants_with_partial_releases() {
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
                released: false,
            },
        ];

        let state = EscrowState {
            client,
            freelancer,
            status: ContractStatus::Funded,
            milestones,
            funding: FundingAccount {
                total_funded: 1000,
                total_released: 500,
                total_available: 500,
            },
        };

        // Should not panic
        Escrow::check_contract_invariants(state);
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
