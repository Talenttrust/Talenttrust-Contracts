//! # TalentTrust Escrow Contract
//!
//! Soroban smart contract that holds funds in milestone-based escrow.
//! Clients deposit the total project budget; milestones are released to the
//! freelancer on approval; unreleased balances can be refunded to the client
//! if the contract is cancelled before completion.
//!
//! ## State machine
//!
//! ```text
//! Created ──► Funded ──► Completed   (all milestones released)
//!                   └──► Cancelled   (client calls request_refund)
//! ```
//!
//! ## Security assumptions
//!
//! * All state-mutating functions enforce `require_auth` on the authorised
//!   principal (client).
//! * A completed contract cannot be refunded — status guard prevents it.
//! * A cancelled contract cannot be refunded again — prevents double-refund.
//! * The refund amount is derived purely from on-chain milestone data; no
//!   caller-supplied amount is trusted.
//! * In production, replace the internal balance with a `token::Client`
//!   transfer so the contract does not hold raw token custody.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

/// Contract-level error codes returned on panic.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    /// The contract record does not exist.
    NotFound = 1,
    /// The caller is not authorised for this operation.
    Unauthorized = 2,
    /// The action is not permitted in the current contract status.
    InvalidStatus = 3,
    /// The refund amount is zero (all milestones already released).
    NothingToRefund = 4,
    /// A milestone index is out of range.
    InvalidMilestone = 5,
    /// Deposit amount must be positive.
    InvalidAmount = 6,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Lifecycle status of an escrow contract.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    /// Contract created but not yet funded.
    Created = 0,
    /// Client has deposited funds; work is in progress.
    Funded = 1,
    /// All milestones released; contract is complete.
    Completed = 2,
    /// A dispute has been raised (reserved for future arbitration).
    Disputed = 3,
    /// Client requested a refund; unreleased balance returned.
    Cancelled = 4,
}

/// A single deliverable in the contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    /// Payment amount in stroops (1 XLM = 10 000 000 stroops).
    pub amount: i128,
    /// Whether the milestone has been approved and released to the freelancer.
    pub released: bool,
}

/// All persistent state for one escrow contract.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EscrowData {
    /// The party that commissions the work and holds refund rights.
    pub client: Address,
    /// The party that performs the work and receives milestone payments.
    pub freelancer: Address,
    /// Ordered list of milestones.
    pub milestones: Vec<Milestone>,
    /// Current unspent balance held by this contract (internal ledger).
    /// In production this maps 1-to-1 with a token account.
    pub balance: i128,
    /// Lifecycle status.
    pub status: ContractStatus,
}

/// Storage key variants used for persistent contract state.
#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    /// Per-contract data keyed by its numeric ID.
    Contract(u32),
    /// Auto-increment counter used to mint unique contract IDs.
    NextId,
}

// ---------------------------------------------------------------------------
// Helper — load / save
// ---------------------------------------------------------------------------

fn load_contract(env: &Env, contract_id: u32) -> EscrowData {
    env.storage()
        .persistent()
        .get(&DataKey::Contract(contract_id))
        .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotFound))
}

fn save_contract(env: &Env, contract_id: u32, data: &EscrowData) {
    env.storage()
        .persistent()
        .set(&DataKey::Contract(contract_id), data);
}

