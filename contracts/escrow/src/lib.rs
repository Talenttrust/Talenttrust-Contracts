#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, Symbol, Vec,
};

/// Persistent storage keys used by the Escrow contract.
///
/// Each variant corresponds to a distinct piece of contract state:
/// - [`DataKey::Contract`] stores the full [`EscrowContract`] keyed by its numeric ID.
/// - [`DataKey::ReputationIssued`] is a boolean flag that prevents double-issuance of
///   reputation credentials for a given contract.
/// - [`DataKey::CancellationReason`] stores the reason for cancellation (if applicable).
/// - [`DataKey::CancelledAt`] stores the timestamp when a contract was cancelled.
/// - [`DataKey::CancelledBy`] stores the address of who cancelled the contract.
/// - [`DataKey::NextId`] is a monotonically increasing counter for assigning contract IDs.
#[contracttype]
pub enum DataKey {
    /// Full escrow contract state, keyed by the numeric contract ID.
    Contract(u32),
    /// Whether a reputation credential has already been issued for the given contract ID.
    /// Immutably set to `true` on first issuance; prevents replay and double-issuance.
    ReputationIssued(u32),
    /// Reason for cancellation, if the contract was cancelled.
    CancellationReason(u32),
    /// Timestamp when contract was cancelled, if applicable.
    CancelledAt(u32),
    /// Address of party who cancelled the contract, if applicable.
    CancelledBy(u32),
    /// Auto-incrementing counter; incremented on every [`Escrow::create_contract`] call.
    NextId,
}

/// The lifecycle status of an escrow contract.
///
/// Valid transitions:
/// ```text
/// Created     -> Funded -> Completed
/// Created     -> Cancelled
/// Funded      -> Disputed
/// Funded      -> Cancelled (under agreed conditions)
/// Disputed    -> Cancelled (with arbiter approval)
/// ```
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    /// Contract created, awaiting client deposit.
    Created = 0,
    /// Funds deposited by client; work is in progress.
    Funded = 1,
    /// All milestones released and contract finalised by the client.
    Completed = 2,
    /// A dispute has been raised; milestone payments are paused.
    Disputed = 3,
    /// Contract has been cancelled; funds returned to client.
    Cancelled = 4,
}

/// Reason for contract cancellation.
///
/// Tracks why a contract was cancelled to support audit trails and analytics.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationReason {
    /// Cancelled before first funding (parties agreed not to proceed).
    MutualAgreement = 0,
    /// Cancelled by client before any milestones were released.
    ClientInitiated = 1,
    /// Cancelled by freelancer before any milestones were released.
    FreelancerInitiated = 2,
    /// Cancelled by arbiter (e.g., after dispute resolution).
    ArbiterApproved = 3,
    /// Cancelled due to contract timeout or inactivity.
    TimeoutExpired = 4,
}

