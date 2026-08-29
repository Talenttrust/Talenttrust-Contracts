use crate::{
    approvals, keys, ttl, milestone_transitions, Contract, ContractStatus, DataKey, Error, Escrow, Milestone,
    ReleaseAuthorization,
};
use soroban_sdk::{Address, Env, Symbol, Vec};

impl Escrow {
    /// Core logic for releasing a milestone, transferring funds to the freelancer.
    ///
    /// Called from the single `#[contractimpl]` block in lib.rs after the
    /// initialization, pause, and auth guards have been checked.
    ///
    /// This function routes the milestone status change through the centralized
    /// transition validator (`validate_milestone_transition`) to ensure consistent
    /// state-machine enforcement across all mutation paths (Issue #1340).
    pub(crate) fn release_milestone_impl(
        env: &Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        Self::require_not_paused(&env);
        caller.require_auth();

        Self::require_not_paused(&env);

        Self::require_not_finalized(&env, contract_id);

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);

        Self::require_not_paused(&env);
        Self::require_not_finalized(&env, contract_id);

        // Disputed contracts are release-locked until an arbiter resolution is
        // applied through the dispute path. This gate keeps payroll settlement
        // atomic with dispute handling and prevents funds moving during an open
        // dispute.
        if contract.status == ContractStatus::Disputed || contract.status != ContractStatus::Funded
        {
            env.panic_with_error(Error::InvalidState);
        }

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref() == Some(&caller);

        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if !is_client {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if !is_client && !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::MultiSig => {
                if !is_client && !is_freelancer {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
        }

        let milestone_key = keys::milestone_key(&env, contract_id);
        let mut milestones: Vec<Milestone> =
            env.storage().persistent().get(&milestone_key).unwrap();

        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap().clone();

        let milestone_released_key = DataKey::MilestoneReleased(contract_id, milestone_index);
        let is_already_released: bool = env
            .storage()
            .persistent()
            .get(&milestone_released_key)
            .unwrap_or(false);

        if milestone.released || is_already_released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }

        let current_state = milestone_transitions::MilestoneState::from_milestone(&milestone)
            .unwrap_or_else(|e| env.panic_with_error(e));
        let requested_state = milestone_transitions::MilestoneState::Released;

        milestone_transitions::validate_milestone_transition(current_state, requested_state)
            .unwrap_or_else(|e| env.panic_with_error(e));

        approvals::check_approvals(&env, &contract, contract_id, milestone_index)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let gross_amount = milestone.amount;
        let protocol_fee: i128 = if Self::is_initialized(&env) {
            let fee_bps = Self::read_protocol_fee_bps(&env);
            if fee_bps > 0 {
                Self::calculate_protocol_fee(&env, gross_amount, fee_bps)
            } else {
                0
            }
        } else {
            0
        };

        let net_amount = gross_amount - protocol_fee;
        let accumulated_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);

        let available_balance = contract
            .funded_amount
            .checked_sub(contract.released_amount)
            .and_then(|a| a.checked_sub(contract.refunded_amount))
            .and_then(|a| a.checked_sub(accumulated_fees))
            .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
        if available_balance < gross_amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        // Checks-Effects-Interactions: commit settled flag atomically before outward accounting
        env.storage()
            .persistent()
            .set(&milestone_released_key, &true);

        let _release_amount = milestone.amount;
        milestone.released = true;
        milestones.set(milestone_index, milestone.clone());
        contract.released_amount = contract
            .released_amount
            .checked_add(net_amount)
            .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

        // ── Atomic Version/Actor Persistence ──────────────────────────────────
        // Record who performed this transition and increment the version
        milestone_transitions::store_milestone_transition(env, contract_id, milestone_index, caller.clone());

