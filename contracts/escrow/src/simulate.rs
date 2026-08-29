use crate::types::{
    ReleaseAuthorization, SimulateCreateContractOutcome, SimulatedDeposit, SimulatedRefund,
    SimulatedRelease,
};
use crate::{
    amount_validation, approvals, ttl, Contract, ContractStatus, DataKey, Error, Escrow,
    EscrowArgs, EscrowClient, EscrowError, Milestone, MAX_MILESTONES,
};
use soroban_sdk::{contractimpl, token, Address, Env, Symbol, Vec};

fn is_paused(env: &Env) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&DataKey::Paused)
        .unwrap_or(false)
        || env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Emergency)
            .unwrap_or(false)
}

#[contractimpl]
impl Escrow {
    /// Simulate releasing a milestone without mutating state or transferring tokens.
    ///
    /// Runs the same validation as `release_milestone` and returns the projected
    /// outcome. If validation fails, `would_succeed` is `false` and `error_code`
    /// contains the error code — the function never panics.
    pub fn simulate_release_milestone(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> SimulatedRelease {
        let err = |code| SimulatedRelease {
            would_succeed: false,
            error_code: Some(code),
            gross_amount: 0,
            net_amount: 0,
            protocol_fee: 0,
            projected_released_amount: 0,
            would_complete_contract: false,
        };

        if !Self::is_initialized(&env) {
            return err(Error::NotInitialized as u32);
        }
        if is_paused(&env) {
            return err(Error::ContractPaused as u32);
        }

        let contract: Contract = match env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
        {
            Some(c) => c,
            None => return err(EscrowError::ContractNotFound as u32),
        };

        if Self::is_finalized(&env, contract_id) {
            return err(Error::AlreadyFinalized as u32);
        }

        // Disputed contracts are not releasable; simulate the same fail-closed
        // behavior as the real release entrypoint and reject the action before
        // any amount projection is considered.
        if contract.status == ContractStatus::Disputed || contract.status != ContractStatus::Funded
        {
            return err(Error::InvalidState as u32);
        }

        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref() == Some(&caller);

        let authorized = match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => is_client,
            ReleaseAuthorization::ArbiterOnly => is_arbiter,
            ReleaseAuthorization::ClientAndArbiter => is_client || is_arbiter,
            ReleaseAuthorization::MultiSig => is_client || is_freelancer,
        };
        if !authorized {
            return err(EscrowError::UnauthorizedRole as u32);
        }

        let key = (
            DataKey::Contract(contract_id),
            Symbol::new(&env, "milestones"),
        );
        let milestones: Vec<Milestone> = match env.storage().persistent().get(&key) {
            Some(m) => m,
            None => return err(Error::ContractNotFound as u32),
        };

        if milestone_index >= milestones.len() {
            return err(Error::IndexOutOfBounds as u32);
        }

        let milestone = milestones.get(milestone_index).unwrap();

        if milestone.released {
            return err(Error::MilestoneAlreadyReleased as u32);
        }
        if milestone.refunded {
            return err(EscrowError::AlreadyRefunded as u32);
        }

        match approvals::check_approvals(&env, &contract, contract_id, milestone_index) {
            Ok(_) => {}
            Err(e) => return err(e as u32),
        }

        let gross_amount = milestone.amount;

        let protocol_fee: i128 = {
            let fee_bps = Self::read_protocol_fee_bps(&env);
            if fee_bps > 0 {
                Self::calculate_protocol_fee(&env, gross_amount, fee_bps)
            } else {
                0
            }
        };

        let net_amount = gross_amount - protocol_fee;

        let projected_released_amount = contract
            .released_amount
            .checked_add(net_amount)
            .unwrap_or(contract.released_amount);

        let would_complete_contract = milestones
            .iter()
            .enumerate()
            .all(|(i, m)| m.released || m.refunded || i as u32 == milestone_index);

        SimulatedRelease {
            would_succeed: true,
            error_code: None,
            gross_amount,
            net_amount,
            protocol_fee,
            projected_released_amount,
            would_complete_contract,
        }
    }

