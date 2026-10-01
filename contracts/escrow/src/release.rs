//! Milestone release execution for the escrow contract.
//!
//! This module owns the state transition that moves a milestone from `Pending`
//! to `Released` and the money/accounting side effects that must accompany it.
//! The public entrypoints in `lib.rs` apply the outer initialization, pause,
//! finalization, authentication, and re-entrancy guards; the executors here are
//! the shared mutation site that must keep the invariants below true for both
//! the single-milestone and the bounded-batch paths.
//!
//! # State invariants
//!
//! **R1 — Release is one-shot per milestone.** A milestone may transition
//! `Pending -> Released` exactly once. A release is rejected before any
//! mutation when the milestone's `released` flag is already set or when the
//! persisted `DataKey::MilestoneReleased(contract_id, index)` sentinel exists.
//! The sentinel is written *before* outward accounting (checks-effects-
//! interactions) so a re-entrant call cannot observe an intermediate state.
//! (tests: `released_milestone_cannot_be_released_twice`,
//! `batch_release_rejects_an_already_released_milestone`.)
//!
//! **R2 — Release requires the mode's approvals.** The caller must pass the
//! contract's `ReleaseAuthorization` role gate *and*
//! [`approvals::check_approvals`] must observe a sufficient, live approval
//! record: `ClientOnly` = client, `ArbiterOnly` = arbiter,
//! `ClientAndArbiter` = either, `MultiSig` = client and freelancer. A missing
//! or insufficient record fails closed. (tests:
//! `release_without_mode_approval_is_rejected`,
//! `multisig_release_requires_both_approvals`.)
//!
//! **R3 — Release moves exactly the milestone amount.** `released_amount`
//! increases by `gross - protocol_fee` for exactly the milestone(s) released;
//! `funded_amount` and `refunded_amount` are never touched and the protocol fee
//! is accumulated once. (tests:
//! `release_moves_exactly_the_milestone_amount_and_updates_accounting`,
//! `batch_release_moves_exact_sum_and_updates_accounting`.)
//!
//! **R4 — Rejected releases are total no-ops.** Every rejection above happens
//! before the first storage write (milestone flags, `MilestoneReleased`,
//! `released_amount`, approval clearing), so a failed release leaves the
//! milestone vector and all balances byte-identical. (test:
//! `rejected_release_leaves_state_unchanged`.)
//!
//! **R5 — Terminal contract states are immutable.** Release is only permitted
//! while the contract is `Funded`; `Disputed`, `Completed`, `Cancelled`, and
//! `Refunded` are release-locked. Settling the last outstanding milestone flips
//! the contract to `Completed` atomically with the milestone flag. (tests:
//! `disputed_contract_is_release_locked`,
//! `release_all_milestones_completes_contract_and_is_terminal`.)

