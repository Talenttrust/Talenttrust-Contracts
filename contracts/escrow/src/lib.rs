#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env, Symbol,
    Vec,
};

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

/// Errors returned by the escrow contract.
/// `#[contracterror]` generates `Into<soroban_sdk::Error>` so these values
/// can be passed to `panic_with_error!` and surfaced to off-chain tooling.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowError {
    /// The requested escrow contract ID does not exist in storage.
    ContractNotFound = 1,
    /// The milestone index is outside the range of defined milestones.
    InvalidMilestoneId = 2,
    /// The milestone has already been released; double-release is not allowed.
    MilestoneAlreadyReleased = 3,
    /// Release was attempted before all release-readiness items were satisfied.
    ChecklistIncomplete = 4,
    /// Deposit amount must be strictly positive.
    InvalidDepositAmount = 5,
    /// The number of milestones exceeds the allowed maximum.
    TooManyMilestones = 6,
}

// ---------------------------------------------------------------------------
// Storage key
// ---------------------------------------------------------------------------

/// Discriminator for all persistent storage keys used by this contract.
#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    /// Stores `EscrowData` for the given contract ID.
    Contract(u32),
    /// Stores `ReleaseChecklist` for the given contract ID.
    Checklist(u32),
    /// Monotonically-increasing counter used to generate unique contract IDs.
    NextId,
    /// Administrator address for pause controls.
    Admin,
    /// Whether the contract is currently paused.
    Paused,
    /// Whether emergency mode is active.
    EmergencyPaused,
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Overall lifecycle status of one escrow agreement.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    /// Initial state -- contract has been created but not yet funded.
    Created = 0,
    /// Client has deposited funds; milestones can now be released.
    Funded = 1,
    /// All milestones have been released.
    Completed = 2,
    /// A dispute has been raised and is pending resolution.
    Disputed = 3,
}

/// A single payment step in a milestone-based escrow agreement.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    /// Payment amount in stroops (1 XLM = 10 000 000 stroops).
    pub amount: i128,
    /// Whether this milestone's funds have been released to the freelancer.
    pub released: bool,
}

/// All persistent state associated with one escrow contract.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowData {
    /// The hiring party who deposits funds.
    pub client: Address,
    /// The service provider who receives milestone payments.
    pub freelancer: Address,
    /// Ordered list of payment milestones.
    pub milestones: Vec<Milestone>,
    /// Current lifecycle status.
    pub status: ContractStatus,
    /// Total amount deposited by the client so far.
    pub deposited_amount: i128,
}

// ---------------------------------------------------------------------------
// Release-readiness checklist
// ---------------------------------------------------------------------------

/// Tracks whether each deployment, verification, and post-deploy monitoring
/// gate has been satisfied for a specific escrow contract.
///
/// Items are **automatically** updated by contract operations -- no external
/// caller may set them directly, preventing unauthorized state manipulation.
///
/// # Phases
///
/// **Deployment**
/// - `contract_created` -- set when `create_contract` succeeds.
/// - `funds_deposited`  -- set when `deposit_funds` succeeds with amount > 0.
///
/// **Verification**
/// - `parties_authenticated` -- set at contract creation (both addresses recorded).
/// - `milestones_defined`    -- set at contract creation when >= 1 milestone exists.
///
/// **Post-Deploy Monitoring**
/// - `all_milestones_released` -- set when the final milestone is released.
/// - `reputation_issued`       -- set when `issue_reputation` is called.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ReleaseChecklist {
    // -- Deployment --
    /// Contract has been successfully created and persisted.
    pub contract_created: bool,
    /// Client has deposited a positive amount into escrow.
    pub funds_deposited: bool,

    // -- Verification --
    /// Both client and freelancer addresses have been recorded.
    pub parties_authenticated: bool,
    /// At least one milestone amount has been defined.
    pub milestones_defined: bool,

    // -- Post-Deploy Monitoring --
    /// Every milestone in the agreement has been released.
    pub all_milestones_released: bool,
    /// A reputation credential has been issued for the freelancer.
    pub reputation_issued: bool,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