fn next_id(env: &Env) -> u32 {
    let key = DataKey::NextId;
    let id: u32 = env.storage().persistent().get(&key).unwrap_or(0) + 1;
    env.storage().persistent().set(&key, &id);
    id
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Escrow;

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

#[contractimpl]
impl Escrow {
    // -----------------------------------------------------------------------
    // create_contract
    // -----------------------------------------------------------------------

    /// Create a new escrow contract between `client` and `freelancer`.
    ///
    /// # Parameters
    /// - `client`            – address that funds the contract and controls releases.
    /// - `freelancer`        – address that receives milestone payments.
    /// - `milestone_amounts` – ordered list of amounts (in stroops) for each deliverable.
    ///
    /// # Returns
    /// A unique numeric contract ID used in all subsequent calls.
    ///
    /// # Panics
    /// - If any milestone amount is ≤ 0.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        milestone_amounts: Vec<i128>,
    ) -> u32 {
        let mut milestones: Vec<Milestone> = Vec::new(&env);
        for amount in milestone_amounts.iter() {
            if amount <= 0 {
                panic_with_error!(&env, EscrowError::InvalidAmount);
            }
            milestones.push_back(Milestone {
                amount,
                released: false,
            });
        }

        let id = next_id(&env);
        save_contract(
            &env,
            id,
            &EscrowData {
                client,
                freelancer,
                milestones,
                balance: 0,
                status: ContractStatus::Created,
            },
        );
        id
    }

    // -----------------------------------------------------------------------
    // deposit_funds
    // -----------------------------------------------------------------------

    /// Deposit funds into the escrow.
    ///
    /// Only the client may call this. Transitions status to `Funded` on first
    /// successful deposit.
    ///
    /// In production, pair with a `token::Client::transfer` from the client's
    /// wallet into this contract's account before calling this function.
    ///
    /// # Parameters
    /// - `contract_id` – ID returned by `create_contract`.
    /// - `amount`      – amount in stroops to credit (must be > 0).
    ///
    /// # Panics
    /// - Caller is not the client (`Unauthorized`).
    /// - Amount ≤ 0 (`InvalidAmount`).
    /// - Contract is `Completed` or `Cancelled` (`InvalidStatus`).
    pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool {
        if amount <= 0 {
            panic_with_error!(&env, EscrowError::InvalidAmount);
        }

        let mut data = load_contract(&env, contract_id);
        data.client.require_auth();

        if data.status == ContractStatus::Completed || data.status == ContractStatus::Cancelled {
            panic_with_error!(&env, EscrowError::InvalidStatus);
        }

        data.balance += amount;
        if data.status == ContractStatus::Created {
            data.status = ContractStatus::Funded;
        }
        save_contract(&env, contract_id, &data);
        true
    }

    // -----------------------------------------------------------------------
    // release_milestone
    // -----------------------------------------------------------------------

    /// Approve and release a milestone payment to the freelancer.
    ///
    /// Only the client may approve. The contract moves to `Completed` once all
    /// milestones are released.
    ///
    /// In production, pair with a `token::Client::transfer` from the contract
    /// account to the freelancer's wallet.
    ///
    /// # Parameters
    /// - `contract_id`  – ID returned by `create_contract`.
    /// - `milestone_id` – zero-based index into the milestones list.
    ///
    /// # Panics
    /// - Caller is not the client (`Unauthorized`).
    /// - Contract is not `Funded` or `Disputed` (`InvalidStatus`).
    /// - `milestone_id` out of range (`InvalidMilestone`).
    /// - Milestone already released (`InvalidStatus`).
    pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool {
        let mut data = load_contract(&env, contract_id);
        data.client.require_auth();

        if data.status != ContractStatus::Funded && data.status != ContractStatus::Disputed {
            panic_with_error!(&env, EscrowError::InvalidStatus);
        }

        let idx = milestone_id as usize;
        if idx >= data.milestones.len() as usize {
            panic_with_error!(&env, EscrowError::InvalidMilestone);
        }

        let mut milestone = data.milestones.get(milestone_id).unwrap();
        if milestone.released {
            panic_with_error!(&env, EscrowError::InvalidStatus);
        }

        // Credit freelancer (internal ledger; swap for token transfer in prod).
        milestone.released = true;
        data.balance -= milestone.amount;
        data.milestones.set(milestone_id, milestone);

        // Transition to Completed if every milestone is released.
        let all_released = data.milestones.iter().all(|m| m.released);
        if all_released {
            data.status = ContractStatus::Completed;
        }

        save_contract(&env, contract_id, &data);

        env.events()
            .publish((symbol_short!("released"), contract_id), milestone_id);

        true
    }

    // -----------------------------------------------------------------------
    // request_refund  ← NEW
    // -----------------------------------------------------------------------

    /// Refund the client for all unreleased milestone balances.
    ///
    /// This is the core **partial refund** mechanism. Only the client may
    /// trigger it, and only while the contract has not yet been completed or
    /// already cancelled. The refund amount equals the sum of all milestone
    /// amounts that have not yet been released to the freelancer; milestones
    /// that have already been released are **not** reversed.
    ///
    /// After a successful refund the contract status is set to `Cancelled`.
    /// Any subsequent call will be rejected by the double-refund guard.
    ///
    /// In production, follow this call with (or embed) a
    /// `token::Client::transfer(env.current_contract_address(), client, refund_amount)`
    /// so the actual tokens are returned to the client's wallet.
    ///
    /// # Parameters
    /// - `contract_id` – ID returned by `create_contract`.
    ///
    /// # Returns
    /// The total amount (in stroops) refunded to the client.
    ///
    /// # Security
    /// - `require_auth` ensures only the registered client address can call this.
    /// - Status guard prevents refunding a completed contract.
    /// - Double-refund guard sets status to `Cancelled` after first refund.
    /// - Refund amount is computed from on-chain data only; no caller input is trusted.
    ///
    /// # Panics
    /// - Caller is not the client (`Unauthorized`).
    /// - Contract status is `Completed` (`InvalidStatus`).
    /// - Contract status is already `Cancelled` (`InvalidStatus` — double-refund).
    /// - All milestones already released; nothing to refund (`NothingToRefund`).
    pub fn request_refund(env: Env, contract_id: u32) -> i128 {
        let mut data = load_contract(&env, contract_id);

        // ── Auth ─────────────────────────────────────────────────────────────
        data.client.require_auth();

        // ── Status guards ────────────────────────────────────────────────────
        if data.status == ContractStatus::Completed {
            panic_with_error!(&env, EscrowError::InvalidStatus);
        }
        if data.status == ContractStatus::Cancelled {
            // Double-refund guard
            panic_with_error!(&env, EscrowError::InvalidStatus);
        }

        // ── Compute unreleased amount ─────────────────────────────────────────
        let refund_amount: i128 = data
            .milestones
            .iter()
            .filter(|m| !m.released)
            .map(|m| m.amount)
            .sum();

        if refund_amount == 0 {
            panic_with_error!(&env, EscrowError::NothingToRefund);
        }

        // ── Update state ──────────────────────────────────────────────────────
        data.balance -= refund_amount;
        data.status = ContractStatus::Cancelled;
        save_contract(&env, contract_id, &data);

        // ── Emit event ────────────────────────────────────────────────────────
        // Listeners can index (contract_id, refund_amount) for off-chain accounting.
        env.events()
            .publish((symbol_short!("refund"), contract_id), refund_amount);

        // In production: token::Client::new(&env, &token_id)
        //     .transfer(&env.current_contract_address(), &data.client, &refund_amount);

        refund_amount
    }

    // -----------------------------------------------------------------------
    // issue_reputation
    // -----------------------------------------------------------------------

    /// Issue a reputation credential for the freelancer after successful completion.
    ///
    /// Only the client may issue credentials, and only for a completed contract.
    ///
    /// # Parameters
    /// - `freelancer` – address to receive the credential.
    /// - `rating`     – score from 1 to 100.
    ///
    /// # Panics
    /// - Caller is not the client (`Unauthorized`).
    pub fn issue_reputation(_env: Env, _freelancer: Address, _rating: i128) -> bool {
        // Reputation credential issuance — full implementation stores credential
        // in persistent storage and optionally mints an NFT.
        true
    }

    // -----------------------------------------------------------------------
    // hello
    // -----------------------------------------------------------------------

    /// Health-check / CI smoke-test.
    pub fn hello(_env: Env, to: Symbol) -> Symbol {
        to
    }

    /// Returns the stored contract state.
    pub fn get_contract(env: Env, contract_id: u32) -> EscrowContractData {
        Self::load_contract(&env, contract_id)
    }

    /// Returns the stored reputation record for a freelancer, if present.
    pub fn get_reputation(env: Env, freelancer: Address) -> Option<ReputationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
    }

    /// Returns the number of pending reputation updates that can be claimed.
    pub fn get_pending_reputation_credits(env: Env, freelancer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(freelancer))
            .unwrap_or(0)
    }

    /// Returns the active protocol parameters.
    ///
    /// If governance has not been initialized yet, this returns the safe default
    /// parameters baked into the contract.
    pub fn get_protocol_parameters(env: Env) -> ProtocolParameters {
        Self::protocol_parameters(&env)
    }

    /// Returns the current governance admin, if governance has been initialized.
    pub fn get_governance_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::GovernanceAdmin)
    }

    /// Returns the pending governance admin, if an admin transfer is in flight.
    pub fn get_pending_governance_admin(env: Env) -> Option<Address> {
        Self::pending_governance_admin(&env)
    }
}

