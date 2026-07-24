//! Temporary milestone approval storage and release authorization checks.
//!
//! This module owns the temporary
//! `DataKey::MilestoneApprovals(contract_id, milestone_index)` records used by
//! `approve_milestone_release` and `release_milestone`. It reads the escrow
//! contract and milestone vector to validate state and role authorization, but
//! it does not move funds or mutate milestone accounting.
//!
//! Approval records live in Soroban temporary storage and expire according to
//! `PENDING_APPROVAL_TTL_LEDGERS`. Missing or expired approvals fail closed.

use crate::ttl::{PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS};
use crate::types::{
    Contract, ContractStatus, DataKey, Error, Milestone, MilestoneApprovals, ReleaseAuthorization,
};
use soroban_sdk::{Address, Env, Vec};

/// Approves a milestone for release by the caller.
///
/// Records the approval in temporary storage with TTL expiry.
/// The approval will automatically expire after PENDING_APPROVAL_TTL_LEDGERS.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `milestone_index` - The index of the milestone to approve
/// * `caller` - The address of the caller. In MultiSig mode, exactly the
///   client and freelancer can approve, and both approvals are required.
///
/// # Returns
/// `true` if approval was recorded successfully
///
/// # Errors
/// * `ContractNotFound` - If contract doesn't exist
/// * `InvalidState` - If contract is not in Funded state
/// * `IndexOutOfBounds` - If milestone index is invalid
/// * `MilestoneAlreadyReleased` - If milestone was already released
/// * `UnauthorizedRole` - If caller is not authorized to approve
/// * `AlreadyApproved` - If caller has already approved this milestone
///
/// # Security
/// - Caller must be authenticated via require_auth()
/// - Only parties authorized by the contract's release mode can approve
/// - Approvals are stored with TTL and auto-expire
/// - Duplicate approvals from the same party are rejected
pub fn approve_milestone(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    caller: &Address,
) -> Result<bool, Error> {
    // Load contract
    let contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .ok_or(Error::ContractNotFound)?;

    // Verify contract is in Funded or PartiallyFunded state
    if contract.status != ContractStatus::Funded
        && contract.status != ContractStatus::PartiallyFunded
    {
        return Err(Error::InvalidState);
    }

    // Load milestones
    let milestones: Vec<Milestone> = env
        .storage()
        .persistent()
        .get(&crate::ttl::milestone_storage_key(env, contract_id))
        .ok_or(Error::ContractNotFound)?;

    // Validate milestone index
    if milestone_index >= milestones.len() {
        return Err(Error::IndexOutOfBounds);
    }

    let milestone = milestones.get(milestone_index).unwrap();

    // Check if milestone is already released
    if milestone.released {
        return Err(Error::MilestoneAlreadyReleased);
    }

    // Determine caller role and check authorization
    let is_client = caller == &contract.client;
    let is_freelancer = caller == &contract.freelancer;
    let is_arbiter = contract.arbiter.as_ref() == Some(caller);

    // Verify caller is a valid participant
    if !is_client && !is_freelancer && !is_arbiter {
        return Err(Error::UnauthorizedRole);
    }

    // Check authorization based on release mode
    match contract.release_authorization {
        ReleaseAuthorization::ClientOnly => {
            if !is_client {
                return Err(Error::UnauthorizedRole);
            }
        }
        ReleaseAuthorization::ArbiterOnly => {
            if !is_arbiter {
                return Err(Error::UnauthorizedRole);
            }
        }
        ReleaseAuthorization::ClientAndArbiter => {
            if !is_client && !is_arbiter {
                return Err(Error::UnauthorizedRole);
            }
        }
        ReleaseAuthorization::MultiSig => {
            if !is_client && !is_freelancer {
                return Err(Error::UnauthorizedRole);
            }
        }
    }

    // Load or create approval record
    let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
    let mut approvals: MilestoneApprovals =
        env.storage()
            .temporary()
            .get(&approval_key)
            .unwrap_or(MilestoneApprovals {
                client_approved: false,
                freelancer_approved: false,
                arbiter_approved: false,
            });

    // Check for duplicate approval and update
    if is_client {
        if approvals.client_approved {
            return Err(Error::AlreadyApproved);
        }
        approvals.client_approved = true;
    } else if is_freelancer {
        if approvals.freelancer_approved {
            return Err(Error::AlreadyApproved);
        }
        approvals.freelancer_approved = true;
    } else if is_arbiter {
        if approvals.arbiter_approved {
            return Err(Error::AlreadyApproved);
        }
        approvals.arbiter_approved = true;
    }

    // Store approval with TTL
    env.storage().temporary().set(&approval_key, &approvals);

    env.storage().temporary().extend_ttl(
        &approval_key,
        PENDING_APPROVAL_BUMP_THRESHOLD,
        PENDING_APPROVAL_TTL_LEDGERS,
    );

    env.events().publish(
        (soroban_sdk::symbol_short!("ms_appr"), contract_id),
        (milestone_index, milestone.amount),
    );

    Ok(true)
}

