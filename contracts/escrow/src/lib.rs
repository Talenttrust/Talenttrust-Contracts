#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

/// Maximum fee basis points (100% = 10000 basis points)
pub const MAX_FEE_BASIS_POINTS: u32 = 10000;
/// Default protocol fee: 2.5% = 250 basis points
pub const DEFAULT_FEE_BASIS_POINTS: u32 = 250;
/// Default timeout duration: 30 days in seconds
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 2_592_000;

const DEFAULT_MIN_MILESTONE_AMOUNT: i128 = 1;
const DEFAULT_MAX_MILESTONES: u32 = 16;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
    InDispute = 4,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly = 0,
    ArbiterOnly = 1,
    ClientAndArbiter = 2,
    MultiSig = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
    pub approved_by: Option<Address>,
    pub approval_timestamp: Option<u64>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContractData {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<Milestone>,
    pub status: ContractStatus,
    pub release_auth: ReleaseAuthorization,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Paused,
    EmergencyPaused,
    TreasuryConfig,
    Contract(u32),
    NextContractId,
    Reputation(Address),
    PendingReputationCredits(Address),
    ProtocolParameters,
    GovernanceAdmin,
    PendingGovernanceAdmin,
    // Indexing keys
    ParticipantContracts(Address),
    StatusContracts(u32), // status as u32
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    TreasuryNotInitialized = 1,
    InvalidFeePercentage = 2,
    Unauthorized = 3,
    ContractNotFound = 4,
    MilestoneNotFound = 5,
    MilestoneAlreadyReleased = 6,
    InsufficientFunds = 7,
    InvalidAmount = 8,
    TreasuryAlreadyInitialized = 9,
    ArithmeticOverflow = 10,
    TimeoutNotExceeded = 11,
    InvalidTimeout = 12,
    MilestoneNotComplete = 13,
    MilestoneAlreadyComplete = 14,
    DisputeNotFound = 15,
    DisputeAlreadyResolved = 16,
    TimeoutAlreadyClaimed = 17,
    NoDisputeActive = 18,
    // Add custom ones if needed, otherwise use existing
    EmptyMilestones = 19,
    AddressMismatch = 20,
    InsufficientApprovals = 21,
    MilestoneAlreadyApproved = 22,
    ContractNotFunded = 23,
    ContractAlreadyFunded = 24,
}

