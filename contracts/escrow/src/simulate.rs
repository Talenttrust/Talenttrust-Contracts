use crate::types::{
    ReleaseAuthorization, SimulateCreateContractOutcome, SimulatedDeposit, SimulatedRefund,
    SimulatedRelease,
};
use crate::utils::now_seconds;
use crate::{
    amount_validation, approvals, refund, storage, Contract, ContractStatus, DataKey, Error,
    Escrow, EscrowArgs, EscrowClient, EscrowError, Milestone, MAX_MILESTONES,
};
use soroban_sdk::{contractimpl, token, Address, Env, Vec};

// # Failure model
//
// The dry runs come in two shapes, and each keeps its shape for *every* failure,
// including damaged storage:
//
// * `simulate_release_milestone` and `simulate_refund` never panic. Every
//   rejection — validation, arithmetic, or a malformed persisted entry — is
//   returned as `would_succeed: false` with a stable `error_code`.
// * `simulate_deposit_funds` and `simulate_create_contract` panic with a typed
//   contract error, exactly like the entrypoints they mirror; callers use the
//   generated `try_*` client methods.
//
// Storage is read through the typed helpers in `crate::storage`, so a malformed
// entry is reported as `StorageInvariantViolated` instead of aborting with an
// untyped host conversion error. No dry run writes storage, transfers tokens,
// or emits events, so a failed or repeated simulation cannot change state.
// Results depend only on ledger state and arguments: the same inputs always
// produce the same outcome.

/// Pause and initialization preconditions shared by the non-panicking dry runs.
///
/// Mirrors the order used by the mutating entrypoints: initialization first,
/// then the legacy pause flag, then the emergency flag. A malformed flag fails
/// closed with `StorageInvariantViolated`.
fn check_preconditions(env: &Env) -> Result<(), Error> {
    match storage::read_persistent::<_, bool>(env, &DataKey::Initialized).or_default(false)? {
        true => {}
        false => return Err(Error::NotInitialized),
    }
    storage::check_not_paused(env)
}

/// Read the accumulated protocol fees without trapping on a malformed entry.
fn read_accumulated_fees(env: &Env) -> Result<i128, Error> {
    storage::read_persistent::<_, i128>(env, &DataKey::AccumulatedProtocolFees).or_default(0)
}

#[contractimpl]
impl Escrow {
    /// Simulate releasing a milestone without mutating state or transferring tokens.
    ///
    /// Runs the same validation as `release_milestone` and returns the projected
    /// outcome. If validation fails, `would_succeed` is `false` and `error_code`
    /// contains the error code — the function never panics.
    ///
    /// # State-invariant protections
    ///
    /// 1. **Duplicate-released milestone**: a milestone that is both `released`
    ///    and `refunded` is an inconsistent state; the check for `released`
    ///    takes priority and returns `MilestoneAlreadyReleased` before the
    ///    `refunded` check so the invariant ordering mirrors `release_milestone`.
    ///
    /// 2. **`projected_released_amount` overflow**: the original code silently
    ///    capped overflow at the old `released_amount` via `unwrap_or`. We now
    ///    return `PotentialOverflow` so callers cannot observe a silently wrong
    ///    projection.
    ///
    /// 3. **`available_balance` underflow**: the balance subtraction chain uses
    ///    `checked_sub` throughout; any underflow is returned as
    ///    `PotentialOverflow` rather than wrapping silently.
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

        if let Err(error) = check_preconditions(&env) {
            return err(error as u32);
        }

