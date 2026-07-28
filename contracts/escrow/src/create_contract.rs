use crate::{
    amount_validation, ttl, Contract, ContractStatus, DataKey, Error, Escrow, EscrowError,
    GovernedParameters, Milestone, ReleaseAuthorization, MAX_MILESTONES,
    amount_validation, storage_validation, ttl, Contract, ContractStatus, DataKey, Error, Escrow,
    EscrowArgs, EscrowClient, EscrowError, GovernedParameters, Milestone, ReleaseAuthorization,
    MAX_MILESTONES,
};
use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

impl Escrow {
    /// Creates a new escrow contract with the specified client, freelancer, and milestone amounts.
    pub(crate) fn create_contract_impl(
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

        if milestones.len() > MAX_MILESTONES {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        let max_total = env
            .storage()
            .persistent()
            .get::<_, GovernedParameters>(&DataKey::GovernedParameters)
            .map(|params| params.max_escrow_total_stroops)
            .unwrap_or(i128::MAX);

        // Validate milestone amounts
        let mut native_milestones = [0_i128; MAX_MILESTONES as usize];
        let len = milestones.len() as usize;
        for i in 0..len {
            native_milestones[i] = milestones.get(i as u32).unwrap();
        }
        amount_validation::
            validate_milestone_amounts(&native_milestones[..len], max_total)
            .unwrap_or_else(|e| env.panic_with_error(e));

        ttl::extend_next_contract_id_ttl(&env);
        let id = next_contract_id(&env);

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
        for amount in milestones.iter() {
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
        let milestone_key = Symbol::new(&env, "milestones");
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
            (client, freelancer.clone(), env.ledger().timestamp()),
        );

        // Maintain participant and status indexes for paginated readers.
        status_index::index_new_contract(&env, id, &ContractStatus::Created);
        status_index::index_participant(&env, id, &contract.client, 0);
        status_index::index_participant(&env, id, &contract.freelancer, 1);

        id
    }
}

/// Returns the next available contract ID and asserts it is not already occupied.
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