        if Self::is_initialized(&env) {
            let fee_bps = Self::read_protocol_fee_bps(&env);
            if fee_bps > 0 {
                let fee = Self::calculate_protocol_fee(&env, milestone.amount, fee_bps);
                let current_accumulated: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AccumulatedProtocolFees)
                    .unwrap_or(0);
                let new_accumulated = current_accumulated
                    .checked_add(fee)
                    .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
                env.storage()
                    .persistent()
                    .set(&DataKey::AccumulatedProtocolFees, &new_accumulated);
            }
        }

        approvals::clear_approvals(&env, contract_id, milestone_index);

        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
            let pending_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
            let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
            let new_pending = pending
                .checked_add(1)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
            env.storage().persistent().set(&pending_key, &new_pending);
        }

        env.storage().persistent().set(&milestone_key, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_and_milestones_ttl(env, contract_id);

        env.events().publish(
            (Symbol::new(&env, "milestone_released"), contract_id),
            (caller, milestone_index, milestone.amount),
        );

        true
    }

    /// Core logic for releasing multiple milestones in an atomic batch.
    pub(crate) fn release_milestone_batch_impl(
        env: &Env,
        contract_id: u32,
        caller: Address,
        milestone_indices: Vec<u32>,
    ) -> bool {
        Self::require_not_paused(&env);
        caller.require_auth();

        if milestone_indices.is_empty() {
            env.panic_with_error(Error::EmptyBatch);
        }

        if milestone_indices.len() > crate::milestones_consts::MAX_BATCH_MILESTONES {
            env.panic_with_error(Error::BatchLimitExceeded);
        }

        Self::require_not_finalized(&env, contract_id);

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);

        if contract.status != ContractStatus::Funded {
            env.panic_with_error(Error::InvalidState);
        }

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref() == Some(&caller);

        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if !is_client {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if !is_client && !is_arbiter {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::MultiSig => {
                if !is_client && !is_freelancer {
                    env.panic_with_error(Error::UnauthorizedRole);
                }
            }
        }

        let milestone_key = keys::milestone_key(&env, contract_id);
        let mut milestones: Vec<Milestone> =
            env.storage().persistent().get(&milestone_key).unwrap();

        ttl::extend_milestone_ttl(&env, contract_id);

        let batch_len = milestone_indices.len();
        for i in 0..batch_len {
            let idx_i = milestone_indices.get(i).unwrap();
            for j in (i + 1)..batch_len {
                let idx_j = milestone_indices.get(j).unwrap();
                if idx_i == idx_j {
                    env.panic_with_error(Error::DuplicateMilestoneInBatch);
                }
            }
        }

        let mut total_amount: i128 = 0;
        for i in 0..batch_len {
            let milestone_index = milestone_indices.get(i).unwrap();
            if milestone_index >= milestones.len() {
                env.panic_with_error(Error::IndexOutOfBounds);
            }

            let milestone = milestones.get(milestone_index).unwrap();
            let milestone_released_key = DataKey::MilestoneReleased(contract_id, milestone_index);
            let is_already_released: bool = env
                .storage()
                .persistent()
                .get(&milestone_released_key)
                .unwrap_or(false);

            if milestone.released || is_already_released {
                env.panic_with_error(Error::MilestoneAlreadyReleased);
            }

            if milestone.refunded {
                env.panic_with_error(Error::AlreadyRefunded);
            }

            approvals::check_approvals(&env, &contract, contract_id, milestone_index)
                .unwrap_or_else(|e| env.panic_with_error(e));

            total_amount = total_amount
                .checked_add(milestone.amount)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
        }

        let available_balance = contract
            .funded_amount
            .checked_sub(contract.released_amount)
            .and_then(|a| a.checked_sub(contract.refunded_amount))
            .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

        if available_balance < total_amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        let fee_bps = if Self::is_initialized(&env) {
            Self::read_protocol_fee_bps(&env)
        } else {
            0
        };

        for i in 0..batch_len {
            let milestone_index = milestone_indices.get(i).unwrap();
            let mut milestone = milestones.get(milestone_index).unwrap().clone();

            let milestone_released_key = DataKey::MilestoneReleased(contract_id, milestone_index);
            env.storage()
                .persistent()
                .set(&milestone_released_key, &true);

            milestone.released = true;
            milestones.set(milestone_index, milestone.clone());

            contract.released_amount = contract
                .released_amount
                .checked_add(milestone.amount)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

            if fee_bps > 0 {
                let fee = Self::calculate_protocol_fee(&env, milestone.amount, fee_bps);
                let current_accumulated: i128 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::AccumulatedProtocolFees)
                    .unwrap_or(0);
                let new_accumulated = current_accumulated
                    .checked_add(fee)
                    .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
                env.storage()
                    .persistent()
                    .set(&DataKey::AccumulatedProtocolFees, &new_accumulated);
            }

            approvals::clear_approvals(&env, contract_id, milestone_index);

            env.events().publish(
                (Symbol::new(&env, "milestone_released"), contract_id),
                (caller.clone(), milestone_index, milestone.amount),
            );
        }

        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
            let pending_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
            let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
            let new_pending = pending
                .checked_add(1)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
            env.storage().persistent().set(&pending_key, &new_pending);
        }

        env.storage().persistent().set(&milestone_key, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_and_milestones_ttl(env, contract_id);

        true
    }
}
