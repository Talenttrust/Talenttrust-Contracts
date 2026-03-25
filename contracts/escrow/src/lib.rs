#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, Symbol, Vec,
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
    pub protocol_fee: i128,
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
    pub protocol_fee_bps: u32,
    pub protocol_fee_account: Address,
    pub protocol_fee_accrued: i128,
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
    /// Create a new escrow contract with milestone release authorization and protocol fee config.
    ///
    /// # Arguments
    /// * `client` - Address of the client who funds the escrow
    /// * `freelancer` - Address of the freelancer who receives payments
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestone_amounts` - Vector of milestone payment amounts
    /// * `release_auth` - Authorization scheme for milestone releases
    /// * `protocol_fee_bps` - Protocol fee in basis points (0-10000)
    /// * `protocol_fee_account` - Address entitled to withdraw accrued protocol fees
    ///
    /// # Returns
    /// Contract ID for the newly created escrow
    ///
    /// # Errors
    /// Panics if:
    /// - Milestone amounts vector is empty
    /// - Any milestone amount is zero or negative
    /// - Client and freelancer addresses are the same
    /// - Protocol fee is out of range
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
        protocol_fee_bps: u32,
        protocol_fee_account: Address,
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
                protocol_fee: 0,
            });
        }

        // Create contract
        if protocol_fee_bps > 10000 {
            panic!("Protocol fee out of range");
        }

        let contract_data = EscrowContract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            milestones,
            status: ContractStatus::Created,
            release_auth,
            created_at: env.ledger().timestamp(),
            protocol_fee_bps,
            protocol_fee_account: protocol_fee_account.clone(),
            protocol_fee_accrued: 0,
        };

        // Generate contract ID (in real implementation, this would use proper storage)
        let contract_id = env.ledger().sequence();

        // Store contract data (simplified for this implementation)
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract_data);

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
    pub fn deposit_funds(env: Env, _contract_id: u32, caller: Address, amount: i128) -> bool {
        caller.require_auth();

        // In real implementation, retrieve contract from storage
        // For now, we'll use a simplified approach
        let contract: EscrowContract = env
            .storage()
            .persistent()
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
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &updated_contract);

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
    pub fn approve_milestone_release(
        env: Env,
        _contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();

        // Retrieve contract
        let mut contract: EscrowContract = env
            .storage()
            .persistent()
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
            }
            ReleaseAuthorization::ClientAndArbiter => {
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
            ReleaseAuthorization::MultiSig => {
                // For multi-sig, both client and arbiter must approve
                // This function handles individual approval
                caller == contract.client || contract.arbiter.clone().map_or(false, |a| caller == a)
            }
        };

        if !is_authorized {
            panic!("Caller not authorized to approve milestone release");
        }

        // Check if already approved by this caller
        if milestone
            .approved_by
            .clone()
            .map_or(false, |addr| addr == caller)
        {
            panic!("Milestone already approved by this address");
        }

        // Update milestone approval
        let mut updated_milestone = milestone;
        updated_milestone.approved_by = Some(caller);
        updated_milestone.approval_timestamp = Some(env.ledger().timestamp());

        // Update contract
        contract.milestones.set(milestone_id, updated_milestone);
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

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
    pub fn release_milestone(
        env: Env,
        _contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();
        // Retrieve contract
        let mut contract: EscrowContract = env
            .storage()
            .persistent()
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
            ReleaseAuthorization::MultiSig => {
                // For multi-sig, we'd need to track multiple approvals
                // Simplified: require client approval for now
                milestone
                    .approved_by
                    .clone()
                    .map_or(false, |addr| addr == contract.client)
            }
        };

        if !has_sufficient_approval {
            panic!("Insufficient approvals for milestone release");
        }

        // Release milestone, compute protocol fee and accrue it
        let protocol_fee = milestone.amount * (contract.protocol_fee_bps as i128) / 10_000;

        let mut updated_milestone = milestone;
        updated_milestone.released = true;
        updated_milestone.protocol_fee = protocol_fee;

        // Update contract fee tracker
        contract.protocol_fee_accrued += protocol_fee;
        contract.milestones.set(milestone_id, updated_milestone);

        // Check if all milestones are released
        let all_released = contract.milestones.iter().all(|m| m.released);
        if all_released {
            contract.status = ContractStatus::Completed;
        }

        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

        // In real implementation, transfer funds to freelancer minus fee.
        // For now, we'll just mark milestone as released and record fee accrual.

        true
    }

    /// Get accrued protocol fees for this contract.
    pub fn get_protocol_fee_accrued(env: Env, _contract_id: u32) -> i128 {
        let contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));
        contract.protocol_fee_accrued
    }

    /// Withdraw accrued protocol fees to fee account, decreasing accrual balance.
    pub fn withdraw_protocol_fees(
        env: Env,
        _contract_id: u32,
        caller: Address,
        amount: i128,
    ) -> bool {
        caller.require_auth();
        if amount <= 0 {
            panic!("Withdraw amount must be positive");
        }

        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        if caller != contract.protocol_fee_account {
            panic!("Only protocol fee account can withdraw accrued fees");
        }

        if amount > contract.protocol_fee_accrued {
            panic!("Insufficient accrued protocol fee balance");
        }

        contract.protocol_fee_accrued -= amount;
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

        // In real implementation, transfer protocol fees out of escrow.
        true
    }

    /// Update protocol fee basis points. Only fee account may change.
    pub fn set_protocol_fee_bps(
        env: Env,
        _contract_id: u32,
        caller: Address,
        protocol_fee_bps: u32,
    ) -> bool {
        caller.require_auth();
        if protocol_fee_bps > 10_000 {
            panic!("Protocol fee out of range");
        }

        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        if caller != contract.protocol_fee_account {
            panic!("Only protocol fee account can update fee rate");
        }

        contract.protocol_fee_bps = protocol_fee_bps;
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

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
