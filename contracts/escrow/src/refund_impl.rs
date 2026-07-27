//! Per-milestone refund implementation for the TalentTrust escrow contract.
//!
//! This module provides the `refund_unreleased_milestones` functionality that allows
//! clients to refund specific unreleased milestones back to their account.
//!
//! # Security Guarantees
//!
//! - **Authorization**: Only the client can initiate refunds (enforced via `require_auth()`)
//! - **Atomicity**: All validations occur before any state changes
//! - **Idempotency**: Refunded milestones cannot be refunded again
//! - **Balance Protection**: Verifies sufficient balance before processing
//! - **State Machine Integrity**: Respects contract lifecycle, cannot refund released milestones
//!
//! # Validation Guards
//!
//! - `EmptyRefundRequest`: Rejects empty milestone index vectors
//! - `DuplicateMilestoneInRefund`: Prevents duplicate indices in a single request
//! - `AlreadyReleased`: Cannot refund milestones that were already released
//! - `AlreadyRefunded`: Cannot refund the same milestone twice
//! - `InsufficientFunds`: Ensures contract has enough balance to process refund
//!
//! # Accounting Invariant
//!
//! The implementation maintains:
//! ```text
//! funded_amount = released_amount + refunded_amount + available_balance
//! ```
//!
//! # Status Transitions
//!
//! - **Funded → Refunded**: All unreleased milestones refunded (no releases)
//! - **Funded → Funded**: Partial refund (some milestones remain unreleased/unrefunded)
//! - **Funded → Completed**: All milestones either released or refunded (mixed state)

use crate::{Contract, ContractStatus, DataKey, EscrowError, Milestone};
use soroban_sdk::{Env, Symbol, Vec};

/// Refunds unreleased milestones back to the client.
///
/// # Arguments
///
/// * `env` - The contract environment
/// * `contract_id` - The unique identifier of the contract
/// * `milestone_indices` - Vector of milestone indices to refund (0-indexed)
///
/// # Returns
///
/// The total amount refunded (sum of all refunded milestone amounts)
///
/// # Errors
///
/// * `ContractNotFound` - Contract with given ID doesn't exist
/// * `EmptyRefundRequest` - milestone_indices vector is empty
/// * `DuplicateMilestoneInRefund` - Same milestone appears multiple times
/// * `InvalidMilestone` - Milestone index out of bounds
/// * `AlreadyReleased` - Attempting to refund a released milestone
/// * `AlreadyRefunded` - Attempting to refund an already-refunded milestone
/// * `InsufficientFunds` - Contract doesn't have enough balance
///
/// # Example
///
/// ```ignore
/// // Refund milestones 1 and 2 (keeping milestone 0)
/// let refund_ids = vec![&env, 1_u32, 2_u32];
/// let refunded_amount = client.refund_unreleased_milestones(&contract_id, &refund_ids);
/// ```
pub fn refund_unreleased_milestones(
    env: &Env,
    contract_id: u32,
    milestone_indices: &Vec<u32>,
) -> i128 {
    // Guard: Reject empty refund requests
    if milestone_indices.is_empty() {
        env.panic_with_error(EscrowError::EmptyRefundRequest);
    }

    // Guard: Check for duplicate milestone indices
    check_no_duplicates(env, milestone_indices);

    // Load contract state
    let mut contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

    // Authorization: Only client can refund
    contract.client.require_auth();

    // Terminal-state guards: once a contract is Cancelled or Refunded, no further
    // refund or value-moving operations are permitted.
    if contract.status == ContractStatus::Cancelled {
        env.panic_with_error(EscrowError::ContractCancelled);
    }
    if contract.status == ContractStatus::Refunded {
        env.panic_with_error(EscrowError::ContractRefunded);
    }

    // Load milestones
    let milestone_key = Symbol::new(env, "milestones");
    let mut milestones: Vec<Milestone> = env
        .storage()
        .persistent()
        .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
        .unwrap();

    // Validate all milestones and calculate total refund amount
    let total_refund_amount = validate_and_calculate_refund(env, &milestones, milestone_indices);

    // Guard: Check sufficient balance
    check_sufficient_balance(env, &contract, total_refund_amount);

    // Retrieve settlement token and perform transfer
    let token_address: soroban_sdk::Address = env.storage().persistent().get(&DataKey::SettlementToken).unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
    let balance = soroban_sdk::token::Client::new(env, &token_address).balance(&env.current_contract_address());
    if balance < total_refund_amount {
        env.panic_with_error(EscrowError::InsufficientEscrowBalance);
    }
    soroban_sdk::token::Client::new(env, &token_address).transfer(&env.current_contract_address(), &contract.client, &total_refund_amount);

    // Mark milestones as refunded
    mark_milestones_refunded(&mut milestones, milestone_indices);

    // Update contract state
    contract.refunded_amount += total_refund_amount;
    update_contract_status(&mut contract, &milestones);

    // Persist changes
    env.storage()
        .persistent()
        .set(&(DataKey::Contract(contract_id), milestone_key), &milestones);
    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), &contract);

    total_refund_amount
}

