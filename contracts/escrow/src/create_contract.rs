use crate::{
    amount_validation, events, ttl, Contract, ContractStatus, CreatedEvent, DataKey, DepositEvent,
    Error, Escrow, EscrowArgs, EscrowClient, EscrowError, GovernedParameters, Milestone,
    ReleaseAuthorization, MAX_MILESTONES,
};
use soroban_sdk::{contractimpl, token, Address, Env, Symbol, Vec};

/// Validated and pre-built escrow parameters, ready to be written in one
/// storage pass.  Produced by [`build_escrow`]; consumed by `create_contract`
/// and `create_and_fund`.
pub(crate) struct BuiltEscrow {
    pub id: u32,
    pub contract: Contract,
    pub milestone_vec: Vec<Milestone>,
    pub total_amount: i128,
}

/// Shared validation and construction logic for both `create_contract` and
/// `create_and_fund`.
///
/// Allocates the next contract ID, validates all participants and milestone
/// amounts, constructs the `Contract` record (with the caller-supplied initial
/// accounting values) and the milestone vector, advances the ID counter, but
/// does **not** write anything to persistent storage — the caller calls
/// [`commit_escrow`] at the appropriate point.
///
/// # Panics
/// Panics with the corresponding `EscrowError` on any validation failure.
pub(crate) fn build_escrow(
    env: &Env,
    client: &Address,
    freelancer: &Address,
    arbiter: &Option<Address>,
    milestones: &Vec<i128>,
    release_authorization: ReleaseAuthorization,
    initial_status: ContractStatus,
    initial_funded_amount: i128,
    initial_total_deposited: i128,
) -> BuiltEscrow {
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
        if arb == client || arb == freelancer {
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
            EscrowError::TotalCapExceeded => env.panic_with_error(EscrowError::TotalCapExceeded),
            _ => env.panic_with_error(EscrowError::InvalidMilestoneAmount),
        },
    }

    // Compute total milestone amount (already validated, so no overflow possible
    // within the allowed cap — but use checked_add for defense-in-depth).
    let total_amount: i128 = {
        let mut sum: i128 = 0;
        for i in 0..len {
            sum = sum
                .checked_add(native_milestones[i])
                .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
        }
        sum
    };

    // Extend TTL for the next-contract-id counter before reading it.
    ttl::extend_next_contract_id_ttl(env);

    let id = next_contract_id(env);

    // Construct the contract record with caller-supplied initial accounting.
    let contract = Contract {
        client: client.clone(),
        freelancer: freelancer.clone(),
        arbiter: arbiter.clone(),
        status: initial_status,
        total_deposited: initial_total_deposited,
        funded_amount: initial_funded_amount,
        released_amount: 0,
        refunded_amount: 0,
        release_authorization,
        reputation_issued: false,
    };

    // Build the milestone vector.
    let mut milestone_vec: Vec<Milestone> = Vec::new(env);
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

    // Advance the ID counter.  `next_contract_id` already checked that the
    // current id is not occupied; the checked_add is a defense-in-depth guard.
    let next_id = id
        .checked_add(1)
        .unwrap_or_else(|| env.panic_with_error(Error::ContractIdOverflow));
    env.storage()
        .persistent()
        .set(&DataKey::NextContractId, &next_id);

    BuiltEscrow {
        id,
        contract,
        milestone_vec,
        total_amount,
    }
}

