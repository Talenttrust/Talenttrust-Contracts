use crate::{
    approvals, milestones_consts::{MAX_MILESTONES, MIN_WORK_EVIDENCE_BYTES, MAX_WORK_EVIDENCE_BYTES}, ttl, utils::now_seconds, Contract,
    ContractStatus, DataKey, Error, Escrow, EscrowError, Milestone, MilestoneApprovals, MilestoneSummary, ReleaseAuthorization,
};
use soroban_sdk::{contracttype, symbol_short, token, Address, Env, String, Symbol, Vec};

// ── Implementations ──────────────────────────────────────────────────────────

impl Escrow {
    /// Admin setter to update milestone parameters within strict upper/lower bounds.
    ///
    /// # Errors
    /// * `EscrowError::Unauthorized` - Caller is not the admin.
    /// * `EscrowError::InvalidParameter` - `max_milestones` is 0 or exceeds hard cap (`MAX_MILESTONES`).
    pub(crate) fn set_milestone_params_impl(
        env: &Env,
        admin: Address,
        max_milestones: u32,
    ) -> bool {
        Self::require_not_paused(env);
        admin.require_auth();

        // Verify admin authority
        let current_admin: Address = env.storage().persistent().get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::UnauthorizedRole));
        if admin != current_admin {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        // Validate bounds: non-zero and within MAX_MILESTONES cap
        if max_milestones == 0 || max_milestones > MAX_MILESTONES {
            env.panic_with_error(Error::InvalidProtocolParameters);
        }

        // Persist updated configuration
        env.storage()
            .persistent()
            .set(&DataKey::MaxMilestones, &max_milestones);

        // Emit parameter change event
        env.events().publish(
            (symbol_short!("mlst_cfg"), admin),
            (max_milestones, env.ledger().timestamp()),
        );

        true
    }

    pub(crate) fn release_milestone_impl(
        env: &Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        Self::require_not_paused(env);
        // Authenticate caller before any state-dependent logic
        caller.require_auth();

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Extend TTL on contract read
        ttl::extend_contract_ttl(env, contract_id);

        Self::require_not_finalized(env, contract_id);

        // Verify contract is in Funded state before release (deposit transitions
        // Created → Funded when fully funded, so release must accept Funded).
        if contract.status != ContractStatus::Funded {
            env.panic_with_error(Error::InvalidState);
        }

        // Check caller is authorized for this release authorization mode
        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref() == Some(&caller);

        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if !is_client {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if !is_client && !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::MultiSig => {
                if !is_client && !is_freelancer {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
        }

        let mut milestones: Vec<Milestone> = ttl::load_milestones(env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap().clone();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }

        if milestone.refunded {
            env.panic_with_error(EscrowError::AlreadyRefunded);
        }

        // Check for valid approvals
        approvals::check_approvals(env, &contract, contract_id, milestone_index)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let milestone_key = Symbol::new(env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        // Extend TTL on milestone read
        ttl::extend_milestone_ttl(env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap().clone();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }

        if milestone.refunded {
            env.panic_with_error(EscrowError::AlreadyRefunded);
        }

        // Check contract-level funding (per-milestone funded_amount is set after
        // release, so we check the aggregate contract balance here).
        let available =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available < milestone.amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        let gross_amount = milestone.amount;

        // Compute the protocol fee up-front so the available-balance check can
        // account for both the net payout and the fee that stays in the contract.
        let protocol_fee: i128 = if Self::is_initialized(env) {
            let fee_bps = Self::read_protocol_fee_bps(env);
            if fee_bps > 0 {
                Self::calculate_protocol_fee(env, gross_amount, fee_bps)
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
        let available_balance = contract.funded_amount
            - contract.released_amount
            - contract.refunded_amount
            - accumulated_fees;
        if available_balance < gross_amount {
            env.panic_with_error(EscrowError::InsufficientFunds);
        }

        let token = Self::read_settlement_token(env)
            .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured));
        let token_client = token::Client::new(env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &contract.freelancer,
            &net_amount,
        );

        if protocol_fee > 0 {
            env.storage().persistent().set(
                &DataKey::AccumulatedProtocolFees,
                &(accumulated_fees + protocol_fee),
            );
        }

        milestone.released = true;
        milestone.funded_amount = gross_amount;
        milestones.set(milestone_index, milestone.clone());

        contract.released_amount = contract
            .released_amount
            .checked_add(net_amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        let new_accumulated = accumulated_fees + protocol_fee;
        let invariant_sum = contract.released_amount + contract.refunded_amount + new_accumulated;
        if invariant_sum > contract.funded_amount {
            env.panic_with_error(EscrowError::AccountingInvariantViolated);
        }

        approvals::clear_approvals(env, contract_id, milestone_index);

        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
            Self::grant_pending_reputation_credit(env, &contract.freelancer);
        }

        ttl::store_milestones(env, contract_id, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(env, contract_id);

        env.events().publish(
            (symbol_short!("mlstn_rls"), contract_id),
            (
                milestone_index,
                gross_amount,
                protocol_fee,
                contract.released_amount,
                caller.clone(),
                env.ledger().timestamp(),
            ),
        );    
        if all_released {
            env.events().publish(
                (symbol_short!("ctrct_cmp"), contract_id),
                (caller, env.ledger().timestamp()),
            );
        }

        true
    }

    pub(crate) fn is_milestone_overdue_impl(env: &Env, contract_id: u32, milestone_index: u32) -> bool {
        let contract: Contract = match env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
        {
            Some(c) => c,
            None => return false,
        };

        let milestone_key = Symbol::new(env, "milestones");
        let milestones: Vec<Milestone> = match env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
        {
            Some(m) => m,
            None => return false,
        };

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let milestone = milestones.get(milestone_index).unwrap();

        if milestone.released {
            return false;
        }

        match milestone.deadline {
            None => false,
            Some(deadline) => now_seconds(env) > deadline,
        }
    }

    pub(crate) fn refund_unreleased_milestones_impl(
        env: &Env,
        contract_id: u32,
        milestone_indices: Vec<u32>,
    ) -> i128 {
        Self::require_not_paused(env);
        if milestone_indices.is_empty() {
            env.panic_with_error(EscrowError::EmptyRefundRequest);
        }

        for i in 0..milestone_indices.len() {
            for j in (i + 1)..milestone_indices.len() {
                if milestone_indices.get(i).unwrap() == milestone_indices.get(j).unwrap() {
                    env.panic_with_error(EscrowError::DuplicateMilestoneInRefund);
                }
            }
        }

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_contract_ttl(env, contract_id);

        Self::require_not_finalized(env, contract_id);

        if contract.status != ContractStatus::Created
            && contract.status != ContractStatus::Funded
            && contract.status != ContractStatus::Disputed
        {
            env.panic_with_error(EscrowError::InvalidState);
        }

        contract.client.require_auth();

        let mut milestones: Vec<Milestone> = ttl::load_milestones(env, contract_id);

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
                env.panic_with_error(EscrowError::AlreadyRefunded);
            }

            if let Some(deadline) = milestone.deadline {
                if !Self::is_milestone_overdue_impl(env, contract_id, idx) {
                    env.panic_with_error(Error::MilestoneNotOverdue);
                }
            }

            total_refund_amount += milestone.amount;
        }

        let available_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available_balance < total_refund_amount {
            env.panic_with_error(EscrowError::InsufficientFunds);
        }

        let token = Self::read_settlement_token(env)
            .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured));

        let token_client = token::Client::new(env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &contract.client,
            &total_refund_amount,
        );

        for idx in milestone_indices.iter() {
            let mut milestone = milestones.get(idx).unwrap();
            milestone.refunded = true;
            milestone.refunded_amount = milestone.amount;
            milestones.set(idx, milestone);
        }

        contract.refunded_amount = contract
            .refunded_amount
            .checked_add(total_refund_amount)
            .unwrap_or_else(|| env.panic_with_error(Error::InsufficientFunds));

        let all_refunded_or_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_refunded_or_released {
            let all_refunded = milestones.iter().all(|m| m.refunded);
            if all_refunded {
                contract.status = ContractStatus::Refunded;
            } else {
                contract.status = ContractStatus::Completed;
                Self::grant_pending_reputation_credit(env, &contract.freelancer);
            }
        }

        ttl::store_milestones(env, contract_id, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(env, contract_id);

        env.events().publish(
            (symbol_short!("refunded"), contract_id),
            (
                total_refund_amount,
                contract.status,
                env.ledger().timestamp(),
            ),
        );

        total_refund_amount
    }

    pub(crate) fn get_milestones_impl(env: &Env, contract_id: u32) -> Vec<Milestone> {
        let milestone_key = Symbol::new(env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_milestone_ttl(env, contract_id);
        milestones
    }

    pub(crate) fn get_milestone_impl(env: &Env, contract_id: u32, milestone_index: u32) -> Option<Milestone> {
        let milestone_key = Symbol::new(env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_milestone_ttl(env, contract_id);
        
        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        milestones.get(milestone_index)
    }

    pub(crate) fn get_milestone_approvals_impl(
        env: &Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<MilestoneApprovals> {
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), Symbol::new(env, "milestones")))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
        let approvals = env.storage().temporary().get(&approval_key);
        if approvals.is_some() {
            env.storage().temporary().extend_ttl(
                &approval_key,
                ttl::PENDING_APPROVAL_BUMP_THRESHOLD,
                ttl::PENDING_APPROVAL_TTL_LEDGERS,
            );
        }
        approvals
    }

    pub(crate) fn get_approval_deadline_impl(
        env: &Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<u32> {
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), Symbol::new(env, "milestones")))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
        if !env.storage().temporary().has(&approval_key) { return None; } Some(ttl::compute_expiry(env, ttl::PENDING_APPROVAL_TTL_LEDGERS))
    }

    pub(crate) fn submit_work_evidence_impl(
        env: &Env,
        contract_id: u32,
        milestone_index: u32,
        evidence: String,
    ) -> bool {
        Self::require_not_paused(env);
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        Self::require_not_finalized(env, contract_id);

        if contract.status != ContractStatus::Funded {
            env.panic_with_error(Error::InvalidState);
        }
        contract.freelancer.require_auth();

        let evidence_len = evidence.len();
        if evidence_len < MIN_WORK_EVIDENCE_BYTES {
            env.panic_with_error(Error::EmptyEvidence);
        }
        if evidence_len > MAX_WORK_EVIDENCE_BYTES {
            env.panic_with_error(Error::EvidenceTooLong);
        }

        let milestone_key = Symbol::new(env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap().clone();
        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }
        if milestone.refunded {
            env.panic_with_error(Error::AlreadyRefunded);
        }

        milestone.work_evidence = Some(evidence.clone());
        milestones.set(milestone_index, milestone);

        ttl::store_milestones(env, contract_id, &milestones);

        env.events().publish(
            (symbol_short!("evidence"), contract_id),
            (milestone_index, evidence, env.ledger().timestamp()),
        );

        true
    }

    pub(crate) fn get_work_evidence_impl(
        env: &Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<String> {
        let milestone_key = Symbol::new(env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_milestone_ttl(env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        milestones.get(milestone_index).unwrap().work_evidence
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_set_milestone_params_success() {
        let env = Env::default();
        let admin = Address::generate(&env);

        env.storage().instance().set(&DataKey::Admin, &admin);

        let new_limit = 8;
        let res = Escrow::set_milestone_params_impl(&env, admin, new_limit);
        assert!(res);

        let stored: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::MaxMilestones)
            .unwrap();
        assert_eq!(stored, new_limit);
    }

    #[test]
    #[should_panic]
    fn test_set_milestone_params_out_of_bounds_high() {
        let env = Env::default();
        let admin = Address::generate(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);

        Escrow::set_milestone_params_impl(&env, admin, 11);
    }

    #[test]
    #[should_panic]
    fn test_set_milestone_params_out_of_bounds_zero() {
        let env = Env::default();
        let admin = Address::generate(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);

        Escrow::set_milestone_params_impl(&env, admin, 0);
    }

    #[test]
    #[should_panic]
    fn test_set_milestone_params_unauthorized() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);

        Escrow::set_milestone_params_impl(&env, attacker, 5);
    }
}