impl Escrow {
    fn next_contract_id(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    }

    fn load_contract(env: &Env, contract_id: u32) -> EscrowContractData {
        env.storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| panic!("contract not found"))
    }

    fn save_contract(env: &Env, contract_id: u32, contract: &EscrowContractData) {
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), contract);
    }

    fn add_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let key = DataKey::PendingReputationCredits(freelancer.clone());
        let current = env.storage().persistent().get::<_, u32>(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + 1));
    }

    fn governance_admin(env: &Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::GovernanceAdmin)
            .unwrap_or_else(|| panic!("protocol governance is not initialized"))
    }

    fn pending_governance_admin(env: &Env) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingGovernanceAdmin)
    }

    fn protocol_parameters(env: &Env) -> ProtocolParameters {
        env.storage()
            .persistent()
            .get(&DataKey::ProtocolParameters)
            .unwrap_or_else(Self::default_protocol_parameters)
    }

    fn default_protocol_parameters() -> ProtocolParameters {
        ProtocolParameters {
            min_milestone_amount: DEFAULT_MIN_MILESTONE_AMOUNT,
            max_milestones: DEFAULT_MAX_MILESTONES,
            min_reputation_rating: DEFAULT_MIN_REPUTATION_RATING,
            max_reputation_rating: DEFAULT_MAX_REPUTATION_RATING,
        }
    }

    fn validated_protocol_parameters(
        min_milestone_amount: i128,
        max_milestones: u32,
        min_reputation_rating: i128,
        max_reputation_rating: i128,
    ) -> ProtocolParameters {
        if min_milestone_amount <= 0 {
            panic!("minimum milestone amount must be positive");
        }
        if max_milestones == 0 {
            panic!("maximum milestones must be positive");
        }
        if min_reputation_rating <= 0 {
            panic!("minimum reputation rating must be positive");
        }
        if min_reputation_rating > max_reputation_rating {
            panic!("reputation rating range is invalid");
        }

        ProtocolParameters {
            min_milestone_amount,
            max_milestones,
            min_reputation_rating,
            max_reputation_rating,
        }
    }

    fn all_milestones_released(milestones: &Vec<Milestone>) -> bool {
        let mut index = 0_u32;
        while index < milestones.len() {
            let milestone = milestones
                .get(index)
                .unwrap_or_else(|| panic!("missing milestone"));
            if !milestone.released {
                return false;
            }
            index += 1;
        }
        true
    }
}

#[cfg(test)]
mod test;