/// Write the built escrow and its milestones to persistent storage in a single
/// pass, extending TTL for both keys.
pub(crate) fn commit_escrow(env: &Env, built: &BuiltEscrow) {
    env.storage()
        .persistent()
        .set(&DataKey::Contract(built.id), &built.contract);

    let milestone_key = Symbol::new(env, "milestones");
    env.storage().persistent().set(
        &(DataKey::Contract(built.id), milestone_key),
        &built.milestone_vec,
    );

    ttl::extend_contract_ttl(env, built.id);
    ttl::extend_milestone_ttl(env, built.id);
}

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
        // mutating entrypoint halts uniformly. Runs before auth.
        Self::require_not_paused(&env);

        client.require_auth();

        let built = build_escrow(
            &env,
            &client,
            &freelancer,
            &arbiter,
            &milestones,
            release_authorization,
            ContractStatus::Created,
            0,
            0,
        );

        commit_escrow(&env, &built);

        let freelancer_addr = freelancer.clone();

        // Emit creation event for indexers and off-chain subscribers.
        events::emit_created(
            &env,
            built.id,
            CreatedEvent {
                client: client.clone(),
                freelancer: freelancer_addr.clone(),
                token: crate::Escrow::read_settlement_token(&env),
                total: built.total_amount,
                milestone_count: built.milestone_vec.len(),
            },
        );

        built.id
    }

    /// Creates a new escrow contract **and** fully funds it in a single atomic
    /// transaction.
    ///
    /// Equivalent to calling `create_contract` then `deposit_funds` for the
    /// full `total_amount`, but:
    /// - Performs only **one** storage write for the escrow record instead of two.
    /// - Requires only **one** client signature / transaction.
    /// - The freelancer never observes a `Created` escrow that may never be funded.
    /// - Emits both `("created", id)` and `("deposit", id)` events in order so
    ///   existing indexers need no changes.
    ///
    /// The settlement token must be bound via `bind_settlement_token` before
    /// this entrypoint can be called.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `client` - The address of the client funding the contract (must authorize)
    /// * `freelancer` - The address of the freelancer performing the work
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestones` - Vector of milestone amounts (in stroops)
    /// * `release_authorization` - Authorization mode for milestone releases
    ///
    /// # Returns
    /// The unique contract ID assigned to the new escrow (as `u64`).
    ///
    /// # Errors
    /// All errors from `create_contract` validation, plus:
    /// * `NotInitialized`               - If `initialize` has not been called
    /// * `ContractPaused`               - If the contract is paused or in emergency
    /// * `SettlementTokenNotConfigured` - If no settlement token has been bound
    ///
    /// # Events
    /// Emits in order:
    /// 1. `("created", id)` with `(client, freelancer, timestamp)` — identical
    ///    payload to `create_contract`.
    /// 2. `("deposit", id)` with `(caller, amount, funded_amount, status, timestamp)` —
    ///    identical payload to `deposit_funds`.
    pub fn create_and_fund(
        env: Env,
        client: Address,
        freelancer: Address,
        arbiter: Option<Address>,
        milestones: Vec<i128>,
        release_authorization: ReleaseAuthorization,
    ) -> u64 {
        Self::require_initialized(&env);
        // Reject while paused or in emergency mode.
        Self::require_not_paused(&env);

        // Authenticate the client before the cross-contract SAC transfer so
        // the auth check is always at the root invocation level.
        client.require_auth();

        // Build and validate the escrow.  `build_escrow` computes `total_amount`
        // from the milestones so we can set `funded_amount` and
        // `total_deposited` correctly before the single storage write.
        let built = build_escrow(
            &env,
            &client,
            &freelancer,
            &arbiter,
            &milestones,
            release_authorization,
            ContractStatus::Funded,
            0, // placeholder — patched below after we know total_amount
            0, // placeholder — patched below after we know total_amount
        );

        let total_amount = built.total_amount;
        let id = built.id;

        // Patch the escrow record's accounting to reflect full funding before
        // writing it.  We mutate the built struct in place.
        let funded_contract = Contract {
            funded_amount: total_amount,
            total_deposited: total_amount,
            status: ContractStatus::Funded,
            ..built.contract
        };

        let funded_built = BuiltEscrow {
            id,
            contract: funded_contract,
            milestone_vec: built.milestone_vec,
            total_amount,
        };

        // Single storage write for both contract and milestones.
        commit_escrow(&env, &funded_built);

        // Transfer the full amount from the client into the escrow.
        // This is done AFTER the state write to follow CEI ordering: if the
        // SAC transfer fails, the contract state is already written — however
        // the entire transaction is atomic, so a failed transfer rolls back the
        // whole invocation and the storage write never lands on-chain.
        let token = Self::read_settlement_token(&env)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::SettlementTokenNotConfigured));
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&client, &env.current_contract_address(), &total_amount);

        let timestamp = env.ledger().timestamp();

        // Emit `created` event — structured payload.
        events::emit_created(
            &env,
            id,
            CreatedEvent {
                client: client.clone(),
                freelancer: freelancer.clone(),
                token: Some(token.clone()),
                total: total_amount,
                milestone_count: funded_built.milestone_vec.len(),
            },
        );

        // Emit `deposit` event — structured payload.
        events::emit_deposit(
            &env,
            id,
            DepositEvent {
                from: client,
                amount: total_amount,
                funded_amount: total_amount,
                status: ContractStatus::Funded,
            },
        );

        id as u64
    }
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
