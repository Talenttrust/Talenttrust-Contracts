#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Vec,
};

// --- Constants ---

pub const MAX_FEE_BASIS_POINTS: u32 = 10000;
pub const DEFAULT_FEE_BASIS_POINTS: u32 = 250;
pub const MIN_TIMEOUT_SECONDS: u64 = 86_400; // 1 day
pub const MAX_TIMEOUT_SECONDS: u64 = 31_536_000; // 365 days
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 2_592_000; // 30 days

// --- Data Types ---

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly = 0,
    ArbiterOnly = 1,
    ClientAndArbiter = 2,
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
pub struct Amendment {
    pub new_amount: i128,
    pub proposer: Address,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Paused,
    NextContractId,
    Contract(u32),
    Amendment(u32, u32), // (contract_id, milestone_id)
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    Unauthorized = 1,
    ContractNotFound = 2,
    MilestoneNotFound = 3,
    MilestoneAlreadyReleased = 4,
    InvalidAmount = 5,
    InvalidState = 6,
    ArithmeticOverflow = 7,
    AmendmentNotFound = 8,
    ContractPaused = 9,
}

// --- Contract ---

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Initializes the contract with an admin.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::NextContractId, &1u32);
    }

    // --- Admin Functions ---

    pub fn pause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
    }

    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    // --- Escrow Lifecycle ---

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

        if milestone_amounts.is_empty() {
            panic!("Milestones required");
        }

        let mut milestones = Vec::new(&env);
        for amount in milestone_amounts.iter() {
            if amount <= 0 {
                panic!("Invalid amount");
            }
            milestones.push_back(Milestone {
                amount,
                released: false,
                approved_by: None,
                approval_timestamp: None,
            });
        }

        let contract_id = Self::get_and_inc_next_id(&env);
        let contract_data = EscrowContractData {
            client,
            freelancer,
            arbiter,
            milestones,
            status: ContractStatus::Created,
            release_auth,
            created_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract_data);

        contract_id
    }

    pub fn deposit_funds(env: Env, contract_id: u32) {
        Self::ensure_not_paused(&env);
        let mut contract = Self::load_contract(&env, contract_id);
        contract.client.require_auth();

        if contract.status != ContractStatus::Created {
            panic!("Invalid state");
        }

        // In a real implementation, we would transfer tokens here.
        // For this task, we just update the status.
        contract.status = ContractStatus::Funded;
        Self::save_contract(&env, contract_id, &contract);
    }

    pub fn approve_milestone_release(env: Env, _contract_id: u32, _milestone_id: u32) {
        Self::ensure_not_paused(&env);
        // This function is kept for backward compatibility or placeholder logic.
        // It should call approve_milestone or similar logic with proper Address.
        // For simplicity in this task, we assume the use of approve_milestone.
    }

    /// Explicitly authorized milestone approval
    pub fn approve_milestone(env: Env, contract_id: u32, milestone_id: u32, approver: Address) {
        Self::ensure_not_paused(&env);
        approver.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Funded {
            panic!("Invalid state");
        }

        let mut milestone = contract
            .milestones
            .get(milestone_id)
            .ok_or(EscrowError::MilestoneNotFound)
            .unwrap();

        if milestone.released {
            panic!("Already released");
        }

        // Check authorization
        let is_authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => approver == contract.client,
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.as_ref().map_or(false, |a| &approver == a)
            }
            ReleaseAuthorization::ClientAndArbiter => {
                approver == contract.client || contract.arbiter.as_ref().map_or(false, |a| &approver == a)
            }
        };

        if !is_authorized {
            panic!("Unauthorized");
        }

        milestone.approved_by = Some(approver);
        milestone.approval_timestamp = Some(env.ledger().timestamp());
        contract.milestones.set(milestone_id, milestone);
        Self::save_contract(&env, contract_id, &contract);
    }

    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) {
        Self::ensure_not_paused(&env);
        let mut contract = Self::load_contract(&env, contract_id);

        let mut milestone = contract
            .milestones
            .get(milestone_id)
            .unwrap_or_else(|| panic!("Milestone not found"));

        if milestone.released {
            panic!("Already released");
        }

        if milestone.approved_by.is_none() {
            panic!("Not approved");
        }

        milestone.released = true;
        contract.milestones.set(milestone_id, milestone);

        // Check if all milestones are released
        let mut all_done = true;
        for m in contract.milestones.iter() {
            if !m.released {
                all_done = false;
                break;
            }
        }
        if all_done {
            contract.status = ContractStatus::Completed;
        }

        Self::save_contract(&env, contract_id, &contract);
    }

    // --- Amendment Process ---

    /// Propose an amendment to a milestone amount.
    /// Can be called by either the client or the freelancer.
    pub fn propose_milestone_amendment(
        env: Env,
        contract_id: u32,
        milestone_id: u32,
        proposer: Address,
        new_amount: i128,
    ) {
        Self::ensure_not_paused(&env);
        proposer.require_auth();

        let contract = Self::load_contract(&env, contract_id);
        if contract.status != ContractStatus::Funded {
            panic!("Contract must be Funded");
        }

        if proposer != contract.client && proposer != contract.freelancer {
            panic!("Unauthorized: only client or freelancer can propose amendments");
        }

        let milestone = contract
            .milestones
            .get(milestone_id)
            .unwrap_or_else(|| panic!("Milestone not found"));

        if milestone.released {
            panic!("Cannot amend a released milestone");
        }

        if new_amount <= 0 {
            panic!("Invalid amount");
        }

        let amendment = Amendment {
            new_amount,
            proposer,
            created_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Amendment(contract_id, milestone_id), &amendment);
    }

    /// Approve a proposed amendment.
    /// Must be called by the party that DID NOT propose the amendment.
    pub fn approve_milestone_amendment(
        env: Env,
        contract_id: u32,
        milestone_id: u32,
        approver: Address,
    ) {
        Self::ensure_not_paused(&env);
        approver.require_auth();

        let mut contract = Self::load_contract(&env, contract_id);
        let amendment: Amendment = env
            .storage()
            .persistent()
            .get(&DataKey::Amendment(contract_id, milestone_id))
            .unwrap_or_else(|| panic!("Amendment not found"));

        if approver == amendment.proposer {
            panic!("Proposer cannot approve their own amendment");
        }

        if approver != contract.client && approver != contract.freelancer {
            panic!("Unauthorized");
        }

        let mut milestone = contract
            .milestones
            .get(milestone_id)
            .unwrap_or_else(|| panic!("Milestone not found"));

        if milestone.released {
            panic!("Already released");
        }

        // Apply amendment
        milestone.amount = amendment.new_amount;
        contract.milestones.set(milestone_id, milestone);
        
        // Remove the amendment proposal
        env.storage()
            .persistent()
            .remove(&DataKey::Amendment(contract_id, milestone_id));

        Self::save_contract(&env, contract_id, &contract);
    }

    /// Reject a proposed amendment.
    /// Can be called by either party to cancel the proposal.
    pub fn reject_milestone_amendment(
        env: Env,
        contract_id: u32,
        milestone_id: u32,
        caller: Address,
    ) {
        Self::ensure_not_paused(&env);
        caller.require_auth();

        let contract = Self::load_contract(&env, contract_id);
        if caller != contract.client && caller != contract.freelancer {
            panic!("Unauthorized");
        }

        if !env
            .storage()
            .persistent()
            .has(&DataKey::Amendment(contract_id, milestone_id))
        {
            panic!("Amendment not found");
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Amendment(contract_id, milestone_id));
    }

    // --- Getters ---

    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::load_contract(&env, contract_id)
    }

    pub fn get_amendment(env: Env, contract_id: u32, milestone_id: u32) -> Option<Amendment> {
        env.storage()
            .persistent()
            .get(&DataKey::Amendment(contract_id, milestone_id))
    }

    // --- Internal Helpers ---

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

    fn get_and_inc_next_id(env: &Env) -> u32 {
        let id: u32 = env.storage().instance().get(&DataKey::NextContractId).unwrap();
        env.storage().instance().set(&DataKey::NextContractId, &(id + 1));
        id
    }

    fn require_admin(env: &Env) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
    }

    fn ensure_not_paused(env: &Env) {
        let paused: bool = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
        if paused {
            panic!("Contract paused");
        }
    }
}

#[cfg(test)]
mod test;
