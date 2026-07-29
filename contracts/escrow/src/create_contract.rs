use crate::{
    amount_validation, keys, ttl, Contract, ContractStatus, DataKey, Error, Escrow, EscrowArgs,
    EscrowClient, EscrowError, GovernedParameters, Milestone, ReleaseAuthorization, MAX_MILESTONES,
};
use soroban_sdk::{contractimpl, symbol_short, Address, Env, Vec};

impl Escrow {
    /// Creates a new escrow contract with the specified client, freelancer, and milestone amounts.
    ///
    /// This is the single canonical creation path. It enforces:
    /// - Distinct client and freelancer addresses
    /// - Arbiter presence when required by the release authorization mode
    /// - Arbiter distinctness from client and freelancer
    /// - At least one milestone with all amounts strictly positive
    /// - The configurable max-milestones cap (defaults to `MAX_MILESTONES`,
    ///   bounded above by `MAX_MAX_MILESTONES`)
    /// - The governed total-escrow cap combined with the configurable
    ///   max-escrow-stroops cap (the min of the two is enforced; falls back
    ///   to `i128::MAX` when neither is set)
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
    /// * `TooManyMilestones`    - If the number of milestones exceeds the
    ///                            effective max-milestones cap
    /// * `TotalCapExceeded`     - If the sum of milestone amounts exceeds the
    ///                            effective total-escrow cap
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

        if let Some(ref a) = arbiter {
            if a == &client || a == &freelancer {
                env.panic_with_error(EscrowError::InvalidArbiter);
            }
        }

        // The admin-configurable arbiter cap delivered by PR #1243
        // (`Escrow::set_max_arbiters` / `effective_max_arbiters`) is exposed
        // here for forward compatibility with a future multi-arbiter
        // signature. The current `arbiter: Option<Address>` parameter
        // accepts at most one arbiter, and `MIN_MAX_ARBITERS = 1` clamps
        // the admin-set cap to be at least `1`, so a runtime cap check
        // against a single arbiter would be dead code. When the contract
        // signature is extended to `Vec<Address>`, replace this comment
        // with `if arbiter.len() > Escrow::effective_max_arbiters(&env) ...`.

        // Validate at least one milestone is specified.
        if milestones.is_empty() {
            env.panic_with_error(EscrowError::EmptyMilestones);
        }

        // Enforce the configurable max-milestones cap. The getter defaults to
        // `DEFAULT_MAX_MILESTONES` when no admin override has been stored, and
        // `set_max_milestones` clamps administrative updates to
        // `[MIN_MAX_MILESTONES, MAX_MAX_MILESTONES]`, so this check is
        // bounded and safe regardless of caller intent.
        let max_milestones = Self::effective_max_milestones(&env);
        if milestones.len() > max_milestones {
            env.panic_with_error(EscrowError::TooManyMilestones);
        }

        // Combine the governance cap and the admin-configurable cap; the binding
        // cap is the lesser of the two, falling back to `i128::MAX` when neither
        // is set. This keeps legacy deployments (no governance params, no
        // configurable cap) effectively unbounded while letting production
        // deployments tighten the limit via either governance or admin config.
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

        // Validate milestone amounts and enforce the total cap via the
        // canonical helper. The fixed-size scratch buffer is sized for the
        // absolute upper bound (`MAX_MAX_MILESTONES`) so the configurable
        // cap can be raised without re-sizing the buffer.
        let mut native_milestones = [0_i128; crate::MAX_MAX_MILESTONES as usize];
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

        // Retain the original freelancer address alongside `freelancer` so the
        // created event can publish it without re-cloning once the move into
        // the Contract struct below is performed.
        let freelancer_addr = freelancer.clone();

        // Construct the contract with all required fields, initialising
        // accounting counters to zero and reputation_issued to false.
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

        env.storage().persistent().set(&DataKey::Contract(id), &contract);

        let milestone_key = Symbol::new(&env, "milestones");
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