use crate::{
    approvals, keys, milestone_transitions, ttl, Contract, ContractStatus, DataKey, Error, Escrow,
    Milestone, ReleaseAuthorization,
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
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&milestone_key)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones
            .get(milestone_index)
            .unwrap_or_else(|| env.panic_with_error(Error::IndexOutOfBounds))
            .clone();

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
        milestone_transitions::store_milestone_transition(
            env,
            contract_id,
            milestone_index,
            caller.clone(),
        );

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
            // Route through the single accrual policy owned by `lib.rs` so the
            // credit ledger cannot diverge between release paths.
            Self::grant_pending_reputation_credit(env, &contract.freelancer);
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
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&milestone_key)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_milestone_ttl(&env, contract_id);

        let batch_len = milestone_indices.len();
        for i in 0..batch_len {
            let idx_i = milestone_indices
                .get(i)
                .unwrap_or_else(|| env.panic_with_error(Error::IndexOutOfBounds));
            for j in (i + 1)..batch_len {
                let idx_j = milestone_indices
                    .get(j)
                    .unwrap_or_else(|| env.panic_with_error(Error::IndexOutOfBounds));
                if idx_i == idx_j {
                    env.panic_with_error(Error::DuplicateMilestoneInBatch);
                }
            }
        }

        let mut total_gross_amount: i128 = 0;
        for i in 0..batch_len {
            let milestone_index = milestone_indices
                .get(i)
                .unwrap_or_else(|| env.panic_with_error(Error::IndexOutOfBounds));
            if milestone_index >= milestones.len() {
                env.panic_with_error(Error::IndexOutOfBounds);
            }

            let milestone = milestones
                .get(milestone_index)
                .unwrap_or_else(|| env.panic_with_error(Error::IndexOutOfBounds));
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

            total_gross_amount = total_gross_amount
                .checked_add(milestone.amount)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
        }

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

        if available_balance < total_gross_amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        let fee_bps = if Self::is_initialized(&env) {
            Self::read_protocol_fee_bps(&env)
        } else {
            0
        };

        // Calculate total protocol fees for the entire batch upfront
        let mut total_protocol_fees: i128 = 0;
        if fee_bps > 0 {
            for i in 0..batch_len {
                let milestone_index = milestone_indices
                    .get(i)
                    .unwrap_or_else(|| env.panic_with_error(Error::IndexOutOfBounds));
                let milestone = milestones
                    .get(milestone_index)
                    .unwrap_or_else(|| env.panic_with_error(Error::IndexOutOfBounds));
                let fee = Self::calculate_protocol_fee(&env, milestone.amount, fee_bps);
                total_protocol_fees = total_protocol_fees
                    .checked_add(fee)
                    .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
            }
        }

        // Pass 2: Atomic State Updates (Checks-Effects-Interactions)
        // All state changes happen before any token transfers
        for i in 0..batch_len {
            let milestone_index = milestone_indices
                .get(i)
                .unwrap_or_else(|| env.panic_with_error(Error::IndexOutOfBounds));
            let mut milestone = milestones
                .get(milestone_index)
                .unwrap_or_else(|| env.panic_with_error(Error::IndexOutOfBounds))
                .clone();

            let milestone_released_key = DataKey::MilestoneReleased(contract_id, milestone_index);
            env.storage()
                .persistent()
                .set(&milestone_released_key, &true);

            milestone.released = true;
            milestones.set(milestone_index, milestone.clone());

            let gross_amount = milestone.amount;
            let protocol_fee: i128 = if fee_bps > 0 {
                Self::calculate_protocol_fee(&env, gross_amount, fee_bps)
            } else {
                0
            };

            let net_amount = gross_amount - protocol_fee;

            contract.released_amount = contract
                .released_amount
                .checked_add(net_amount)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));

            approvals::clear_approvals(&env, contract_id, milestone_index);

            env.events().publish(
                (Symbol::new(&env, "milestone_released"), contract_id),
                (caller.clone(), milestone_index, milestone.amount),
            );
        }

        // Atomically accumulate total protocol fees after all milestone updates
        if total_protocol_fees > 0 {
            let new_accumulated = accumulated_fees
                .checked_add(total_protocol_fees)
                .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
            env.storage()
                .persistent()
                .set(&DataKey::AccumulatedProtocolFees, &new_accumulated);
        }

        // Final accounting invariant check
        let final_accumulated_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);
        if let Err(e) = crate::amount_validation::validate_accounting_invariant(
            contract.funded_amount,
            contract.released_amount,
            contract.refunded_amount,
            final_accumulated_fees,
        ) {
            env.panic_with_error(e);
        }

        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        if all_released {
            contract.status = ContractStatus::Completed;
            // Route through the single accrual policy owned by `lib.rs` so the
            // credit ledger cannot diverge between release paths.
            Self::grant_pending_reputation_credit(env, &contract.freelancer);
        }

        env.storage().persistent().set(&milestone_key, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_and_milestones_ttl(env, contract_id);

        true
    }
}

