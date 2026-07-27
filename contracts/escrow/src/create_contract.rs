use crate::{
    amount_validation, ttl, Contract, ContractStatus, DataKey, Error, Escrow, EscrowArgs,
    EscrowClient, EscrowError, GovernedParameters, Milestone, ReleaseAuthorization,
    DEFAULT_MAX_MILESTONES, DEFAULT_MAX_TOTAL_ESCROW_STROOPS,
};
use soroban_sdk::{contractimpl, symbol_short, Address, Env, Symbol, Vec};

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
        Self::require_not_paused(&env);

        client.require_auth();

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

        let max_milestones = Self::effective_max_milestones(&env);
        if milestones.len() > max_milestones {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        let max_total = {
            let governed = env
                .storage()
                .persistent()
                .get::<_, GovernedParameters>(&DataKey::GovernedParameters)
                .map(|params| params.max_escrow_total_stroops)
                .unwrap_or(i128::MAX);
            let configurable = Self::effective_max_escrow_stroops(&env);
            governed.min(configurable)
        };

        let max_milestones_usize = max_milestones as usize;
        let mut native_milestones = [0_i128; 100];
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

        ttl::extend_next_contract_id_ttl(&env);

        let id = Self::next_contract_id(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: arbiter.clone(),
            status: ContractStatus::Created,
            total_deposited: 0,
            funded_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
            release_authorization,
            reputation_issued: false,
        };

        env.storage().persistent().set(&DataKey::Contract(id), &contract);

        let milestone_key = Symbol::new(&env, "milestones");
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
        env.storage()
            .persistent()
            .set(&(DataKey::Contract(id), milestone_key), &milestone_vec);

        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractIdOverflow));
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &next_id);

        env.events().publish(
            (symbol_short!("created"), id),
            (client, freelancer, env.ledger().timestamp()),
        );

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
}