    /// Simulate depositing funds into an escrow contract without executing the
    /// SAC transfer or mutating state.
    ///
    /// Runs the same validation as `deposit_funds` and returns the projected
    /// outcome. Panics on validation failure (use `try_simulate_deposit_funds`
    /// to catch).
    pub fn simulate_deposit_funds(
        env: Env,
        contract_id: u32,
        caller: Address,
        amount: i128,
    ) -> SimulatedDeposit {
        Self::require_initialized(&env);
        Self::require_not_paused(&env);

        let token_addr = Self::read_settlement_token(&env)
            .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured));

        // Check token is valid by probing balance (same as real deposit)
        let _probe = token::Client::new(&env, &token_addr).balance(&env.current_contract_address());

        if amount <= 0 {
            env.panic_with_error(Error::AmountMustBePositive);
        }

        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        if caller != contract.client {
            env.panic_with_error(Error::UnauthorizedRole);
        }

        match contract.status {
            ContractStatus::Created | ContractStatus::PartiallyFunded => {}
            ContractStatus::Cancelled => env.panic_with_error(EscrowError::ContractCancelled),
            ContractStatus::Refunded => env.panic_with_error(EscrowError::InvalidState),
            _ => env.panic_with_error(Error::InvalidState),
        }

        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(
                DataKey::Contract(contract_id),
                Symbol::new(&env, "milestones"),
            ))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        let total_milestone_amount: i128 = milestones.iter().map(|m| m.amount).sum();

        let new_funded_amount = contract
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| env.panic_with_error(Error::AmountMustBePositive));

        if new_funded_amount > total_milestone_amount {
            env.panic_with_error(Error::AmountMustBePositive);
        }

        let projected_status = if new_funded_amount >= total_milestone_amount {
            ContractStatus::Funded
        } else {
            ContractStatus::PartiallyFunded
        };

        SimulatedDeposit {
            current_funded_amount: contract.funded_amount,
            new_funded_amount,
            projected_status,
            total_milestone_amount,
        }
    }

    /// Simulate creating a new escrow contract without persisting state or
    /// incrementing the contract ID counter.
    ///
    /// Runs the same validation as `create_contract`. Returns the projected
    /// outcome including the contract ID that would be assigned.
    /// Panics on validation failure.
    pub fn simulate_create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestones: Vec<i128>,
        release_authorization: ReleaseAuthorization,
    ) -> SimulateCreateContractOutcome {
        Self::require_not_paused(&env);

        if client == freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }

        match release_authorization {
            ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter
                if arbiter.is_none() =>
            {
                env.panic_with_error(EscrowError::MissingArbiter);
            }
            _ => {}
        }

        if let Some(ref arb) = arbiter {
            if arb == &client || arb == &freelancer {
                env.panic_with_error(EscrowError::InvalidArbiter);
            }
        }

        if milestones.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }

        if milestones.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        let max_total = env
            .storage()
            .persistent()
            .get::<_, crate::GovernedParameters>(&DataKey::GovernedParameters)
            .map(|params| params.max_escrow_total_stroops)
            .unwrap_or(i128::MAX);

        let mut native_milestones = [0_i128; MAX_MILESTONES as usize];
        let len = milestones.len() as usize;
        for i in 0..len {
            native_milestones[i] = milestones.get(i as u32).unwrap();
        }
        match amount_validation::validate_milestone_amounts(&native_milestones[..len], max_total) {
            Ok(_) => (),
            Err(err) => env.panic_with_error(err),
        }

        // Read next contract ID without incrementing
        ttl::extend_next_contract_id_ttl(&env);
        let contract_id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1);

        let total_amount: i128 = milestones.iter().sum();

        SimulateCreateContractOutcome {
            contract_id,
            client,
            freelancer,
            arbiter,
            release_authorization,
            milestones,
            total_amount,
        }
    }

    /// Simulate refunding unreleased milestones without transferring tokens or
    /// mutating state.
    ///
    /// Runs the same validation as `refund_unreleased_milestones` and returns the
    /// projected outcome. If validation fails, `would_succeed` is `false` and
    /// `error_code` contains the error code — the function never panics.
    pub fn simulate_refund(
        env: Env,
        contract_id: u32,
        milestone_indices: Vec<u32>,
    ) -> SimulatedRefund {
        let err = |code| SimulatedRefund {
            would_succeed: false,
            error_code: Some(code),
            total_refund_amount: 0,
            projected_status: ContractStatus::Created,
            projected_refunded_amount: 0,
            would_complete_contract: false,
        };

        if !Self::is_initialized(&env) {
            return err(Error::NotInitialized as u32);
        }
        if is_paused(&env) {
            return err(Error::ContractPaused as u32);
        }

        if milestone_indices.is_empty() {
            return err(EscrowError::EmptyRefundRequest as u32);
        }

        for i in 0..milestone_indices.len() {
            for j in (i + 1)..milestone_indices.len() {
                if milestone_indices.get(i).unwrap() == milestone_indices.get(j).unwrap() {
                    return err(EscrowError::DuplicateMilestoneInRefund as u32);
                }
            }
        }

        let contract: Contract = match env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
        {
            Some(c) => c,
            None => return err(EscrowError::ContractNotFound as u32),
        };

        if Self::is_finalized(&env, contract_id) {
            return err(Error::AlreadyFinalized as u32);
        }

        if contract.status != ContractStatus::Created
            && contract.status != ContractStatus::Funded
            && contract.status != ContractStatus::Disputed
        {
            return err(Error::InvalidState as u32);
        }

        let key = (
            DataKey::Contract(contract_id),
            Symbol::new(&env, "milestones"),
        );
        let milestones: Vec<Milestone> = match env.storage().persistent().get(&key) {
            Some(m) => m,
            None => return err(EscrowError::ContractNotFound as u32),
        };

        let mut total_refund_amount: i128 = 0;

        for idx in milestone_indices.iter() {
            if idx >= milestones.len() {
                return err(Error::IndexOutOfBounds as u32);
            }

            let milestone = milestones.get(idx).unwrap();

            if milestone.released {
                return err(Error::AlreadyRefunded as u32);
            }

            if milestone.refunded {
                return err(EscrowError::AlreadyRefunded as u32);
            }

            if let Some(_deadline) = milestone.deadline {
                if !Self::is_milestone_overdue(env.clone(), contract_id, idx) {
                    return err(Error::MilestoneNotOverdue as u32);
                }
            }

            total_refund_amount = total_refund_amount
                .checked_add(milestone.amount)
                .unwrap_or(0);
        }

        let available_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available_balance < total_refund_amount {
            return err(EscrowError::InsufficientFunds as u32);
        }

        let projected_refunded_amount = contract
            .refunded_amount
            .checked_add(total_refund_amount)
            .unwrap_or(contract.refunded_amount);

        // Determine projected status
        let all_refunded_or_released: bool = milestones.iter().enumerate().all(|(i, m)| {
            if m.released || m.refunded {
                return true;
            }
            let mut found = false;
            for ri in milestone_indices.iter() {
                if ri == i as u32 {
                    found = true;
                    break;
                }
            }
            found
        });

        let (projected_status, would_complete_contract) = if all_refunded_or_released {
            let all_refunded = milestones.iter().enumerate().all(|(i, m)| {
                if m.refunded {
                    return true;
                }
                let mut in_list = false;
                for ri in milestone_indices.iter() {
                    if ri == i as u32 {
                        in_list = true;
                        break;
                    }
                }
                in_list
            });
            if all_refunded {
                (ContractStatus::Refunded, true)
            } else {
                (ContractStatus::Completed, true)
            }
        } else {
            (contract.status, false)
        };

        SimulatedRefund {
            would_succeed: true,
            error_code: None,
            total_refund_amount,
            projected_status,
            projected_refunded_amount,
            would_complete_contract,
        }
    }
}
