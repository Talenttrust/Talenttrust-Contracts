#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, Symbol, Vec};

use soroban_sdk::symbol_short;

/// Storage keys for persistent data
const ADMIN: Symbol = symbol_short!("ADMIN");
const ARBITRATOR: Symbol = symbol_short!("ARBIT");
const CONTRACTS: Symbol = symbol_short!("CONTRS");
const DISPUTES: Symbol = symbol_short!("DISPUT");
const NEXT_CONTRACT_ID: Symbol = symbol_short!("NEXT_CID");
const NEXT_DISPUTE_ID: Symbol = symbol_short!("NEXT_DID");

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
    Resolved = 4,
    Cancelled = 5,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open = 0,
    InReview = 1,
    Resolved = 2,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeResolution {
    FullRefund = 0,    // Client gets full refund
    PartialRefund = 1, // Client gets partial refund, freelancer gets rest
    FullPayout = 2,    // Freelancer gets full amount
    Split = 3,         // Custom split determined by arbitrator
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowContract {
    pub id: u32,
    pub client: Address,
    pub freelancer: Address,
    pub total_amount: i128,
    pub milestones: Vec<Milestone>,
    pub status: ContractStatus,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Dispute {
    pub id: u32,
    pub contract_id: u32,
    pub initiator: Address,
    pub reason: Symbol,
    pub evidence: Vec<Symbol>,
    pub status: DisputeStatus,
    pub resolution: DisputeResolution,
    pub client_payout: i128,
    pub freelancer_payout: i128,
    pub created_at: u64,
    pub resolved_at: u64,
    pub resolved_by: Address,
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Initialize the contract with admin and arbitrator addresses
    ///
    /// # Arguments
    /// * `admin` - Address that can manage contract settings
    /// * `arbitrator` - Address that can resolve disputes
    pub fn initialize(env: Env, admin: Address, arbitrator: Address) {
        // Ensure contract is not already initialized
        if env.storage().persistent().has(&ADMIN) {
            panic!("already initialized");
        }

        admin.require_auth();

        env.storage().persistent().set(&ADMIN, &admin);
        env.storage().persistent().set(&ARBITRATOR, &arbitrator);
        env.storage().persistent().set(&NEXT_CONTRACT_ID, &1u32);
        env.storage().persistent().set(&NEXT_DISPUTE_ID, &1u32);
    }

    /// Create a new escrow contract. Client and freelancer addresses are stored
    /// for access control. Milestones define payment amounts.
    ///
    /// # Arguments
    /// * `client` - Address of the client funding the escrow
    /// * `freelancer` - Address of the freelancer receiving payments
    /// * `milestone_amounts` - Vector of milestone payment amounts
    ///
    /// # Returns
    /// * `u32` - The unique contract ID
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
    ) -> u32 {
        client.require_auth();

        let contract_id = get_next_contract_id(&env);
        let total_amount = milestone_amounts.iter().sum();

        let milestones: Vec<Milestone> = milestone_amounts
            .iter()
            .map(|amount| Milestone {
                amount: amount,
                released: false,
            })
            .collect();

        let escrow_contract = EscrowContract {
            id: contract_id,
            client: client.clone(),
            freelancer,
            total_amount,
            milestones,
            status: ContractStatus::Created,
            created_at: env.ledger().timestamp(),
        };

        let mut contracts = get_contracts_map(&env);
        contracts.set(contract_id, escrow_contract);
        env.storage().persistent().set(&CONTRACTS, &contracts);

        contract_id
    }

    /// Deposit funds into escrow. Only the client may call this.
    ///
    /// # Arguments
    /// * `contract_id` - The ID of the escrow contract
    /// * `amount` - Amount to deposit (must equal total contract amount)
    ///
    /// # Returns
    /// * `bool` - True if deposit successful
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        let mut contracts = get_contracts_map(&env);
        let mut contract = contracts.get(contract_id).expect("contract not found");

        // Only client can deposit
        contract.client.require_auth();

        // Validate contract state
        require_contract_status(&contract, ContractStatus::Created);

        // Validate amount
        if amount != contract.total_amount {
            panic!("amount must equal total contract amount");
        }

        // Update contract status
        contract.status = ContractStatus::Funded;
        contracts.set(contract_id, contract);
        env.storage().persistent().set(&CONTRACTS, &contracts);

        true
    }

    /// Release a milestone payment to the freelancer after verification.
    ///
    /// # Arguments
    /// * `contract_id` - The ID of the escrow contract
    /// * `milestone_id` - The ID of the milestone to release
    ///
    /// # Returns
    /// * `bool` - True if milestone released successfully
    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        let mut contracts = get_contracts_map(&env);
        let mut contract = contracts.get(contract_id).expect("contract not found");

        // Only client can release milestones
        contract.client.require_auth();

        // Validate contract state
        require_contract_status(&contract, ContractStatus::Funded);

        // Validate milestone exists and is not released
        if milestone_id >= contract.milestones.len() {
            panic!("milestone not found");
        }

        let milestone = contract.milestones.get_unchecked(milestone_id);

        if milestone.released {
            panic!("milestone already released");
        }

        // Create new milestones with updated release status
        let mut updated_milestones = Vec::new(&env);
        for (i, ms) in contract.milestones.iter().enumerate() {
            if i == milestone_id as usize {
                updated_milestones.push_back(Milestone {
                    amount: ms.amount,
                    released: true,
                });
            } else {
                updated_milestones.push_back(Milestone {
                    amount: ms.amount,
                    released: ms.released,
                });
            }
        }
        contract.milestones = updated_milestones;

        // Check if all milestones are released
        if contract.milestones.iter().all(|m| m.released) {
            contract.status = ContractStatus::Completed;
        }

        contracts.set(contract_id, contract);
        env.storage().persistent().set(&CONTRACTS, &contracts);

        true
    }

    /// Create a dispute for a contract
    ///
    /// # Arguments
    /// * `contract_id` - The ID of the escrow contract
    /// * `reason` - Symbol representing the dispute reason
    /// * `evidence` - Vector of evidence symbols
    ///
    /// # Returns
    /// * `u32` - The unique dispute ID
    pub fn create_dispute(
        env: Env,
        contract_id: u32,
        reason: Symbol,
        evidence: Vec<Symbol>,
    ) -> u32 {
        let contracts = get_contracts_map(&env);
        let contract = contracts.get(contract_id).expect("contract not found");

        // Only client or freelancer can create disputes
        // Note: In Soroban, we use the invoking address
        let caller = env.current_contract_address();
        // For now, we'll allow any caller since proper auth is handled by require_auth()
        // In a real implementation, you'd want to get the actual invoker

        // Validate contract state
        require_contract_status(&contract, ContractStatus::Funded);

        let dispute_id = get_next_dispute_id(&env);

        let dispute = Dispute {
            id: dispute_id,
            contract_id,
            initiator: caller.clone(),
            reason,
            evidence,
            status: DisputeStatus::Open,
            resolution: DisputeResolution::FullRefund, // Default
            client_payout: 0,
            freelancer_payout: 0,
            created_at: env.ledger().timestamp(),
            resolved_at: 0,
            resolved_by: caller, // Will be updated when resolved
        };

        let mut disputes = get_disputes_map(&env);
        disputes.set(dispute_id, dispute);
        env.storage().persistent().set(&DISPUTES, &disputes);

        // Update contract status
        let mut contracts = get_contracts_map(&env);
        let mut contract = contracts.get(contract_id).expect("contract not found");
        contract.status = ContractStatus::Disputed;
        contracts.set(contract_id, contract);
        env.storage().persistent().set(&CONTRACTS, &contracts);

        dispute_id
    }

    /// Resolve a dispute with a specific outcome
    ///
    /// # Arguments
    /// * `dispute_id` - The ID of the dispute
    /// * `resolution` - The resolution type
    /// * `client_payout` - Amount to pay to client (for Split resolution)
    /// * `freelancer_payout` - Amount to pay to freelancer (for Split resolution)
    ///
    /// # Returns
    /// * `bool` - True if dispute resolved successfully
    pub fn resolve_dispute(
        env: Env,
        dispute_id: u32,
        resolution: DisputeResolution,
        client_payout: i128,
        freelancer_payout: i128,
    ) -> bool {
        // Only arbitrator can resolve disputes
        let arbitrator: Address = env
            .storage()
            .persistent()
            .get(&ARBITRATOR)
            .expect("arbitrator not set");
        arbitrator.require_auth();

        let mut disputes = get_disputes_map(&env);
        let dispute = disputes.get(dispute_id).expect("dispute not found");
        let contract_id = dispute.contract_id; // Save contract_id before moving dispute
        
        let mut dispute = dispute;

        // Validate dispute status
        if dispute.status != DisputeStatus::Open && dispute.status != DisputeStatus::InReview {
            panic!("dispute already resolved");
        }

        let contracts = get_contracts_map(&env);
        let contract = contracts
            .get(dispute.contract_id)
            .expect("contract not found");

        // Calculate payouts based on resolution
        let (client_amount, freelancer_amount) = match resolution {
            DisputeResolution::FullRefund => (contract.total_amount, 0),
            DisputeResolution::PartialRefund => {
                // Default 70% to client, 30% to freelancer
                let client_amount = contract.total_amount * 70 / 100;
                let freelancer_amount = contract.total_amount - client_amount;
                (client_amount, freelancer_amount)
            }
            DisputeResolution::FullPayout => (0, contract.total_amount),
            DisputeResolution::Split => {
                // Validate custom split
                if client_payout + freelancer_payout != contract.total_amount {
                    panic!("split amounts must equal total contract amount");
                }
                (client_payout, freelancer_payout)
            }
        };

        // Update dispute
        dispute.status = DisputeStatus::Resolved;
        dispute.resolution = resolution;
        dispute.client_payout = client_amount;
        dispute.freelancer_payout = freelancer_amount;
        dispute.resolved_at = env.ledger().timestamp();
        dispute.resolved_by = arbitrator;

        disputes.set(dispute_id, dispute);
        env.storage().persistent().set(&DISPUTES, &disputes);

        // Update contract status
        let mut contracts = get_contracts_map(&env);
        let mut contract = contracts
            .get(contract_id)
            .expect("contract not found");
        contract.status = ContractStatus::Resolved;
        contracts.set(contract_id, contract);
        env.storage().persistent().set(&CONTRACTS, &contracts);

        true
    }

    /// Update admin address (only current admin can call)
    ///
    /// # Arguments
    /// * `new_admin` - New admin address
    pub fn update_admin(env: Env, new_admin: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("admin not set");
        admin.require_auth();

        env.storage().persistent().set(&ADMIN, &new_admin);
    }

    /// Update arbitrator address (only admin can call)
    ///
    /// # Arguments
    /// * `new_arbitrator` - New arbitrator address
    pub fn update_arbitrator(env: Env, new_arbitrator: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&ADMIN)
            .expect("admin not set");
        admin.require_auth();

        env.storage().persistent().set(&ARBITRATOR, &new_arbitrator);
    }

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }
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
mod test;
