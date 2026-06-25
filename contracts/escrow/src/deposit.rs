use crate::{
    ttl, Contract, ContractStatus, DataKey, Error, Escrow, EscrowArgs, EscrowClient,
    EscrowError, Milestone,
};
use soroban_sdk::{contractimpl, symbol_short, Address, Env, Symbol, Vec};

#[contractimpl]
impl Escrow {
    /// Deposits funds into the contract, supporting installment funding.
    ///
    /// Accepts deposits while the contract is in [`ContractStatus::Created`] or
    /// [`ContractStatus::PartiallyFunded`]. After each successful call the
    /// contract status is recomputed and transitions as follows:
    ///
    /// * `Created`              `->` `PartiallyFunded` if `0 < funded_amount < total_amount`
    /// * `Created`/`PartiallyFunded` `->` `Funded` once `funded_amount >= total_amount`
    ///
    /// # Errors (panics)
    ///
    /// * `ContractPaused` - if the contract is paused while not in emergency mode.
    /// * `EmergencyActive` - if the contract is in an active emergency pause.
    /// * `AmountMustBePositive` - if `amount <= 0`.
    /// * `ContractNotFound` - if `contract_id` was never allocated.
    /// * `InvalidState` - if the contract is in any status other than
    ///   `Created` or `PartiallyFunded` (including `Funded`, `Completed`,
    ///   `Refunded`, `Cancelled`, etc.).
    /// * `UnauthorizedRole` - if `caller` is not the stored client.
    /// * `InvalidDepositAmount` - if the deposit would push `funded_amount`
    ///   past `total_amount` (over-funding) or would overflow `i128`.
    ///
    /// # Arguments
    /// * `env`         - The contract environment
    /// * `contract_id` - The contract ID
    /// * `caller`      - The address calling the deposit (must be the stored client)
    /// * `amount`      - The stroop amount to deposit (must be `> 0`)
    ///
    /// # Returns
    /// `true` when the deposit is persisted and the status machine has been
    /// updated.
    ///
    /// # Events
    /// Emits `(deposited, contract_id)` with `(caller, amount, funded_amount,
    /// total_amount, status)` so off-chain indexers can distinguish partial
    /// installments from a final-funding deposit.
    ///
    /// # Security
    /// * TTL on the contract and milestones entries is bumped on both the
    ///   pre-read and post-write paths, so installment deposits do not expire
    ///   idle contracts.
    /// * All arithmetic is done via `checked_add` BEFORE the contract is
    ///   mutated; over-funding and overflow panic before any write.
    /// * The status gate runs before caller-auth so a non-authenticated
    ///   caller against a `Funded`/`Completed`/`Cancelled` contract does not
    ///   reveal the existence of a valid funded slot beyond the public read
    ///   path.
    pub fn deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool {
        // 0. Pause/emergency gate: refuses any deposit while the contract is
        //    paused or in an active emergency. Surfaced as `ContractPaused`
        //    when only the `Paused` flag is set, and `EmergencyActive` when
        //    the `Emergency` flag is set. Runs BEFORE any state read, auth,
        //    or balance change so funds cannot move while paused.
        Self::require_not_paused(&env);

        // 1. Positivity check first; preserves the original `AmountMustBePositive` surface.
        if amount <= 0 {
            env.panic_with_error(Error::AmountMustBePositive);
        }

        // 2. Load the contract; bump TTL on the read path.
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);

        // 3. Status gate: only `Created` and `PartiallyFunded` accept deposits.
        //    Checking status before auth avoids leaking caller-info via auth
        //    for contracts that are no longer accepting deposits.
        match contract.status {
            ContractStatus::Created | ContractStatus::PartiallyFunded => {}
            _ => env.panic_with_error(Error::InvalidState),
        }

        // 4. Authenticate caller. Caller must be the stored client.
        if caller != contract.client {
            env.panic_with_error(Error::UnauthorizedRole);
        }
        caller.require_auth();

        // 5. Resolve the milestone total BEFORE mutating state and BEFORE
        //    any overflow / over-funding check. Milestones are the source of
        //    truth for the funding cap.
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        ttl::extend_milestone_ttl(&env, contract_id);

        let total_amount: i128 = milestones.iter().map(|m| m.amount).sum();

        // 6. Compute the post-deposit funded amount with checked arithmetic.
        //    Reject over-funding and overflow before any write to the contract.
        let new_funded = contract
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InvalidDepositAmount));

        if new_funded > total_amount {
            env.panic_with_error(EscrowError::InvalidDepositAmount);
        }

        // 7. Apply the deposit and transition the status machine.
        contract.funded_amount = new_funded;
        contract.status = if new_funded >= total_amount {
            ContractStatus::Funded
        } else {
            ContractStatus::PartiallyFunded
        };

        // 8. Persist milestones + contract, then extend TTL.
        env.storage().persistent().set(
            &(DataKey::Contract(contract_id), milestone_key),
            &milestones,
        );
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(&env, contract_id);

        // 9. Emit a structured deposit event so indexers can distinguish
        //    partial installments from final-funding deposits.
        env.events().publish(
            (symbol_short!("deposited"), contract_id),
            (
                caller,
                amount,
                contract.funded_amount,
                total_amount,
                contract.status.clone(),
            ),
        );

        true
    }
}
