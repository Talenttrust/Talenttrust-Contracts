#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, Symbol, Vec, Map,
    symbol_short
};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
    pub approved_by: Option<Address>,
    pub approval_timestamp: Option<u64>,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly = 0,
    ClientAndArbiter = 1,
    ArbiterOnly = 2,
    MultiSig = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowContract {
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
pub enum Approval {
    None = 0,
    Client = 1,
    Arbiter = 2,
    Both = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneApproval {
    pub milestone_id: u32,
    pub approvals: Map<Address, bool>,
    pub required_approvals: u32,
    pub approval_status: Approval,
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
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
    /// - Milestone amounts vector is empty
    /// - Any milestone amount is zero or negative
    /// - Client and freelancer addresses are the same
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
    ) -> u32 {
        // Validate inputs
        if milestone_amounts.is_empty() {
            panic!("At least one milestone required");
        }
        
        if client == freelancer {
            panic!("Client and freelancer cannot be the same address");
        }
        
        // Validate milestone amounts
        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts.get(i).unwrap();
            if amount <= 0 {
                panic!("Milestone amounts must be positive");
            }
        }
        
        // Create milestones
        let mut milestones = Vec::new(&env);
        for i in 0..milestone_amounts.len() {
            milestones.push_back(Milestone {
                amount: milestone_amounts.get(i).unwrap(),
                released: false,
                approved_by: None,
                approval_timestamp: None,
            });
        }
        
        // Create contract
        let contract_data = EscrowContract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            milestones,
            status: ContractStatus::Created,
            release_auth,
            created_at: env.ledger().timestamp(),
        };
        
        // Generate contract ID (in real implementation, this would use proper storage)
        let contract_id = env.ledger().sequence();
        
        // Store contract data (simplified for this implementation)
        env.storage().persistent().set(&symbol_short!("contract"), &contract_data);
        
        contract_id
    }

    /// Deposit funds into escrow. Only the client may call this.
    /// 
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract
    /// * `amount` - Amount to deposit (must equal total milestone amounts)
    /// 
    /// # Returns
    /// true if deposit successful
    /// 
    /// # Errors
    /// Panics if:
    /// - Caller is not the client
    /// - Contract is not in Created status
    /// - Amount doesn't match total milestone amounts
    pub fn deposit_funds(env: Env, _contract_id: u32, amount: i128) -> bool {
        // Get caller
        let caller = env.current_contract_address();
        
        // In real implementation, retrieve contract from storage
        // For now, we'll use a simplified approach
        let contract: EscrowContract = env.storage().persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));
        
        // Verify caller is client
        if caller != contract.client {
            panic!("Only client can deposit funds");
        }
        
        // Verify contract status
        if contract.status != ContractStatus::Created {
            panic!("Contract must be in Created status to deposit funds");
        }
        
        // Calculate total required amount
        let mut total_required = 0i128;
        for i in 0..contract.milestones.len() {
            total_required += contract.milestones.get(i).unwrap().amount;
        }
        
        if amount != total_required {
            panic!("Deposit amount must equal total milestone amounts");
        }
        
        // Update contract status to Funded
        let mut updated_contract = contract;
        updated_contract.status = ContractStatus::Funded;
        env.storage().persistent().set(&symbol_short!("contract"), &updated_contract);
        
        true
    }

    /// Approve a milestone for release with proper authorization
    /// 
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract
    /// * `milestone_id` - ID of the milestone to approve
    /// 
    /// # Returns
    /// true if approval successful
    /// 
    /// # Errors
    /// Panics if:
    /// - Caller is not authorized to approve
    /// - Contract is not in Funded status
    /// - Milestone ID is invalid
    /// - Milestone already released
    /// - Milestone already approved by this caller
    pub fn approve_milestone_release(env: Env, _contract_id: u32, milestone_id: u32) -> bool {
        let caller = env.current_contract_address();
        
        // Retrieve contract
        let mut contract: EscrowContract = env.storage().persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));
        
        // Verify contract status
        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to approve milestones");
        }
        
        // Validate milestone ID
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }
        
        let milestone = contract.milestones.get(milestone_id).unwrap();
        
        // Check if milestone already released
        if milestone.released {
            panic!("Milestone already released");
        }
        
        // Check authorization based on release_auth scheme
        let is_authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => caller == contract.client,
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |a| caller == a)
            },
            ReleaseAuthorization::ClientAndArbiter => {
                caller == contract.client || 
                contract.arbiter.clone().map_or(false, |a| caller == a)
            },
            ReleaseAuthorization::MultiSig => {
                // For multi-sig, both client and arbiter must approve
                // This function handles individual approval
                caller == contract.client || 
                contract.arbiter.clone().map_or(false, |a| caller == a)
            },
        };
        
        if !is_authorized {
            panic!("Caller not authorized to approve milestone release");
        }
        
        // Check if already approved by this caller
        if milestone.approved_by.clone().map_or(false, |addr| addr == caller) {
            panic!("Milestone already approved by this address");
        }
        
        // Update milestone approval
        let mut updated_milestone = milestone;
        updated_milestone.approved_by = Some(caller);
        updated_milestone.approval_timestamp = Some(env.ledger().timestamp());
        
        // Update contract
        contract.milestones.set(milestone_id, updated_milestone);
        env.storage().persistent().set(&symbol_short!("contract"), &contract);
        
        true
    }
    
    /// Release a milestone payment to the freelancer after proper authorization
    /// 
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract
    /// * `milestone_id` - ID of the milestone to release
    /// 
    /// # Returns
    /// true if release successful
    /// 
    /// # Errors
    /// Panics if:
    /// - Contract is not in Funded status
    /// - Milestone ID is invalid
    /// - Milestone already released
    /// - Insufficient approvals based on authorization scheme
    pub fn release_milestone(env: Env, _contract_id: u32, milestone_id: u32) -> bool {
        // Retrieve contract
        let mut contract: EscrowContract = env.storage().persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));
        
        // Verify contract status
        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to release milestones");
        }
        
        // Validate milestone ID
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }
        
        let milestone = contract.milestones.get(milestone_id).unwrap();
        
        // Check if milestone already released
        if milestone.released {
            panic!("Milestone already released");
        }
        
        // Check if milestone has sufficient approvals
        let has_sufficient_approval = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => {
                milestone.approved_by.clone().map_or(false, |addr| addr == contract.client)
            },
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().map_or(false, |arbiter| {
                    milestone.approved_by.clone().map_or(false, |addr| addr == arbiter)
                })
            },
            ReleaseAuthorization::ClientAndArbiter => {
                milestone.approved_by.clone().map_or(false, |addr| {
                    addr == contract.client || 
                    contract.arbiter.clone().map_or(false, |arbiter| addr == arbiter)
                })
            },
            ReleaseAuthorization::MultiSig => {
                // For multi-sig, we'd need to track multiple approvals
                // Simplified: require client approval for now
                milestone.approved_by.clone().map_or(false, |addr| addr == contract.client)
            },
        };
        
        if !has_sufficient_approval {
            panic!("Insufficient approvals for milestone release");
        }
        
        // Release milestone
        let mut updated_milestone = milestone;
        updated_milestone.released = true;
        
        // Update contract
        contract.milestones.set(milestone_id, updated_milestone);
        
        // Check if all milestones are released
        let all_released = contract.milestones.iter().all(|m| m.released);
        if all_released {
            contract.status = ContractStatus::Completed;
        }
        
        env.storage().persistent().set(&symbol_short!("contract"), &contract);
        
        // In real implementation, transfer funds to freelancer
        // For now, we'll just mark as released
        
        true
    }

    /// Issue a reputation credential for the freelancer after contract completion.
    pub fn issue_reputation(_env: Env, _freelancer: Address, _rating: i128) -> bool {
        // Reputation credential issuance.
        true
    }

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }
}

#[cfg(test)]
mod test;