        let contract: Contract = match storage::try_load_contract(&env, contract_id) {
            Ok(c) => c,
            Err(error) => return err(error as u32),
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

        let milestones: Vec<Milestone> = match storage::try_load_milestones(&env, contract_id) {
            Ok(m) => m,
            Err(error) => return err(error as u32),
        };

        if milestone_index >= milestones.len() {
            return err(Error::IndexOutOfBounds as u32);
        }

        let milestone = milestones.get(milestone_index).unwrap();

        // Invariant: check `released` before `refunded` to mirror the ordering
        // in `release_milestone`. A milestone that somehow ends up with both
        // flags set is an inconsistent state — surface it as AlreadyReleased.
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

        let net_amount = match gross_amount.checked_sub(protocol_fee) {
            Some(v) => v,
            None => return err(EscrowError::PotentialOverflow as u32),
        };

        // Read for its failure mode only: a malformed fee accumulator makes the
        // real release fail when it credits the fee, so the dry run must too.
        if let Err(error) = read_accumulated_fees(&env) {
            return err(error as u32);
        }

        // Invariant: use checked arithmetic throughout the balance calculation
        // so that any overflow/underflow is surfaced as PotentialOverflow rather
        // than wrapping silently.
        let available_balance = match contract
            .funded_amount
            .checked_sub(contract.released_amount)
            .and_then(|balance| balance.checked_sub(contract.refunded_amount))
        {
            Some(balance) => balance,
            None => return err(EscrowError::PotentialOverflow as u32),
        };

        if available_balance < gross_amount {
            return err(EscrowError::InsufficientFunds as u32);
        }

        // Invariant: `projected_released_amount` overflow must be returned as
        // an error rather than silently clamped via `unwrap_or`. A silently
        // capped projection would mislead callers into believing a release is
        // safe when it would produce an arithmetic fault at execution time.
        let projected_released_amount = match contract.released_amount.checked_add(net_amount) {
            Some(v) => v,
            None => return err(EscrowError::PotentialOverflow as u32),
        };

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
    ///
    /// # State-invariant protections
    ///
    /// 1. **`new_funded_amount` overflow**: the original code panicked with a
    ///    misleading `AmountMustBePositive` on overflow. We now explicitly panic
    ///    with `PotentialOverflow` so the error code accurately describes the
    ///    arithmetic fault.
    ///
    /// 2. **`total_milestone_amount` overflow**: summing milestone amounts with a
    ///    plain `.sum()` can overflow silently. We use a `checked_add` fold so
    ///    overflow panics with `PotentialOverflow`.
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

        let contract: Contract = storage::load_contract(&env, contract_id);

        if caller != contract.client {
            env.panic_with_error(Error::UnauthorizedRole);
        }

        match contract.status {
            ContractStatus::Created | ContractStatus::PartiallyFunded => {}
            ContractStatus::Cancelled => env.panic_with_error(EscrowError::ContractCancelled),
            ContractStatus::Refunded => env.panic_with_error(EscrowError::InvalidState),
            _ => env.panic_with_error(Error::InvalidState),
        }

        let milestones: Vec<Milestone> = storage::load_milestones(&env, contract_id);

        // Invariant: use checked_add fold so milestone-amount sum overflow is
        // surfaced as PotentialOverflow rather than wrapping silently.
        let total_milestone_amount: i128 = milestones
            .iter()
            .try_fold(0_i128, |acc, m| acc.checked_add(m.amount))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        // Invariant: panic with PotentialOverflow on funded_amount + amount
        // overflow so the error code accurately describes the arithmetic fault.
        let new_funded_amount = contract
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

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
    ///
    /// # State-invariant protections
    ///
    /// 1. **`total_amount` overflow**: the original code computed the sum with a
    ///    plain `.sum()` which can overflow silently on i128. We use a
    ///    `checked_add` fold so overflow panics with `PotentialOverflow` instead
    ///    of wrapping and producing a wrong projected total.
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

        // A malformed governed-parameters entry must not silently lift the cap
        // to `i128::MAX`: fail closed instead.
        let max_total = match storage::read_persistent::<_, crate::GovernedParameters>(
            &env,
            &DataKey::GovernedParameters,
        ) {
            storage::StorageRead::Present(params) => params.max_escrow_total_stroops,
            storage::StorageRead::Missing => i128::MAX,
            storage::StorageRead::Corrupt => env.panic_with_error(Error::StorageInvariantViolated),
        };

        let mut native_milestones = [0_i128; MAX_MILESTONES as usize];
        let len = milestones.len() as usize;
        for i in 0..len {
            native_milestones[i] = milestones.get(i as u32).unwrap();
        }
        let total_amount = match amount_validation::validate_milestone_amounts(
            &native_milestones[..len],
            max_total,
        ) {
            Ok(total) => total,
            Err(err) => env.panic_with_error(err),
        };

        // Read the next contract ID without incrementing it. The dry run must
        // not write, so the TTL is not extended here either (`create_contract`
        // extends it when it actually allocates the ID).
        let contract_id: u32 = storage::read_persistent::<_, u32>(&env, &DataKey::NextContractId)
            .or_default(1)
            .unwrap_or_else(|error| env.panic_with_error(error));

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
    ///
    /// # State-invariant protections
    ///
    /// 1. **`total_refund_amount` overflow**: the original code used
    ///    `checked_add(...).unwrap_or(0)` which silently reset the accumulator
    ///    to zero on overflow — a crafted set of milestone amounts could exploit
    ///    this to make a refund appear to cost nothing. We now return
    ///    `PotentialOverflow` so the simulation correctly rejects arithmetic faults.
    ///
    /// 2. **`available_balance` underflow**: the original code used unchecked
    ///    subtraction (`funded_amount - released_amount - refunded_amount`).
    ///    Underflow wraps to a large positive value, bypassing the
    ///    `InsufficientFunds` guard. We use `checked_sub` so underflow returns
    ///    `PotentialOverflow`.
    ///
    /// 3. **`projected_refunded_amount` overflow**: the original code silently
    ///    clamped overflow via `unwrap_or(contract.refunded_amount)`. We now
    ///    return `PotentialOverflow` so callers cannot observe a silently wrong
    ///    projection.
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

        if let Err(error) = check_preconditions(&env) {
            return err(error as u32);
        }

        // V1: the request-shape boundary defined in `crate::refund`, shared with
        // `refund_unreleased_milestones` so both paths report the same rejection.
        if let Err(error) = refund::validate_request(&milestone_indices) {
            return err(error as u32);
        }

        let contract: Contract = match storage::try_load_contract(&env, contract_id) {
            Ok(c) => c,
            Err(error) => return err(error as u32),
        };

        if Self::is_finalized(&env, contract_id) {
            return err(Error::AlreadyFinalized as u32);
        }

        // V2: the lifecycle boundary defined in `crate::refund`.
        if let Err(error) = refund::validate_status(&contract) {
            return err(error as u32);
        }

        let milestones: Vec<Milestone> = match storage::try_load_milestones(&env, contract_id) {
            Ok(m) => m,
            Err(error) => return err(error as u32),
        };

        // V3: the same milestone and total boundaries the mutating entrypoint
        // enforces — index bounds, released (`MilestoneAlreadyReleased`) before
        // refunded (`AlreadyRefunded`), deadline, and a checked total. This is
        // the *only* pass over the requested indices: each amount is counted
        // exactly once, so the projected total equals what the real refund
        // would move.
        let total_refund_amount =
            match refund::validate_milestones(&milestones, &milestone_indices, now_seconds(&env)) {
                Ok(total) => total,
                Err(error) => return err(error as u32),
            };

        // V4/V5: the shared balance boundary. Checked subtraction reports
        // broken accounting as `PotentialOverflow`, and an over-committed or
        // short balance as `InsufficientFunds`, exactly as the real refund does.
        if let Err(error) = refund::ensure_available_balance(&contract, total_refund_amount) {
            return err(error as u32);
        }

        // Invariant: return PotentialOverflow on projected_refunded_amount
        // overflow rather than silently clamping to the old value.
        let projected_refunded_amount =
            match contract.refunded_amount.checked_add(total_refund_amount) {
                Some(v) => v,
                None => return err(EscrowError::PotentialOverflow as u32),
            };

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