/// Maximum number of milestones per escrow contract.
/// Guards against excessive storage cost and iteration gas.
pub const MAX_MILESTONES: u32 = 20;

#[contract]
pub struct Escrow;

// ---------------------------------------------------------------------------
// Private helpers (pause controls)
// ---------------------------------------------------------------------------

impl Escrow {
    fn read_admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("Pause controls are not initialized"))
    }

    fn require_admin(env: &Env) {
        let admin = Self::read_admin(env);
        admin.require_auth();
    }

    fn is_paused_internal(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn is_emergency_internal(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::EmergencyPaused)
            .unwrap_or(false)
    }

    fn ensure_not_paused(env: &Env) {
        if Self::is_paused_internal(env) {
            panic!("Contract is paused");
        }
    }
}

// ---------------------------------------------------------------------------
// Public contract implementation
// ---------------------------------------------------------------------------

#[contractimpl]
impl Escrow {
    // -----------------------------------------------------------------------
    // Pause controls
    // -----------------------------------------------------------------------

    /// Initialize admin-managed pause controls.
    ///
    /// # Panics
    /// - If called more than once.
    pub fn initialize(env: Env, admin: Address) -> bool {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Pause controls already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        true
    }

    /// Returns the configured pause-control administrator.
    pub fn get_admin(env: Env) -> Address {
        Self::read_admin(&env)
    }

    /// Pause state-changing operations for incident response.
    pub fn pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    /// Lift a normal pause.
    ///
    /// # Panics
    /// - If emergency mode is still active.
    /// - If the contract is not currently paused.
    pub fn unpause(env: Env) -> bool {
        Self::require_admin(&env);
        if Self::is_emergency_internal(&env) {
            panic!("Emergency pause active");
        }
        if !Self::is_paused_internal(&env) {
            panic!("Contract is not paused");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        true
    }

    /// Activate emergency mode and hard-pause the contract.
    pub fn activate_emergency_pause(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &true);
        env.storage().instance().set(&DataKey::Paused, &true);
        true
    }

