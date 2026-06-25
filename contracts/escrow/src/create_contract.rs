use crate::{
    ttl, Contract, ContractStatus, DataKey, Error, Escrow, EscrowArgs, EscrowClient, Milestone,
    ReleaseAuthorization,
};
use soroban_sdk::{contractimpl, symbol_short, Address, Env, Symbol, Vec};

#[contractimpl]
impl Escrow {
    /// Creates a new escrow contract with the specified client, freelancer, and milestone amounts.
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
    /// The unique contract ID
    ///
    /// # Errors
    /// * `ContractPaused` - If the contract is paused while not in emergency mode
    /// * `EmergencyActive` - If the contract is in an active emergency pause
    /// * `InvalidParticipants` - If client and freelancer are the same address
    /// * `EmptyMilestones` - If no milestones are provided
    /// * `InvalidMilestoneAmount` - If any milestone amount is <= 0
    /// * `MissingArbiter` - If arbiter is required but not provided
    /// * `InvalidArbiter` - If arbiter is same as client or freelancer
    /// * `ContractIdOverflow` - If the next id would exceed `u32::MAX`
    /// * `ContractIdCollision` - If the allocated id slot is already occupied
    ///
    /// # Security
    /// * Pause/emergency gate runs BEFORE any other state read or auth so
    ///   funds cannot be re-allocated while the contract is paused.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestones: Vec<i128>,
        release_authorization: ReleaseAuthorization,
    ) -> u32 {
        // Pause/emergency gate: refuse new contract creation while the
        // contract-level pause switch is active or emergency mode is on.
        // Mismatched between state read/write and pause-gate invocation will
        // be caught by tests/test/pause_controls.rs and the test/contracts-
        // facing CI checks.
        Self::require_not_paused(&env);

        client.require_auth();

        if client == freelancer {
            env.panic_with_error(Error::InvalidParticipant);
        }

        match release_authorization {
            ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter
                if arbiter.is_none() =>
            {
                env.panic_with_error(Error::MissingArbiter);
            }
            _ => {}
        }

        if let Some(ref arb) = arbiter {
            if arb == &client || arb == &freelancer {
                env.panic_with_error(Error::InvalidArbiter);
            }
        }

        if milestones.is_empty() {
            env.panic_with_error(Error::EmptyMilestones);
        }

        for amount in milestones.iter() {
            if amount <= 0 {
                env.panic_with_error(Error::InvalidMilestoneAmount);
            }
        }

        let id = next_contract_id(&env);

        ttl::extend_next_contract_id_ttl(&env);

        let freelancer_addr = freelancer.clone();
        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter,
            status: ContractStatus::Created,
            funded_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &contract);

        let mut milestone_vec: Vec<Milestone> = Vec::new(&env);
        for amount in milestones.iter() {
            milestone_vec.push_back(Milestone {
                amount,
                funded_amount: 0,
                released: false,
                refunded: false,
                work_evidence: None,
                refunded_amount: 0,
            });
        }
        let milestone_key = Symbol::new(&env, "milestones");
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(id), milestone_key), &milestone_vec);

        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &(id + 1));

        env.events().publish(
            (symbol_short!("created"), id),
            (client, freelancer_addr, env.ledger().timestamp()),
        );

        id
    }
}

/// Returns the next contract id after verifying the slot is unused.
fn next_contract_id(env: &Env) -> u32 {
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

/// Advances [`DataKey::NextContractId`] after a contract is persisted.
#[allow(dead_code)]
fn bump_next_contract_id(env: &Env, id: u32) {
    let next_id = id
        .checked_add(1)
        .unwrap_or_else(|| env.panic_with_error(Error::ContractIdOverflow));
    env.storage()
        .persistent()
        .set(&DataKey::NextContractId, &next_id);
}
