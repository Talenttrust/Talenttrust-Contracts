use crate::{
    amount_validation, ttl, Contract, ContractStatus, DataKey, EscrowError, GovernedParameters,
    Milestone, ReleaseAuthorization, Error, MAX_MILESTONES, status_index,
};
use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

#[contractimpl]
impl Escrow {
    /// Creates a new escrow contract with the specified client, freelancer, and milestone amounts.
    ///
    /// This is the single canonical creation path. It enforces:
    /// - Distinct client and freelancer addresses
    /// - Arbiter presence when required by the release authorization mode
    /// - Arbiter distinctness from client and freelancer
    /// - At least one milestone with all amounts strictly positive
    /// - The `MAX_MILESTONES` cap
    /// - The governed total-escrow cap (falls back to `i128::MAX` when unset)
    /// - No contract-id collision or overflow
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `client` - The address of the client funding the contract
    /// * `freelancer` - The address of the freelancer performing the work
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestones` - Vector of milestone amounts (in stroops)
    /// * `release_authorization` - Authorization mode for milestone releases
    ///
    /// # Returns
    /// The unique contract ID assigned to the new escrow.
    ///
    /// # Errors
    /// * `InvalidParticipant`   - If client and freelancer are the same address
    /// * `EmptyMilestones`      - If no milestones are provided
    /// * `InvalidMilestoneAmount` - If any milestone amount is <= 0
    /// * `MissingArbiter`       - If arbiter is required but not provided
    /// * `InvalidArbiter`       - If arbiter is same as client or freelancer
    /// * `TooManyMilestones`    - If the number of milestones exceeds `MAX_MILESTONES`
    /// * `TotalCapExceeded`     - If the sum of milestone amounts exceeds the governed cap
    /// * `ContractIdOverflow`   - If the next id would exceed `u32::MAX`
    /// * `ContractIdCollision`  - If the allocated id slot is already occupied
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestones: Vec<i128>,
        release_authorization: ReleaseAuthorization,
    ) -> u32 {
        // Reject state-changing calls while paused or in emergency mode so every
        // mutating entrypoint halts uniformly. Runs before auth. See
        // finalize.rs::require_not_paused.
        Self::require_not_paused(&env);

        client.require_auth();

        // Validate that client and freelancer are distinct participants.
        if client == freelancer {
            env.panic_with_error(EscrowError::InvalidParticipant);
        }

        // Validate arbiter requirement based on release authorization mode.
        match release_authorization {
            ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter
                if arbiter.is_none() =>
            {
                env.panic_with_error(EscrowError::MissingArbiter);
            }
            _ => {}
        }

    // Validate arbiter is distinct from both client and freelancer.
    if let Some(ref arb) = arbiter {
        if arb == &client || arb == &freelancer {
            env.panic_with_error(EscrowError::InvalidArbiter);
        }
    }

    // Validate at least one milestone is specified.
    if milestones.is_empty() {
        env.panic_with_error(EscrowError::EmptyMilestones);
    }

    // Enforce maximum number of milestones.
    if milestones.len() > MAX_MILESTONES {
        env.panic_with_error(EscrowError::TooManyMilestones);
    }

    // Retrieve governed parameters for total escrow cap; allow any total if unset.
    let max_total = env
        .storage()
        .persistent()
        .get::<_, GovernedParameters>(&DataKey::GovernedParameters)
        .map(|params| params.max_escrow_total_stroops)
        .unwrap_or(i128::MAX);

    // Validate milestone amounts and enforce the total cap via the canonical helper.
    let mut native_milestones = [0_i128; MAX_MILESTONES as usize];
    let len = milestones.len() as usize;
    for i in 0..len {
        native_milestones[i] = milestones.get(i as u32).unwrap();
    }
    match amount_validation::validate_milestone_amounts(&native_milestones[..len], max_total) {
        Ok(_) => (),
        Err(err) => match err {
            EscrowError::InvalidMilestoneAmount => {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount)
            }
            EscrowError::TotalCapExceeded => {
                env.panic_with_error(EscrowError::TotalCapExceeded)
            }
            _ => env.panic_with_error(EscrowError::InvalidMilestoneAmount),
        },
    }

        // Extend TTL for the next-contract-id counter before reading it.
        ttl::extend_next_contract_id_ttl(&env);

        let id = next_contract_id(&env);

        let freelancer_addr = freelancer.clone();

    let freelancer_addr = freelancer.clone();
    let contract = Contract {
        client: client.clone(),
        freelancer: freelancer.clone(),
        arbiter,
        status: ContractStatus::Created,
        total_deposited: 0,
        funded_amount: 0,
        released_amount: 0,
        refunded_amount: 0,
        release_authorization,
        reputation_issued: false,
    };
    env.storage()
        .persistent()
        .set(&DataKey::Contract(id), &contract);

    let mut milestone_vec: Vec<Milestone> = Vec::new(&env);
    for (i, amount) in milestones.iter().enumerate() {
        let deadline = deadlines.as_ref().and_then(|d| d.get(i as u32));
        milestone_vec.push_back(Milestone {
            amount,
            funded_amount: 0,
            released: false,
            refunded: false,
            work_evidence: None,
            refunded_amount: 0,
            release_authorization,
            reputation_issued: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &contract);

        // Build and persist the milestone vector.
        let mut milestone_vec: Vec<Milestone> = Vec::new(&env);
        for amount in milestones.iter() {
            milestone_vec.push_back(Milestone {
                amount,
                funded_amount: 0,
                released: false,
                refunded: false,
                work_evidence: None,
                refunded_amount: 0,
                deadline: None,
            });
        }
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(id), milestone_key), &milestone_vec);

        // Advance the counter. `next_contract_id` already checked `id < u32::MAX`;
        // the `checked_add` here is a defense-in-depth guard.
        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractIdOverflow));
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &next_id);

    // Emit creation event for indexers and off-chain subscribers.
    env.events().publish(
        (symbol_short!("created"), id),
        (client, freelancer_addr, env.ledger().timestamp()),
    );

    // Maintain participant and status indexes for paginated readers.
    status_index::index_new_contract(&env, id, &ContractStatus::Created);
    status_index::index_participant(&env, id, &contract.client, 0);
    status_index::index_participant(&env, id, &contract.freelancer, 1);

    id
}

/// Returns the next available contract ID and asserts it is not already occupied.
///
/// # Errors
/// * `ContractIdCollision` - If the allocated id slot is already occupied
pub(crate) fn next_contract_id(env: &Env) -> u32 {
    let id: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::NextContractId)
        .unwrap_or(1);

    if env
        .storage()
        .persistent()
        .get::<_, Contract>(&DataKey::Contract(id))
        .is_some()
    {
        env.panic_with_error(Error::ContractIdCollision);
    }

    id
}