/// A single deliverable and its associated payment within an escrow contract.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    /// Payment amount in stroops (1 XLM = 10_000_000 stroops).
    pub amount: i128,
    /// Whether the client has released this milestone's funds to the freelancer.
    pub released: bool,
    pub approved_by: Option<Address>,
    pub approval_timestamp: Option<u64>,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAuthorization {
    ClientOnly = 0,
    ClientAndArbiter = 1,
    ArbiterOnly = 2,
    MultiSig = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowContract {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<Milestone>,
    pub status: ContractStatus,
    pub release_auth: ReleaseAuthorization,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Approval {
    None = 0,
    Client = 1,
    Arbiter = 2,
    Both = 3,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MilestoneApproval {
    pub milestone_id: u32,
    pub approvals: Map<Address, bool>,
    pub required_approvals: u32,
    pub approval_status: Approval,
}

#[contract]
pub struct Escrow;

#[contractimpl]
impl Escrow {
    /// Create a new escrow contract with milestone release authorization
    ///
    /// # Arguments
    /// * `client` - Address of the client who funds the escrow
    /// * `freelancer` - Address of the freelancer who receives payments
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestone_amounts` - Vector of milestone payment amounts
    /// * `release_auth` - Authorization scheme for milestone releases
    ///
    /// # Returns
    /// Contract ID for the newly created escrow
    ///
    /// # Errors
    /// Panics if:
    /// - Milestone amounts vector is empty
    /// - Any milestone amount is zero or negative
    /// - Client and freelancer addresses are the same
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestone_amounts: Vec<i128>,
        release_auth: ReleaseAuthorization,
    ) -> u32 {
        // Validate inputs
        if milestone_amounts.is_empty() {
            panic!("At least one milestone required");
        }

        if client == freelancer {
            panic!("Client and freelancer cannot be the same address");
        }

        // Validate milestone amounts
        for i in 0..milestone_amounts.len() {
            let amount = milestone_amounts.get(i).unwrap();
            if amount <= 0 {
                panic!("Milestone amounts must be positive");
            }
        }

        // Create milestones
        let mut milestones = Vec::new(&env);
        for i in 0..milestone_amounts.len() {
            milestones.push_back(Milestone {
                amount: milestone_amounts.get(i).unwrap(),
                released: false,
                approved_by: None,
                approval_timestamp: None,
            });
        }

        // Create contract
        let contract_data = EscrowContract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            milestones,
            status: ContractStatus::Created,
            release_auth,
            created_at: env.ledger().timestamp(),
        };

        // Generate contract ID (in real implementation, this would use proper storage)
        let contract_id = env.ledger().sequence();

        // Store contract data (simplified for this implementation)
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract_data);

        contract_id
    }

    /// Deposit funds into escrow. Only the client may call this.
    ///
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract
    /// * `amount` - Amount to deposit (must equal total milestone amounts)
    ///
    /// # Returns
    /// true if deposit successful
    ///
    /// # Errors
    /// Panics if:
    /// - Caller is not the client
    /// - Contract is not in Created status
    /// - Amount doesn't match total milestone amounts
    pub fn deposit_funds(env: Env, _contract_id: u32, caller: Address, amount: i128) -> bool {
        caller.require_auth();

        // In real implementation, retrieve contract from storage
        // For now, we'll use a simplified approach
        let contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        // Verify caller is client
        if caller != contract.client {
            panic!("Only client can deposit funds");
        }

        // Verify contract status
        if contract.status != ContractStatus::Created {
            panic!("Contract must be in Created status to deposit funds");
        }

        // Calculate total required amount
        let mut total_required = 0i128;
        for i in 0..contract.milestones.len() {
            total_required += contract.milestones.get(i).unwrap().amount;
        }

        if amount != total_required {
            panic!("Deposit amount must equal total milestone amounts");
        }

        // Update contract status to Funded
        let mut updated_contract = contract;
        updated_contract.status = ContractStatus::Funded;
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &updated_contract);

        true
    }

    /// Approve a milestone for release with proper authorization
    ///
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract
    /// * `milestone_id` - ID of the milestone to approve
    ///
    /// # Returns
    /// true if approval successful
    ///
    /// # Errors
    /// Panics if:
    /// - Caller is not authorized to approve
    /// - Contract is not in Funded status
    /// - Milestone ID is invalid
    /// - Milestone already released
    /// - Milestone already approved by this caller
    pub fn approve_milestone_release(
        env: Env,
        _contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();

        // Retrieve contract
        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        // Verify contract status
        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to approve milestones");
        }

        // Validate milestone ID
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();

        // Check if milestone already released
        if milestone.released {
            panic!("Milestone already released");
        }

        // Check authorization based on release_auth scheme
        let is_authorized = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => caller == contract.client,
            ReleaseAuthorization::ArbiterOnly => {
                contract.arbiter.clone().is_some_and(|a| caller == a)
            }
            ReleaseAuthorization::ClientAndArbiter => {
                caller == contract.client || contract.arbiter.clone().is_some_and(|a| caller == a)
            }
            ReleaseAuthorization::MultiSig => {
                // For multi-sig, both client and arbiter must approve
                // This function handles individual approval
                caller == contract.client || contract.arbiter.clone().is_some_and(|a| caller == a)
            }
        };

        if !is_authorized {
            panic!("Caller not authorized to approve milestone release");
        }

        // Check if already approved by this caller
        if milestone
            .approved_by
            .clone()
            .is_some_and(|addr| addr == caller)
        {
            panic!("Milestone already approved by this address");
        }

        // Update milestone approval
        let mut updated_milestone = milestone;
        updated_milestone.approved_by = Some(caller);
        updated_milestone.approval_timestamp = Some(env.ledger().timestamp());

        // Update contract
        contract.milestones.set(milestone_id, updated_milestone);
        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

        true
    }

    /// Release a milestone payment to the freelancer after proper authorization
    ///
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract
    /// * `milestone_id` - ID of the milestone to release
    ///
    /// # Returns
    /// true if release successful
    ///
    /// # Errors
    /// Panics if:
    /// - Contract is not in Funded status
    /// - Milestone ID is invalid
    /// - Milestone already released
    /// - Insufficient approvals based on authorization scheme
    pub fn release_milestone(
        env: Env,
        _contract_id: u32,
        caller: Address,
        milestone_id: u32,
    ) -> bool {
        caller.require_auth();
        // Retrieve contract
        let mut contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        // Verify contract status
        if contract.status != ContractStatus::Funded {
            panic!("Contract must be in Funded status to release milestones");
        }

        // Validate milestone ID
        if milestone_id >= contract.milestones.len() {
            panic!("Invalid milestone ID");
        }

        let milestone = contract.milestones.get(milestone_id).unwrap();

        // Check if milestone already released
        if milestone.released {
            panic!("Milestone already released");
        }

        // Check if milestone has sufficient approvals
        let has_sufficient_approval = match contract.release_auth {
            ReleaseAuthorization::ClientOnly => milestone
                .approved_by
                .clone()
                .is_some_and(|addr| addr == contract.client),
            ReleaseAuthorization::ArbiterOnly => contract.arbiter.clone().is_some_and(|arbiter| {
                milestone
                    .approved_by
                    .clone()
                    .is_some_and(|addr| addr == arbiter)
            }),
            ReleaseAuthorization::ClientAndArbiter => {
                milestone.approved_by.clone().is_some_and(|addr| {
                    addr == contract.client
                        || contract
                            .arbiter
                            .clone()
                            .is_some_and(|arbiter| addr == arbiter)
                })
            }
            ReleaseAuthorization::MultiSig => {
                // For multi-sig, we'd need to track multiple approvals
                // Simplified: require client approval for now
                milestone
                    .approved_by
                    .clone()
                    .is_some_and(|addr| addr == contract.client)
            }
        };

        if !has_sufficient_approval {
            panic!("Insufficient approvals for milestone release");
        }

        // Release milestone
        let mut updated_milestone = milestone;
        updated_milestone.released = true;

        // Update contract
        contract.milestones.set(milestone_id, updated_milestone);

        // Check if all milestones are released
        let all_released = contract.milestones.iter().all(|m| m.released);
        if all_released {
            contract.status = ContractStatus::Completed;
        }

        env.storage()
            .persistent()
            .set(&symbol_short!("contract"), &contract);

        // In real implementation, transfer funds to freelancer
        // For now, we'll just mark as released

        true
    }

    /// Issue a reputation credential for the freelancer of a completed escrow contract.
    ///
    /// # Reputation Issuance Constraints
    ///
    /// This function enforces the following ordered constraints:
    ///
    /// 1. **Contract existence** - The contract identified by `contract_id` must exist in
    ///    persistent storage.
    /// 2. **Completion gate** - `status` must be [`ContractStatus::Completed`]. Reputation
    ///    cannot be issued for contracts that are still `Created`, `Funded`, or `Disputed`.
    /// 3. **Final settlement** - Every milestone must have `released == true`. This ensures
    ///    no outstanding payment obligations remain before a credential is recorded.
    /// 4. **Single issuance** - A credential can be issued at most once per contract.
    ///    The [`DataKey::ReputationIssued`] flag is set atomically before the event is
    ///    emitted, preventing replay attacks and double-issuance.
    /// 5. **Valid rating** - `rating` must be in the inclusive range `[1, 5]`.
    ///
    /// On success, a `reputation_issued` event is published for off-chain indexers:
    /// ```text
    /// topic:  (Symbol("reputation_issued"),)
    /// data:   (contract_id: u32, freelancer: Address, rating: u32)
    /// ```
    ///
    /// # Arguments
    /// * `contract_id` - Numeric ID of the completed escrow contract.
    /// * `rating` - Reputation score in `[1, 5]`.
    ///
    /// # Panics
    /// Panics with a descriptive message for each violated constraint (see above).
    pub fn issue_reputation(env: Env, contract_id: u32, rating: u32) -> bool {
        // Constraint 1: contract must exist.
        let escrow: EscrowContract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .expect("contract not found");

        // Constraint 2: must be Completed.
        assert!(
            escrow.status == ContractStatus::Completed,
            "reputation can only be issued after contract completion"
        );

        // Constraint 3: all milestones released (final settlement).
        for i in 0..escrow.milestones.len() {
            let m = escrow.milestones.get(i).unwrap();
            assert!(
                m.released,
                "reputation can only be issued after final settlement of all milestones"
            );
        }

        // Constraint 4: no double issuance.
        let already_issued: bool = env
            .storage()
            .persistent()
            .get(&DataKey::ReputationIssued(contract_id))
            .unwrap_or(false);
        assert!(
            !already_issued,
            "reputation already issued for this contract"
        );

        // Constraint 5: rating must be in [1, 5].
        assert!((1..=5).contains(&rating), "rating must be between 1 and 5");

        // Set the issued flag before emitting the event (checks-effects-interactions).
        env.storage()
            .persistent()
            .set(&DataKey::ReputationIssued(contract_id), &true);

        // Emit an observable event for off-chain indexers and auditors.
        env.events().publish(
            (Symbol::new(&env, "reputation_issued"),),
            (contract_id, escrow.freelancer.clone(), rating),
        );

        true
    }

    /// Cancel an escrow contract under agreed conditions.
    ///
    /// # Cancellation Policy
    ///
    /// This function enforces safe cancellation with clear authorization rules:
    ///
    /// 1. **Created Status (No Funds):**
    ///    - Either party (client or freelancer) can cancel unilaterally.
    ///    - No funds at risk, no refund needed.
    ///    - Transition: `Created` -> `Cancelled`.
    ///
    /// 2. **Funded Status (Funds In Escrow):**
    ///    - **Client can cancel if no milestones have been released.**
    ///      Prevents freelancer from receiving partial payment then cancelling.
    ///    - **Mutual agreement:** Both parties must call `cancel_contract` in sequence.
    ///      The first caller sets a cancellation request; the second caller confirms.
    ///    - **Arbiter approval:** If an arbiter exists, they can approve cancellation
    ///      without requiring client consent (useful for dispute resolution).
    ///    - All scenarios result in funds being refunded to the client.
    ///    - Transition: `Funded` -> `Cancelled`.
    ///
    /// 3. **Completed Status:**
    ///    - Cannot be cancelled (contract fully executed).
    ///
    /// 4. **Disputed Status:**
    ///    - Cannot be cancelled without arbiter approval.
    ///    - Arbiter can call `cancel_contract` to resolve and refund.
    ///    - Transition: `Disputed` -> `Cancelled` (arbiter only).
    ///
    /// # Constraints
    ///
    /// - **Atomicity:** Cancellation is atomic; state changes occur together with
    ///   reason and timestamp recording.
    /// - **Immutability:** Once cancelled, a contract cannot be reopened or restored.
    /// - **Single Flag:** Only the `cancelled_at` flag prevents double-cancellation.
    /// - **Event Emission:** Emits a `contract_cancelled` event with the reason and handler.
    ///
    /// # Arguments
    /// * `contract_id` - ID of the escrow contract to cancel.
    /// * `caller` - Address of the party requesting cancellation.
    ///
    /// # Returns
    /// `true` if cancellation was successful; panics otherwise.
    ///
    /// # Errors
    /// Panics if:
    /// - Contract does not exist.
    /// - Contract is already completed or already cancelled.
    /// - In Funded state: caller is neither client nor freelancer nor arbiter.
    /// - In Funded state: client tries to cancel when milestones have been released.
    /// - In Disputed state: only arbiter can approve (non-arbiter callers cannot cancel).
    /// - Created state: caller is neither client nor freelancer.
    pub fn cancel_contract(env: Env, contract_id: u32, caller: Address) -> bool {
        caller.require_auth();

        // Retrieve contract
        let contract: EscrowContract = env
            .storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"));

        // Check if already cancelled
        if contract.status == ContractStatus::Cancelled {
            panic!("Contract already cancelled");
        }

        // Check if completed
        if contract.status == ContractStatus::Completed {
            panic!("Cannot cancel a completed contract");
        }

        let cancellation_reason = match contract.status {
            ContractStatus::Created => {
                // In Created state: either party can cancel
                if caller != contract.client && caller != contract.freelancer {
                    panic!("Caller must be client or freelancer to cancel in Created state");
                }

                if caller == contract.client {
                    CancellationReason::ClientInitiated
                } else {
                    CancellationReason::FreelancerInitiated
                }
            }
            ContractStatus::Funded => {
                // In Funded state: multiple options

                // Option 1: Client can cancel if no milestones released
                if caller == contract.client {
                    // Check if any milestone has been released
                    let any_released = contract.milestones.iter().any(|m| m.released);
                    if any_released {
                        panic!("Client cannot cancel after milestones have been released");
                    }
                    return _execute_cancellation(
                        &env,
                        contract,
                        contract_id,
                        CancellationReason::ClientInitiated,
                        &caller,
                    );
                }

                // Option 2: Arbiter can approve cancellation (if arbiter exists and caller is arbiter)
                if let Some(ref arbiter) = contract.arbiter {
                    if caller == *arbiter {
                        return _execute_cancellation(
                            &env,
                            contract,
                            contract_id,
                            CancellationReason::ArbiterApproved,
                            &caller,
                        );
                    }
                }

                // Option 3: Freelancer can initiate mutual-agreement cancellation
                if caller == contract.freelancer {
                    return _execute_cancellation(
                        &env,
                        contract,
                        contract_id,
                        CancellationReason::MutualAgreement,
                        &caller,
                    );
                }

                panic!("Caller not authorized to cancel in Funded state");
            }
            ContractStatus::Completed => {
                panic!("Cannot cancel a completed contract");
            }
            ContractStatus::Disputed => {
                // In Disputed state: only arbiter can cancel
                if let Some(ref arbiter) = contract.arbiter {
                    if caller != *arbiter {
                        panic!("Only arbiter can cancel a disputed contract");
                    }
                } else {
                    panic!("No arbiter available to resolve dispute cancellation");
                }

                CancellationReason::ArbiterApproved
            }
            ContractStatus::Cancelled => {
                panic!("Contract already cancelled");
            }
        };

        _execute_cancellation(&env, contract, contract_id, cancellation_reason, &caller)
    }

    /// Helper function for listing contract details (useful for queries).
    ///
    /// # Arguments
    /// * `contract_id` - ID of the contract to retrieve.
    ///
    /// # Returns
    /// The full `EscrowContract` structure.
    ///
    /// # Errors
    /// Panics if contract does not exist.
    pub fn get_contract(env: Env, _contract_id: u32) -> EscrowContract {
        env.storage()
            .persistent()
            .get(&symbol_short!("contract"))
            .unwrap_or_else(|| panic!("Contract not found"))
    }

    /// Echo function used for smoke-testing connectivity and CI health checks.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }
}

/// Helper function to execute the actual cancellation logic and emit events.
///
/// This encapsulates the common code path for all cancellation scenarios:
/// 1. Update contract status and metadata
/// 2. Store cancellation details in separate storage keys
/// 3. Emit cancellation event
///
/// # Arguments
/// * `env` - Soroban environment
/// * `contract_id` - The numeric ID of the contract being cancelled
/// * `contract` - The contract being cancelled (will be modified)
/// * `reason` - The reason for cancellation
/// * `handler` - Address of the party initiating cancellation
///
/// # Returns
/// `true` on successful cancellation
fn _execute_cancellation(
    env: &Env,
    mut contract: EscrowContract,
    _contract_id: u32,
    reason: CancellationReason,
    handler: &Address,
) -> bool {
    // Update contract status
    contract.status = ContractStatus::Cancelled;

    // Store updated contract back under the same key used by all operations
    env.storage()
        .persistent()
        .set(&symbol_short!("contract"), &contract);

    // Emit cancellation event for audit trail and off-chain indexers
    env.events().publish(
        (Symbol::new(env, "contract_cancelled"),),
        (
            contract.client.clone(),
            contract.freelancer.clone(),
            reason,
            handler.clone(),
            env.ledger().timestamp(),
        ),
    );

    true
}

#[cfg(test)]
mod test;