// ── State-invariant tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{assert_contract_error, EscrowFixture};
    use crate::EscrowError;

    /// Invoke the release module's single-milestone executor exactly as a wired
    /// `lib.rs` entrypoint would, from within the escrow's contract context.
    fn release_impl(fixture: &EscrowFixture, caller: &Address, index: u32) -> bool {
        fixture.env.as_contract(&fixture.escrow_address, || {
            Escrow::release_milestone_impl(&fixture.env, fixture.escrow_id, caller.clone(), index)
        })
    }

    /// Invoke the release module's bounded-batch executor directly.
    fn batch_release_impl(
        fixture: &EscrowFixture,
        caller: &Address,
        indices: soroban_sdk::Vec<u32>,
    ) -> bool {
        fixture.env.as_contract(&fixture.escrow_address, || {
            Escrow::release_milestone_batch_impl(
                &fixture.env,
                fixture.escrow_id,
                caller.clone(),
                indices,
            )
        })
    }

    /// A fully funded, ClientOnly, three-milestone fixture: the baseline for
    /// every release invariant below.
    fn funded_client_only() -> EscrowFixture {
        EscrowFixture::builder().funded().build()
    }

    // R3 — release moves exactly the milestone amount and updates accounting.
    #[test]
    fn release_moves_exactly_the_milestone_amount_and_updates_accounting() {
        let fixture = funded_client_only();
        let escrow = fixture.escrow();
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));

        let before = escrow.get_contract(&fixture.escrow_id);
        assert!(release_impl(&fixture, &fixture.client, 0));
        let after = escrow.get_contract(&fixture.escrow_id);

        let released = escrow.get_milestones(&fixture.escrow_id).get(0).unwrap();
        assert!(released.released, "R1: the released flag must be set");
        assert_eq!(
            after.released_amount - before.released_amount,
            released.amount,
            "R3: released_amount must increase by exactly the milestone amount"
        );
        assert_eq!(
            after.funded_amount, before.funded_amount,
            "R3: funded_amount must not change on release"
        );
        assert_eq!(
            after.refunded_amount, before.refunded_amount,
            "R3: refunded_amount must not change on release"
        );
        assert_eq!(
            after.status,
            ContractStatus::Funded,
            "R5: the contract stays Funded while milestones remain unsettled"
        );
    }

    // R3 — a batch moves exactly the sum of the milestones it settles.
    #[test]
    fn batch_release_moves_exact_sum_and_updates_accounting() {
        let fixture = funded_client_only();
        let escrow = fixture.escrow();
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &1));

        let milestones = escrow.get_milestones(&fixture.escrow_id);
        let expected = milestones.get(0).unwrap().amount + milestones.get(1).unwrap().amount;

        let before = escrow.get_contract(&fixture.escrow_id);
        assert!(batch_release_impl(
            &fixture,
            &fixture.client,
            soroban_sdk::vec![&fixture.env, 0u32, 1u32],
        ));
        let after = escrow.get_contract(&fixture.escrow_id);

        assert_eq!(
            after.released_amount - before.released_amount,
            expected,
            "R3: a batch must move exactly the sum of the released milestones"
        );
        let settled = escrow.get_milestones(&fixture.escrow_id);
        assert!(settled.get(0).unwrap().released);
        assert!(settled.get(1).unwrap().released);
        assert!(!settled.get(2).unwrap().released);
    }

    // R1 — a released milestone cannot be released twice.
    #[test]
    #[should_panic]
    fn released_milestone_cannot_be_released_twice() {
        let fixture = funded_client_only();
        let escrow = fixture.escrow();
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
        assert!(release_impl(&fixture, &fixture.client, 0));

        // The second attempt must be rejected before any accounting is touched.
        release_impl(&fixture, &fixture.client, 0);
    }

    // R1 — a batch containing an already-settled milestone is rejected whole.
    #[test]
    #[should_panic]
    fn batch_release_rejects_an_already_released_milestone() {
        let fixture = funded_client_only();
        let escrow = fixture.escrow();
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
        assert!(release_impl(&fixture, &fixture.client, 0));

        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &1));
        batch_release_impl(
            &fixture,
            &fixture.client,
            soroban_sdk::vec![&fixture.env, 1u32, 0u32],
        );
    }

    // R2 — a release without the mode's approval fails closed.
    #[test]
    #[should_panic]
    fn release_without_mode_approval_is_rejected() {
        let fixture = funded_client_only();
        release_impl(&fixture, &fixture.client, 0);
    }

    // R2 — MultiSig needs both the client and the freelancer approval.
    #[test]
    #[should_panic]
    fn multisig_release_requires_both_approvals() {
        let fixture = EscrowFixture::builder()
            .funded()
            .release_authorization(ReleaseAuthorization::MultiSig)
            .build();
        let escrow = fixture.escrow();
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));
        // Only the client has approved; the freelancer approval is still absent.
        release_impl(&fixture, &fixture.client, 0);
    }

    // R5 — a disputed contract is release-locked.
    #[test]
    #[should_panic]
    fn disputed_contract_is_release_locked() {
        let fixture = funded_client_only();
        let escrow = fixture.escrow();
        assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &0));

        fixture.env.as_contract(&fixture.escrow_address, || {
            let key = DataKey::Contract(fixture.escrow_id);
            let mut contract: Contract = fixture.env.storage().persistent().get(&key).unwrap();
            contract.status = ContractStatus::Disputed;
            fixture.env.storage().persistent().set(&key, &contract);
        });

        release_impl(&fixture, &fixture.client, 0);
    }

    // R5 — settling every milestone completes the contract, which is terminal.
    #[test]
    fn release_all_milestones_completes_contract_and_is_terminal() {
        let fixture = funded_client_only();
        let escrow = fixture.escrow();
        for index in 0..3u32 {
            assert!(escrow.approve_milestone_release(&fixture.escrow_id, &fixture.client, &index));
            assert!(release_impl(&fixture, &fixture.client, index));
        }

        let contract = escrow.get_contract(&fixture.escrow_id);
        assert_eq!(contract.status, ContractStatus::Completed);
        assert_eq!(contract.released_amount, fixture.total_amount());

        // Once Completed, the release entrypoint refuses to mutate further.
        assert_contract_error(
            escrow.try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
            EscrowError::InvalidState,
        );
    }

    // R4 — a rejected release leaves milestones and balances unchanged.
    #[test]
    fn rejected_release_leaves_state_unchanged() {
        let fixture = funded_client_only();
        let escrow = fixture.escrow();

        let contract_before = escrow.get_contract(&fixture.escrow_id);
        let milestones_before = escrow.get_milestones(&fixture.escrow_id);

        assert_contract_error(
            escrow.try_release_milestone(&fixture.escrow_id, &fixture.client, &0),
            EscrowError::InsufficientApprovals,
        );

        assert_eq!(
            escrow.get_contract(&fixture.escrow_id),
            contract_before,
            "R4: a rejected release must not change contract accounting"
        );
        assert_eq!(
            escrow.get_milestones(&fixture.escrow_id),
            milestones_before,
            "R4: a rejected release must not change any milestone"
        );
    }
}