/// Checks if a milestone has sufficient approvals for release.
///
/// Expired approvals (TTL elapsed) are treated as absent and return None.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract` - The contract data
/// * `contract_id` - The contract ID
/// * `milestone_index` - The milestone index
///
/// # Returns
/// * `Ok(true)` - If sufficient approvals exist and are valid
/// * `Err(InsufficientApprovals)` - If approvals are missing or insufficient
/// * `Err(ApprovalExpired)` - If approvals existed but have expired
///
/// # Security
/// - Fail-closed: missing or expired approvals prevent release
/// - MultiSig requires both client and freelancer approvals
/// - TTL expiry is enforced by Soroban's temporary storage
pub fn check_approvals(
    env: &Env,
    contract: &Contract,
    contract_id: u32,
    milestone_index: u32,
) -> Result<bool, Error> {
    let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);

    // Try to load approvals from temporary storage
    // If TTL has expired, this will return None
    let approvals: Option<MilestoneApprovals> = env.storage().temporary().get(&approval_key);

    // If no approvals exist (or they expired), fail
    let approvals = approvals.ok_or(Error::InsufficientApprovals)?;

    // Check if required approvals are present based on authorization mode
    let sufficient = match contract.release_authorization {
        ReleaseAuthorization::ClientOnly => approvals.client_approved,
        ReleaseAuthorization::ArbiterOnly => approvals.arbiter_approved,
        ReleaseAuthorization::ClientAndArbiter => {
            approvals.client_approved || approvals.arbiter_approved
        }
        ReleaseAuthorization::MultiSig => {
            approvals.client_approved && approvals.freelancer_approved
        }
    };

    if sufficient {
        Ok(true)
    } else {
        Err(Error::InsufficientApprovals)
    }
}

/// Clears approval records for a milestone after successful release.
///
/// This prevents approval reuse and cleans up temporary storage.
///
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `milestone_index` - The milestone index
pub fn clear_approvals(env: &Env, contract_id: u32, milestone_index: u32) {
    let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
    env.storage().temporary().remove(&approval_key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Escrow;
    use soroban_sdk::{testutils::Address as _, Env, Symbol, Vec};

    fn setup_contract_in_storage(
        env: &Env,
        escrow_id: &crate::Address,
        contract_id: u32,
        contract: &Contract,
        release_auth: ReleaseAuthorization,
    ) {
        env.as_contract(escrow_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), contract);
            let milestones = Vec::from_array(
                env,
                [Milestone {
                    amount: 1000,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let _ = release_auth;
            let milestone_key = Symbol::new(env, "milestones");
            env.storage().persistent().set(
                &(DataKey::Contract(contract_id), milestone_key),
                &milestones,
            );
        });
    }

    #[test]
    fn test_approve_milestone_client_only() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_address = env.register(crate::Escrow, ());

        let escrow_id = env.register(Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };

        let contract_id = 1u32;
        env.as_contract(&contract_address, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), &contract);

            let milestones = Vec::from_array(
                &env,
                [Milestone {
                    amount: 1000,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let milestone_key = Symbol::new(&env, "milestones");
            env.storage().persistent().set(
                &(DataKey::Contract(contract_id), milestone_key),
                &milestones,
            );

            // Client approves
            let result = approve_milestone(&env, contract_id, 0, &client);
            assert!(result.is_ok());

            // Check approvals
            let check = check_approvals(&env, &contract, contract_id, 0);
            assert!(check.is_ok());
        });
    }

    #[test]
    fn test_approve_milestone_multisig() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_address = env.register(crate::Escrow, ());

        let escrow_id = env.register(Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::MultiSig,
            reputation_issued: false,
        };

        let contract_id = 1u32;
        env.as_contract(&contract_address, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), &contract);

            let milestones = Vec::from_array(
                &env,
                [Milestone {
                    amount: 1000,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let milestone_key = Symbol::new(&env, "milestones");
            env.storage().persistent().set(
                &(DataKey::Contract(contract_id), milestone_key),
                &milestones,
            );

            // Only client approves - insufficient
            let result = approve_milestone(&env, contract_id, 0, &client);
            assert!(result.is_ok());

            let check = check_approvals(&env, &contract, contract_id, 0);
            assert_eq!(check, Err(Error::InsufficientApprovals));

            // Freelancer also approves - now sufficient
            let result = approve_milestone(&env, contract_id, 0, &freelancer);
            assert!(result.is_ok());

            let check = check_approvals(&env, &contract, contract_id, 0);
            assert!(check.is_ok());
        });
    }

    #[test]
    #[ignore]
    fn test_duplicate_approval_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_address = env.register(crate::Escrow, ());

        let escrow_id = env.register(Escrow, ());
        let client = crate::Address::generate(&env);
        let freelancer = crate::Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            total_deposited: 1000,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
            reputation_issued: false,
        };

        let contract_id = 1u32;
        env.as_contract(&contract_address, || {
            env.storage()
                .persistent()
                .set(&DataKey::Contract(contract_id), &contract);

            let milestones = Vec::from_array(
                &env,
                [Milestone {
                    amount: 1000,
                    funded_amount: 0,
                    released: false,
                    refunded: false,
                    work_evidence: None,
                    refunded_amount: 0,
                    deadline: None,
                }],
            );
            let milestone_key = Symbol::new(&env, "milestones");
            env.storage().persistent().set(
                &(DataKey::Contract(contract_id), milestone_key),
                &milestones,
            );

            // First approval succeeds
            let result = approve_milestone(&env, contract_id, 0, &client);
            assert!(result.is_ok());

            // Second approval fails
            let result = approve_milestone(&env, contract_id, 0, &client);
            assert_eq!(result, Err(Error::AlreadyApproved));
        });
    }
}
