use crate::{
    amount_validation, keys, ttl, Contract, ContractStatus, DataKey, Error, Escrow, EscrowArgs,
    EscrowClient, EscrowError, GovernedParameters, Milestone, ReleaseAuthorization, MAX_MILESTONES,
};
use soroban_sdk::{contractimpl, symbol_short, Address, Env, Vec};

impl Escrow {
    /// Creates a new escrow contract with the specified client, freelancer, and milestone amounts.
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

        if let Some(ref a) = arbiter {
            if a == &client || a == &freelancer {
                env.panic_with_error(EscrowError::InvalidArbiter);
            }
        }

        if milestones.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }

        if milestones.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        let max_total = env
            .storage()
            .persistent()
            .get::<_, GovernedParameters>(&DataKey::GovernedParameters)
            .map(|p| p.max_escrow_total_stroops)
            .unwrap_or(i128::MAX);

        // Copy into a native fixed-size array for the shared validator helper.
        let mut native_milestones = [0_i128; MAX_MILESTONES as usize];
        let len = milestones.len() as usize;
        for i in 0..len {
            let v = milestones.get(i as u32).unwrap();
            if v <= 0 {
                env.panic_with_error(EscrowError::InvalidMilestoneAmount);
            }
            native_milestones[i] = v;
        }

        match amount_validation::validate_milestone_amounts(&native_milestones[..len], max_total) {
            Ok(_) => {}
            Err(e) => env.panic_with_error(e),
        }

        ttl::extend_next_contract_id_ttl(&env);
        let id = next_contract_id(&env);

        let contract = Contract {
            client: client.clone(),
            freelancer: freelancer.clone(),
            arbiter: arbiter.clone(),
        let freelancer_addr = freelancer.clone();

        // Construct the contract with all required fields, initialising accounting
        // counters to zero and reputation_issued to false.
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

        // Maintain append-only participant indices for fast enumeration.
        // These are updated after the contract is persisted to keep the index consistent.
        let client_key = DataKey::ClientContracts(client.clone());
        let mut client_ids: Vec<u32> = env
            .storage()
            .persistent()
            .get(&client_key)
            .unwrap_or_else(|| Vec::new(&env));
        client_ids.push_back(id);
        env.storage().persistent().set(&client_key, &client_ids);
        ttl::extend_participant_contract_index_ttl(&env, &client_key);

        let freelancer_key = DataKey::FreelancerContracts(freelancer_addr.clone());
        let mut freelancer_ids: Vec<u32> = env
            .storage()
            .persistent()
            .get(&freelancer_key)
            .unwrap_or_else(|| Vec::new(&env));
        freelancer_ids.push_back(id);
        env.storage()
            .persistent()
            .set(&freelancer_key, &freelancer_ids);
        ttl::extend_participant_contract_index_ttl(&env, &freelancer_key);

        // Build and persist the milestone vector.
        let mut milestone_vec: Vec<Milestone> = Vec::new(&env);
        for i in 0..len {
            let amount = native_milestones[i];
            milestone_vec.push_back(Milestone {
                amount: *amount,
                funded_amount: 0,
                released: false,
                refunded: false,
                work_evidence: None,
                refunded_amount: 0,
                deadline: None,
            });
        }
        let milestone_key = keys::milestone_key(&env, id);
        env.storage()
            .persistent()
            .set(&milestone_key, &milestone_vec);

        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(Error::ContractIdOverflow));
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &next_id);

        env.events().publish(
            (symbol_short!("created"), id),
            (client, freelancer.clone(), env.ledger().timestamp()),
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
            (client, freelancer_addr, env.ledger().timestamp()),
        );

        status_index::index_new_contract(&env, id, &ContractStatus::Created);
        status_index::index_participant(&env, id, &contract.client, 0);
        status_index::index_participant(&env, id, &contract.freelancer, 1);

        id
    }
}

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
