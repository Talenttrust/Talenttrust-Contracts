use crate::{
    ttl, Contract, ContractStatus, DataKey, Error, Escrow, EscrowArgs, EscrowClient, Milestone,
};
use soroban_sdk::{contractimpl, Env, Symbol, Vec};

#[contractimpl]
impl Escrow {
    /// Refunds unreleased milestones back to the client.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_indices` - Vector of milestone indices to refund
    ///
    /// # Returns
    /// The total amount refunded
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `EmptyRefundRequest` - If milestone_indices is empty
    /// * `DuplicateMilestoneInRefund` - If the same milestone appears multiple times
    /// * `IndexOutOfBounds` - If any milestone index is out of bounds
    /// * `AlreadyReleased` - If any milestone was already released
    /// * `AlreadyRefunded` - If any milestone was already refunded
    /// * `InsufficientFunds` - If contract doesn't have enough balance to refund
    pub fn refund_unreleased_milestones(
        env: Env,
        contract_id: u32,
        milestone_indices: Vec<u32>,
    ) -> i128 {
        if milestone_indices.is_empty() {
            env.panic_with_error(Error::EmptyRefundRequest);
        }

        for i in 0..milestone_indices.len() {
            for j in (i + 1)..milestone_indices.len() {
                if milestone_indices.get(i).unwrap() == milestone_indices.get(j).unwrap() {
                    env.panic_with_error(Error::DuplicateMilestoneInRefund);
                }
            }
        }

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);

        Self::require_not_finalized(&env, contract_id);

        contract.client.require_auth();

        let mut milestones: Vec<Milestone> = ttl::load_milestones(&env, contract_id);

        let mut total_refund_amount: i128 = 0;

        for idx in milestone_indices.iter() {
            if idx >= milestones.len() {
                env.panic_with_error(Error::IndexOutOfBounds);
            }

            let milestone = milestones.get(idx).unwrap();

            if milestone.released {
                env.panic_with_error(Error::AlreadyReleased);
            }

            if milestone.refunded {
                env.panic_with_error(Error::AlreadyRefunded);
            }

            total_refund_amount += milestone.amount;
        }

        let available_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available_balance < total_refund_amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        for idx in milestone_indices.iter() {
            let mut milestone = milestones.get(idx).unwrap();
            milestone.refunded = true;
            milestones.set(idx, milestone);
        }

        contract.refunded_amount += total_refund_amount;

        let all_refunded_or_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_refunded_or_released {
            let all_refunded = milestones.iter().all(|m| m.refunded);
            if all_refunded {
                contract.status = ContractStatus::Refunded;
            } else {
                contract.status = ContractStatus::Completed;
            }
        }

        ttl::store_milestones(&env, contract_id, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(&env, contract_id);

        total_refund_amount
    }
}