#[cfg(test)]
mod test;

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Initializes admin-managed pause controls.
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
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &0u32);
        true
    }

    pub fn get_admin(env: Env) -> Address {
        Self::read_admin(&env)
    }

    pub fn pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    pub fn unpause(env: Env) -> bool {
        Self::require_admin(&env);
        if Self::is_emergency_internal(&env) {
            panic!("Emergency pause active");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        true
    }

    pub fn activate_emergency_pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &true);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    pub fn resolve_emergency(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        env.storage().instance().set(&DataKey::Paused, &false);
        true
    }

    pub fn is_paused(env: Env) -> bool {
        Self::is_paused_internal(&env)
    }

    pub fn is_emergency(env: Env) -> bool {
        Self::is_emergency_internal(&env)
    }

    /// Create a new escrow contract
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
    ) -> u32 {
        Self::ensure_not_paused(&env);
        client.require_auth();

        if client == freelancer {
            panic!("Client and freelancer cannot be the same address");
        }
        if milestone_amounts.is_empty() {
            panic!("At least one milestone required");
        }

        let mut milestones = Vec::new(&env);
        for amount in milestone_amounts.iter() {
            if amount <= 0 {
                env.panic_with_error(EscrowError::InvalidAmount);
            }
            milestones.push_back(Milestone {
                amount,
                released: false,
                approved_by: None,
                approval_timestamp: None,
            });
        }

        let contract_id = Self::next_contract_id(&env);
        let contract_data = EscrowContractData {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            milestones,
            status: ContractStatus::Created,
            release_auth,
            created_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract_data);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(contract_id + 1));

        // Indexing
        Self::add_to_participant_index(&env, &client, contract_id);
        Self::add_to_participant_index(&env, &freelancer, contract_id);
        Self::update_status_index(&env, contract_id, None, ContractStatus::Created);

        contract_id
    }

    pub fn deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        if caller != contract.client {
            panic!("Only client can deposit funds");
        }
        if contract.status != ContractStatus::Created {
            panic!("Contract already funded");
        }

        let mut total_required = 0i128;
        for m in contract.milestones.iter() {
            total_required += m.amount;
        }
        if amount != total_required {
            panic!("Deposit amount must equal total milestone amounts");
        }
        let old_status = contract.status;
        contract.status = ContractStatus::Funded;
        Self::save_contract(&env, contract_id, &contract);

        Self::update_status_index(&env, contract_id, Some(old_status), ContractStatus::Funded);
        true
    }

    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Funded {
            panic!("Contract not in Funded status");
        }
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let mut milestone = contract.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic!("Milestone already released");
        }

        let is_authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => caller == contract.client,
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.as_ref().map_or(false, |a| caller == *a)
            }
            ReleaseAuthorization::ClientAndArbiter | ReleaseAuthorization::MultiSig => {
                caller == contract.client
                    || contract.arbiter.as_ref().map_or(false, |a| caller == *a)
            }
        };

        if !is_authorized {
            panic!("Caller not authorized to approve milestone release");
        }
        if milestone
            .approved_by
            .as_ref()
            .map_or(false, |a| *a == caller)
        {
            panic!("Milestone already approved by this address");
        }

        milestone.approved_by = Some(caller);
        milestone.approval_timestamp = Some(env.ledger().timestamp());
        contract.milestones.set(milestone_id, milestone);
        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Funded {
            panic!("Contract not in Funded status");
        }
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let mut milestone = contract.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic!("Milestone already released");
        }

        let has_sufficient_approval = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => milestone
                .approved_by
                .as_ref()
                .map_or(false, |a| *a == contract.client),
            ReleaseAuthorization::ArbiterOnly => contract.arbiter.as_ref().map_or(false, |arb| {
                milestone.approved_by.as_ref().map_or(false, |a| *a == *arb)
            }),
            ReleaseAuthorization::ClientAndArbiter => {
                // For simplicity in this implementation, we require both if both are set?
                // No, original code suggested either.
                milestone.approved_by.as_ref().map_or(false, |a| {
                    a == &contract.client || contract.arbiter.as_ref().map_or(false, |arb| a == arb)
                })
            }
            ReleaseAuthorization::MultiSig => milestone
                .approved_by
                .as_ref()
                .map_or(false, |a| *a == contract.client),
        };

        if !has_sufficient_approval {
            panic!("Insufficient approvals for milestone release");
        }

        milestone.released = true;
        contract.milestones.set(milestone_id, milestone);

        let all_released = contract.milestones.iter().all(|m| m.released);
        if all_released {
            let old_status = contract.status;
            contract.status = ContractStatus::Completed;
            Self::update_status_index(
                &env,
                contract_id,
                Some(old_status),
                ContractStatus::Completed,
            );
        }

        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::load_contract(&env, contract_id)
    }

    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Returns a list of contract IDs associated with a participant (client or freelancer).
    ///
    /// # Arguments
    /// * `participant` - The Address of the participant to query.
    pub fn get_contracts_by_participant(env: Env, participant: Address) -> Vec<u32> {
        env.storage()
            .persistent()
            .get(&DataKey::ParticipantContracts(participant))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns a list of contract IDs that currently have the specified status.
    ///
    /// # Arguments
    /// * `status` - The ContractStatus to query.
    pub fn get_contracts_by_status(env: Env, status: ContractStatus) -> Vec<u32> {
        env.storage()
            .persistent()
            .get(&DataKey::StatusContracts(status as u32))
            .unwrap_or(Vec::new(&env))
    }
}

impl Escrow {
    fn read_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("Pause controls are not initialized"))
    }

    fn require_admin(env: &Env) {
        Self::read_admin(env).require_auth();
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

    fn next_contract_id(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(0)
    }

    fn load_contract(env: &Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| panic!("Contract not found"))
    }

    fn save_contract(env: &Env, contract_id: u32, contract: &EscrowContractData) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), contract);
    }

    // Indexing helpers
    fn add_to_participant_index(env: &Env, participant: &Address, contract_id: u32) {
        let key = DataKey::ParticipantContracts(participant.clone());
        let mut contracts: Vec<u32> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        contracts.push_back(contract_id);
        env.storage().persistent().set(&key, &contracts);
    }

    fn update_status_index(
        env: &Env,
        contract_id: u32,
        old_status: Option<ContractStatus>,
        new_status: ContractStatus,
    ) {
        if let Some(old) = old_status {
            let old_key = DataKey::StatusContracts(old as u32);
            let mut old_list: Vec<u32> = env
                .storage()
                .persistent()
                .get(&old_key)
                .unwrap_or(Vec::new(env));
            if let Some(idx) = old_list.iter().position(|id| id == contract_id) {
                old_list.remove(idx as u32);
                env.storage().persistent().set(&old_key, &old_list);
            }
        }

        let new_key = DataKey::StatusContracts(new_status as u32);
        let mut new_list: Vec<u32> = env
            .storage()
            .persistent()
            .get(&new_key)
            .unwrap_or(Vec::new(env));
        new_list.push_back(contract_id);
        env.storage().persistent().set(&new_key, &new_list);
    }
}
