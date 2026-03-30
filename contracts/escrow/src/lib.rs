#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

const DEFAULT_MIN_MILESTONE_AMOUNT: i128 = 1;
const DEFAULT_MAX_MILESTONES: u32 = 16;
const DEFAULT_MIN_REPUTATION_RATING: i128 = 1;
const DEFAULT_MAX_REPUTATION_RATING: i128 = 5;

pub const MAINNET_PROTOCOL_VERSION: u32 = 1_000_000;
pub const MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS: i128 = 1_000_000_000_000;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
}

impl ContractStatus {
    pub fn can_transition_to(self, next: ContractStatus) -> bool {
        if self == next {
            return true;
        }

        match (self, next) {
            (ContractStatus::Created, ContractStatus::Funded) => true,
            (ContractStatus::Funded, ContractStatus::Completed) => true,
            (ContractStatus::Funded, ContractStatus::Disputed) => true,
            (ContractStatus::Disputed, ContractStatus::Completed) => true,
            _ => false,
        }
    }

    pub fn assert_can_transition_to(self, next: ContractStatus) {
        if !self.can_transition_to(next) {
            panic!("Invalid contract status transition");
        }
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContract {
    pub client: Address,
    pub freelancer: Address,
    pub milestones: Vec<Milestone>,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub status: ContractStatus,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationRecord {
    pub completed_contracts: u32,
    pub total_rating: i128,
    pub last_rating: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolParameters {
    pub min_milestone_amount: i128,
    pub max_milestones: u32,
    pub min_reputation_rating: i128,
    pub max_reputation_rating: i128,
}

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
    fn next_contract_id(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    }

    fn save_next_contract_id(env: &Env, id: u32) {
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(id + 1));
    }

    fn load_contract(env: &Env, contract_id: u32) -> EscrowContract {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| panic!("contract not found"))
    }

    fn save_contract(env: &Env, contract_id: u32, contract: &EscrowContract) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), contract);
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

    fn governance_admin(env: &Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::GovernanceAdmin)
    }

    fn pending_governance_admin(env: &Env) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingGovernanceAdmin)
            .unwrap_or(None)
    }

    fn add_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let key = DataKey::PendingReputationCredits(freelancer.clone());
        let current = env.storage().persistent().get::<_, u32>(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + 1));
    }

    fn validate_protocol_parameters(
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) {
        if min_milestone_amount <= 0 {
            panic!("min milestone amount must be positive");
        }
        if max_milestones == 0 {
            panic!("max milestones must be positive");
        }
        if min_reputation_rating < DEFAULT_MIN_REPUTATION_RATING
            || min_reputation_rating > DEFAULT_MAX_REPUTATION_RATING
        {
            panic!("min reputation rating out of bounds");
        }
        if max_reputation_rating < DEFAULT_MIN_REPUTATION_RATING
            || max_reputation_rating > DEFAULT_MAX_REPUTATION_RATING
        {
            panic!("max reputation rating out of bounds");
        }
        if min_reputation_rating > max_reputation_rating {
            panic!("min reputation rating cannot exceed max reputation rating");
        }
    }

    pub fn get_protocol_parameters(env: Env) -> ProtocolParameters {
        Self::protocol_parameters(&env)
    }

    pub fn get_governance_admin(env: Env) -> Option<Address> {
        Self::governance_admin(&env)
    }

    pub fn get_pending_governance_admin(env: Env) -> Option<Address> {
        Self::pending_governance_admin(&env)
    }

    pub fn get_mainnet_readiness_info(env: Env) -> MainnetReadinessInfo {
        let params = Self::protocol_parameters(&env);
        MainnetReadinessInfo {
            protocol_version: MAINNET_PROTOCOL_VERSION,
            max_escrow_total_stroops: MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS,
            min_milestone_amount: params.min_milestone_amount,
            max_milestones: params.max_milestones,
            min_reputation_rating: params.min_reputation_rating,
            max_reputation_rating: params.max_reputation_rating,
        }
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
        if Self::governance_admin(&env).is_some() {
            panic!("governance already initialized");
        }
        Self::validate_protocol_parameters(
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );

        let params = ProtocolParameters {
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        };

        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &params);
        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &Option::<Address>::None);

        true
    }

    pub fn update_protocol_parameters(
        env: Env,
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> bool {
        let admin =
            Self::governance_admin(&env).unwrap_or_else(|| panic!("governance is not initialized"));
        admin.require_auth();
        Self::validate_protocol_parameters(
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        );

        let params = ProtocolParameters {
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        };

        env.storage()
            .persistent()
            .set(&DataKey::ProtocolParameters, &params);
        true
    }

    pub fn propose_governance_admin(env: Env, next_admin: Address) -> bool {
        let admin =
            Self::governance_admin(&env).unwrap_or_else(|| panic!("governance is not initialized"));
        admin.require_auth();

        if next_admin == admin {
            panic!("next governance admin must differ from current admin");
        }

        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &Some(next_admin));
        true
    }

    pub fn accept_governance_admin(env: Env) -> bool {
        let pending = Self::pending_governance_admin(&env)
            .unwrap_or_else(|| panic!("no pending governance admin"));
        pending.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::GovernanceAdmin, &pending);
        env.storage()
            .persistent()
            .set(&DataKey::PendingGovernanceAdmin, &Option::<Address>::None);
        true
    }

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

        if milestone_amounts.len() == 0 {
            panic!("at least one milestone is required");
        }

        let params = Self::protocol_parameters(&env);
        if milestone_amounts.len() > params.max_milestones {
            panic!("milestone count exceeds governed limit");
        }

        let mut total_amount = 0_i128;
        let mut milestones = Vec::new(&env);
        let mut index = 0_u32;
        while index < milestone_amounts.len() {
            let amount = milestone_amounts.get(index).unwrap();
            if amount < params.min_milestone_amount {
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
        let contract = EscrowContract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            milestones,
            total_amount,
            funded_amount: 0,
            released_amount: 0,
            status: ContractStatus::Created,
        };

        Self::save_contract(&env, contract_id, &contract);
        Self::save_next_contract_id(&env, contract_id);

        contract_id
    }

    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        if amount <= 0 {
            panic!("deposit amount must be positive");
        }

        let mut contract = Self::load_contract(&env, contract_id);

        if contract.status != ContractStatus::Created {
            panic!("contract is not awaiting funding");
        }

        if amount != contract.total_amount {
            panic!("deposit must match milestone total");
        }

        contract.funded_amount = amount;
        contract.status = ContractStatus::Funded;
        Self::save_contract(&env, contract_id, &contract);

        true
    }

    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        let mut contract = Self::load_contract(&env, contract_id);

        if contract.status != ContractStatus::Funded {
            panic!("contract is not funded");
        }

        if milestone_id >= contract.milestones.len() {
            panic!("invalid milestone id");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic!("milestone already released");
        }

        let mut updated_milestone = milestone.clone();
        updated_milestone.released = true;
        contract.milestones.set(milestone_id, updated_milestone);
        contract.released_amount = contract
            .released_amount
            .checked_add(milestone.amount)
            .unwrap_or_else(|| panic!("released amount overflow"));

        let mut all_released = true;
        let mut index = 0_u32;
        while index < contract.milestones.len() {
            if !contract.milestones.get(index).unwrap().released {
                all_released = false;
                break;
            }
            index += 1;
        }

        if all_released {
            contract.status = ContractStatus::Completed;
            Self::add_pending_reputation_credit(&env, &contract.freelancer);
        }

        Self::save_contract(&env, contract_id, &contract);
        true
    }

    pub fn issue_reputation(env: Env, freelancer: Address, rating: i128) -> bool {
        freelancer.require_auth();

        let params = Self::protocol_parameters(&env);
        if rating < params.min_reputation_rating || rating > params.max_reputation_rating {
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

    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContract {
        Self::load_contract(&env, contract_id)
    }

    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get::<_, u32>(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }
}

#[cfg(test)]
mod test;