    /// Resolve emergency mode and restore normal operations.
    pub fn resolve_emergency(env: Env) -> bool {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::EmergencyPaused, &false);
        env.storage().instance().set(&DataKey::Paused, &false);
        true
    }

    /// Read-only pause status.
    pub fn is_paused(env: Env) -> bool {
        Self::is_paused_internal(&env)
    }

    /// Read-only emergency status.
    pub fn is_emergency(env: Env) -> bool {
        Self::is_emergency_internal(&env)
    }

    // -----------------------------------------------------------------------
    // Core escrow operations
    // -----------------------------------------------------------------------

    /// Create a new escrow agreement and return its unique contract ID.
    ///
    /// # Arguments
    /// * `client`            -- Address of the hiring party.
    /// * `freelancer`        -- Address of the service provider.
    /// * `milestone_amounts` -- Ordered list of per-milestone payment amounts
    ///                          (in stroops). Must be non-empty and <= `MAX_MILESTONES`.
    ///
    /// # Checklist updates
    /// Sets `contract_created`, `parties_authenticated`, and `milestones_defined`.
    ///
    /// # Panics
    /// Panics with `TooManyMilestones` if the milestone count is 0 or exceeds `MAX_MILESTONES`.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
    ) -> u32 {
        Self::ensure_not_paused(&env);

        let count = milestone_amounts.len();
        if count == 0 || count > MAX_MILESTONES {
            panic_with_error!(&env, EscrowError::TooManyMilestones)
        }

        // Allocate a unique ID.
        let id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextId)
            .unwrap_or(0u32)
            + 1;
        env.storage().persistent().set(&DataKey::NextId, &id);

        // Build milestone list.
        let mut milestones: Vec<Milestone> = Vec::new(&env);
        for amount in milestone_amounts.iter() {
            milestones.push_back(Milestone {
                amount,
                released: false,
            });
        }

        // Persist contract data.
        let data = EscrowData {
            client,
            freelancer,
            milestones,
            status: ContractStatus::Created,
            deposited_amount: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Contract(id), &data);

        // Initialise checklist -- deployment + verification items are satisfied
        // by the act of calling this function successfully.
        let checklist = ReleaseChecklist {
            contract_created: true,
            funds_deposited: false,
            parties_authenticated: true,
            milestones_defined: true,
            all_milestones_released: false,
            reputation_issued: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Checklist(id), &checklist);

        id
    }

    /// Deposit funds into escrow on behalf of the client.
    ///
    /// # Arguments
    /// * `contract_id` -- ID returned by `create_contract`.
    /// * `amount`      -- Amount in stroops; must be strictly positive.
    ///
    /// # Checklist updates
    /// Sets `funds_deposited` and advances status to `Funded`.
    ///
    /// # Panics
    /// Panics with `InvalidDepositAmount` if `amount <= 0`.
    /// Panics with `ContractNotFound` if `contract_id` does not exist.
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        Self::ensure_not_paused(&env);

        if amount <= 0 {
            panic_with_error!(&env, EscrowError::InvalidDepositAmount)
        }

        let mut data: EscrowData = match env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
        {
            Some(d) => d,
            None => panic_with_error!(&env, EscrowError::ContractNotFound),
        };

        data.deposited_amount += amount;
        data.status = ContractStatus::Funded;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &data);

        // Mark funds_deposited on the checklist.
        let mut checklist: ReleaseChecklist = match env
            .storage()
            .persistent()
            .get(&DataKey::Checklist(contract_id))
        {
            Some(c) => c,
            None => panic_with_error!(&env, EscrowError::ContractNotFound),
        };
        checklist.funds_deposited = true;
        env.storage()
            .persistent()
            .set(&DataKey::Checklist(contract_id), &checklist);

        true
    }

    /// Release the payment for one milestone to the freelancer.
    ///
    /// # Arguments
    /// * `contract_id`  -- ID returned by `create_contract`.
    /// * `milestone_id` -- Zero-based index of the milestone to release.
    ///
    /// # Enforcement
    /// This function panics with `ChecklistIncomplete` if `is_release_ready`
    /// returns `false` -- i.e., if the deployment or verification checklist
    /// items have not all been satisfied.
    ///
    /// # Checklist updates
    /// Sets `all_milestones_released` when the final milestone is released.
    ///
    /// # Panics
    /// - `ChecklistIncomplete`       -- release-readiness gates not all met.
    /// - `ContractNotFound`          -- unknown `contract_id`.
    /// - `InvalidMilestoneId`        -- `milestone_id` out of range.
    /// - `MilestoneAlreadyReleased`  -- milestone was already released.
    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        Self::ensure_not_paused(&env);

        // Hard enforcement gate -- all deployment + verification items must pass.
        if !Self::is_release_ready(env.clone(), contract_id) {
            panic_with_error!(&env, EscrowError::ChecklistIncomplete)
        }

        let mut data: EscrowData = match env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
        {
            Some(d) => d,
            None => panic_with_error!(&env, EscrowError::ContractNotFound),
        };

        if milestone_id >= data.milestones.len() {
            panic_with_error!(&env, EscrowError::InvalidMilestoneId)
        }

        let mut milestone = data.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic_with_error!(&env, EscrowError::MilestoneAlreadyReleased)
        }
        milestone.released = true;
        data.milestones.set(milestone_id, milestone);

        // Check if every milestone is now released.
        let all_released = data.milestones.iter().all(|m| m.released);
        if all_released {
            data.status = ContractStatus::Completed;
        }
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &data);

        // Update post-deploy checklist item.
        if all_released {
            let mut checklist: ReleaseChecklist = match env
                .storage()
                .persistent()
                .get(&DataKey::Checklist(contract_id))
            {
                Some(c) => c,
                None => panic_with_error!(&env, EscrowError::ContractNotFound),
            };
            checklist.all_milestones_released = true;
            env.storage()
                .persistent()
                .set(&DataKey::Checklist(contract_id), &checklist);
        }

        true
    }

    /// Issue a reputation credential for the freelancer upon contract completion.
    ///
    /// # Arguments
    /// * `contract_id` -- ID returned by `create_contract`.
    /// * `_freelancer` -- Address of the freelancer receiving the credential.
    /// * `_rating`     -- Reputation score (interpretation left to callers).
    ///
    /// # Checklist updates
    /// Sets `reputation_issued`.
    ///
    /// # Panics
    /// Panics with `ContractNotFound` if `contract_id` does not exist.
    pub fn issue_reputation(
        env: Env,
        contract_id: u32,
        _freelancer: Address,
        _rating: i128,
    ) -> bool {
        let mut checklist: ReleaseChecklist = match env
            .storage()
            .persistent()
            .get(&DataKey::Checklist(contract_id))
        {
            Some(c) => c,
            None => panic_with_error!(&env, EscrowError::ContractNotFound),
        };
        checklist.reputation_issued = true;
        env.storage()
            .persistent()
            .set(&DataKey::Checklist(contract_id), &checklist);
        true
    }

    // -----------------------------------------------------------------------
    // Release-readiness checklist queries
    // -----------------------------------------------------------------------

    /// Return the full release-readiness checklist for an escrow contract.
    ///
    /// # Arguments
    /// * `contract_id` -- ID returned by `create_contract`.
    ///
    /// # Panics
    /// Panics with `ContractNotFound` if `contract_id` does not exist.
    pub fn get_release_checklist(env: Env, contract_id: u32) -> ReleaseChecklist {
        match env
            .storage()
            .persistent()
            .get(&DataKey::Checklist(contract_id))
        {
            Some(c) => c,
            None => panic_with_error!(&env, EscrowError::ContractNotFound),
        }
    }

    /// Return `true` if all deployment and verification checklist items are
    /// satisfied, meaning milestone releases are permitted.
    ///
    /// Items checked: `contract_created`, `funds_deposited`,
    /// `parties_authenticated`, `milestones_defined`.
    ///
    /// # Arguments
    /// * `contract_id` -- ID returned by `create_contract`.
    ///
    /// # Panics
    /// Panics with `ContractNotFound` if `contract_id` does not exist.
    pub fn is_release_ready(env: Env, contract_id: u32) -> bool {
        let checklist: ReleaseChecklist = match env
            .storage()
            .persistent()
            .get(&DataKey::Checklist(contract_id))
        {
            Some(c) => c,
            None => panic_with_error!(&env, EscrowError::ContractNotFound),
        };

        checklist.contract_created
            && checklist.funds_deposited
            && checklist.parties_authenticated
            && checklist.milestones_defined
    }

    /// Return `true` if all six checklist items are satisfied, indicating the
    /// full deployment lifecycle has been completed and monitored.
    ///
    /// # Arguments
    /// * `contract_id` -- ID returned by `create_contract`.
    ///
    /// # Panics
    /// Panics with `ContractNotFound` if `contract_id` does not exist.
    pub fn is_post_deploy_complete(env: Env, contract_id: u32) -> bool {
        let checklist: ReleaseChecklist = match env
            .storage()
            .persistent()
            .get(&DataKey::Checklist(contract_id))
        {
            Some(c) => c,
            None => panic_with_error!(&env, EscrowError::ContractNotFound),
        };

        checklist.contract_created
            && checklist.funds_deposited
            && checklist.parties_authenticated
            && checklist.milestones_defined
            && checklist.all_milestones_released
            && checklist.reputation_issued
    }

    // -----------------------------------------------------------------------
    // Utility
    // -----------------------------------------------------------------------

    /// Echo function retained for CI smoke-testing and SDK version validation.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }
}

#[cfg(test)]
mod test;
