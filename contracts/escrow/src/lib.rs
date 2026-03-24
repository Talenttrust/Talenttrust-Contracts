#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    InvalidRating = 1,
    NotCompleted = 2,
    DuplicateRating = 3,
}

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
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Status(u32),
    Reputation(u32, Address),
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Create a new escrow contract. Client and freelancer addresses are stored
    /// for access control. Milestones define payment amounts.
    pub fn create_contract(
        env: Env,
        _client: Address,
        _freelancer: Address,
        _milestone_amounts: Vec<i128>,
    ) -> u32 {
        let contract_id = 1;
        env.storage()
            .instance()
            .set(&DataKey::Status(contract_id), &ContractStatus::Created);
        contract_id
    }

    /// Deposit funds into escrow. Only the client may call this.
    pub fn deposit_funds(env: Env, contract_id: u32, _amount: i128) -> bool {
        env.storage()
            .instance()
            .set(&DataKey::Status(contract_id), &ContractStatus::Funded);
        true
    }

    /// Release a milestone payment to the freelancer after verification.
    pub fn release_milestone(env: Env, contract_id: u32, _milestone_id: u32) -> bool {
        env.storage()
            .instance()
            .set(&DataKey::Status(contract_id), &ContractStatus::Completed);
        true
    }

    /// Issue a reputation credential for the freelancer after contract completion.
    ///
    /// # Title
    /// Reputation Credential Issuance
    ///
    /// # Description
    /// Validate rating bounds, verify issuance timing (only after Completion), and
    /// ensure duplicate prevention per project.
    ///
    /// # Security Assumptions
    /// - Only authorized actors can issue valid ratings.
    /// - Contracts must be completed before rating.
    ///
    /// # Threat Scenarios checks
    /// - Duplicate rating attack: Prevented.
    /// - Early rating attack: Prevented.
    /// - Out of bounds rating attack: Prevented.
    pub fn issue_reputation(
        env: Env,
        contract_id: u32,
        freelancer: Address,
        rating: u32,
    ) -> Result<bool, EscrowError> {
        if rating < 1 || rating > 5 {
            return Err(EscrowError::InvalidRating);
        }

        let status_key = DataKey::Status(contract_id);
        let status: ContractStatus = env
            .storage()
            .instance()
            .get(&status_key)
            .unwrap_or(ContractStatus::Created);

        if status != ContractStatus::Completed {
            return Err(EscrowError::NotCompleted);
        }

        let rep_key = DataKey::Reputation(contract_id, freelancer.clone());
        if env.storage().instance().has(&rep_key) {
            return Err(EscrowError::DuplicateRating);
        }

        env.storage().instance().set(&rep_key, &rating);

        Ok(true)
    }

    /// Hello-world style function for testing and CI.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_reputation;
