use crate::ttl::{PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS};
use crate::types::{Contract, ContractStatus, DataKey, Error, MilestoneApprovals, Milestone, ReleaseAuthorization};
use soroban_sdk::{Address, Env, Symbol, Vec};

/// Approves a milestone for release by the caller.
/// 
/// Records the approval in temporary storage with TTL expiry.
/// The approval will automatically expire after PENDING_APPROVAL_TTL_LEDGERS.
/// 
/// # Arguments
/// * `env` - The contract environment
/// * `contract_id` - The contract ID
/// * `milestone_index` - The index of the milestone to approve
/// * `caller` - The address of the caller (must be client, freelancer, or arbiter)
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
/// - Only authorized parties (client/freelancer/arbiter) can approve
/// - Approvals are stored with TTL and auto-expire
/// - Duplicate approvals from the same party are rejected
pub fn approve_milestone(
    env: &Env,
    contract_id: u32,
    milestone_index: u32,
    caller: &Address,
) -> Result<bool, Error> {
    // Authenticate caller
    caller.require_auth();

    // Load contract
    let contract: Contract = env
        .storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .ok_or(Error::ContractNotFound)?;

    // Verify contract is in Funded state
    if contract.status != ContractStatus::Funded {
        return Err(Error::InvalidState);
    }

    // Load milestones
    let milestone_key = Symbol::new(env, "milestones");
    let milestones: Vec<Milestone> = env
        .storage()
        .persistent()
        .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
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
    let is_arbiter = contract.arbiter.as_ref().map_or(false, |a| caller == a);

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
    let mut approvals: MilestoneApprovals = env
        .storage()
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
    env.storage()
        .temporary()
        .set(&approval_key, &approvals);
    
    env.storage()
        .temporary()
        .extend_ttl(&approval_key, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS);

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
    let approvals: Option<MilestoneApprovals> = env
        .storage()
        .temporary()
        .get(&approval_key);

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
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_approve_milestone_client_only() {
        let env = Env::default();
        env.mock_all_auths();

        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
        };

        let contract_id = 1u32;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        let milestones = Vec::from_array(
            &env,
            [Milestone {
                amount: 1000,
                released: false,
                refunded: false,
                work_evidence: None,
            }],
        );
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(contract_id), milestone_key), &milestones);

        // Client approves
        let result = approve_milestone(&env, contract_id, 0, &client);
        assert!(result.is_ok());

        // Check approvals
        let check = check_approvals(&env, &contract, contract_id, 0);
        assert!(check.is_ok());
    }

    #[test]
    fn test_approve_milestone_multisig() {
        let env = Env::default();
        env.mock_all_auths();

        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::MultiSig,
        };

        let contract_id = 1u32;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        let milestones = Vec::from_array(
            &env,
            [Milestone {
                amount: 1000,
                released: false,
                refunded: false,
                work_evidence: None,
            }],
        );
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(contract_id), milestone_key), &milestones);

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
    }

    #[test]
    fn test_duplicate_approval_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let client = Address::generate(&env);
        let freelancer = Address::generate(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: None,
            status: ContractStatus::Funded,
            funded_amount: 1000,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization: ReleaseAuthorization::ClientOnly,
        };

        let contract_id = 1u32;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        let milestones = Vec::from_array(
            &env,
            [Milestone {
                amount: 1000,
                released: false,
                refunded: false,
                work_evidence: None,
            }],
        );
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(contract_id), milestone_key), &milestones);

        // First approval succeeds
        let result = approve_milestone(&env, contract_id, 0, &client);
        assert!(result.is_ok());

        // Second approval fails
        let result = approve_milestone(&env, contract_id, 0, &client);
        assert_eq!(result, Err(Error::AlreadyApproved));
    }
}