/// Checks for duplicate milestone indices in the refund request.
fn check_no_duplicates(env: &Env, milestone_indices: &Vec<u32>) {
    for i in 0..milestone_indices.len() {
        for j in (i + 1)..milestone_indices.len() {
            if milestone_indices.get(i).unwrap() == milestone_indices.get(j).unwrap() {
                env.panic_with_error(EscrowError::DuplicateMilestoneInRefund);
            }
        }
    }
}

/// Validates all milestones in the refund request and calculates total refund amount.
///
/// # Validation Rules
///
/// - Milestone index must be within bounds
/// - Milestone must not be already released
/// - Milestone must not be already refunded
fn validate_and_calculate_refund(
    env: &Env,
    milestones: &Vec<Milestone>,
    milestone_indices: &Vec<u32>,
) -> i128 {
    let mut total_refund_amount: i128 = 0;

    for idx in milestone_indices.iter() {
        // Guard: Check milestone exists
        if idx >= milestones.len() {
            env.panic_with_error(EscrowError::InvalidMilestone);
        }

        let milestone = milestones.get(idx).unwrap();

        // Guard: Cannot refund released milestones
        if milestone.released {
            env.panic_with_error(EscrowError::AlreadyReleased);
        }

        // Guard: Cannot refund already-refunded milestones
        if milestone.refunded {
            env.panic_with_error(EscrowError::AlreadyRefunded);
        }

        total_refund_amount += milestone.amount;
    }

    total_refund_amount
}

/// Checks if the contract has sufficient balance to process the refund.
fn check_sufficient_balance(env: &Env, contract: &Contract, refund_amount: i128) {
    let available_balance =
        contract.funded_amount - contract.released_amount - contract.refunded_amount;

    if available_balance < refund_amount {
        env.panic_with_error(EscrowError::InsufficientFunds);
    }
}

/// Marks the specified milestones as refunded.
fn mark_milestones_refunded(milestones: &mut Vec<Milestone>, milestone_indices: &Vec<u32>) {
    for idx in milestone_indices.iter() {
        let mut milestone = milestones.get(idx).unwrap();
        milestone.refunded = true;
        milestones.set(idx, milestone);
    }
}

/// Updates the contract status based on milestone states.
///
/// # Status Transition Logic
///
/// - If all milestones are refunded → `Refunded`
/// - If all milestones are either released or refunded → `Completed`
/// - Otherwise → remains `Funded`
fn update_contract_status(contract: &mut Contract, milestones: &Vec<Milestone>) {
    let all_refunded_or_released = milestones.iter().all(|m| m.released || m.refunded);

    if all_refunded_or_released {
        let all_refunded = milestones.iter().all(|m| m.refunded);
        if all_refunded {
            contract.status = ContractStatus::Refunded;
        } else {
            // Mixed state: some released, some refunded
            contract.status = ContractStatus::Completed;
        }
    }
    // Otherwise, status remains Funded
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, vec, Address, Env};

    #[test]
    fn test_check_no_duplicates_passes_for_unique_indices() {
        let env = Env::default();
        let indices = vec![&env, 0_u32, 1_u32, 2_u32];
        check_no_duplicates(&env, &indices);
        // Should not panic
    }

    #[test]
    #[should_panic(expected = "DuplicateMilestoneInRefund")]
    fn test_check_no_duplicates_fails_for_duplicate_indices() {
        let env = Env::default();
        let indices = vec![&env, 0_u32, 1_u32, 1_u32];
        check_no_duplicates(&env, &indices);
    }
}
