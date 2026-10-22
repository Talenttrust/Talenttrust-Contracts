//! TalentTrust escrow contract for milestone-based freelancer payments on Stellar (Soroban).
//!
//! Lifecycle:
//! 1. `initialize(admin)` once after deployment.
//! 2. A client calls `create_contract(client, freelancer, token, milestone_amounts, review_window_secs)`.
//! 3. The client funds the escrow with `deposit_funds` (real token transfer into the contract).
//! 4. The freelancer signals completion with `submit_milestone`; the client approves with
//!    `release_milestone`, or the freelancer auto-claims after the review window via `claim_milestone`.
//! 5. Either party may `open_dispute`; the admin settles with `resolve_dispute`.
//! 6. The client can `cancel_contract` before completion. Only funds backing *Pending* milestones
//!    are refunded; any funds backing *Submitted* milestones remain claimable by the freelancer.
//! 7. After completion the client may `issue_reputation` for the freelancer.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol,
    Vec,
};

pub const MAX_MILESTONES: u32 = 20;
pub const BPS_DENOMINATOR: i128 = 10_000;
/// Threshold for dispute outcome accounting (5,000 basis points = 50%).
pub const DISPUTE_WIN_THRESHOLD_BPS: u32 = 5_000;
/// Minimum review window: 1 hour in seconds.
pub const MIN_REVIEW_WINDOW_SECS: u64 = 3_600;
/// Maximum review window: 30 days in seconds.
pub const MAX_REVIEW_WINDOW_SECS: u64 = 30 * 24 * 3_600;

const PERSISTENT_TTL_THRESHOLD: u32 = 17_280; // ~1 day
const PERSISTENT_TTL_EXTEND: u32 = 518_400; // ~30 days

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    Paused = 4,
    ContractNotFound = 5,
    InvalidParties = 6,
    InvalidMilestones = 7,
    InvalidAmount = 8,
    OverFunded = 9,
    InvalidStatus = 10,
    MilestoneNotFound = 11,
    MilestoneAlreadyReleased = 12,
    InsufficientFunds = 13,
    InvalidSplit = 14,
    InvalidRating = 15,
    ReputationAlreadyIssued = 16,
    ArithmeticOverflow = 17,
    RatingNotFound = 18,
    RatingAlreadyVoided = 19,
    InvalidReviewWindow = 20,
    MilestoneNotSubmitted = 21,
    ReviewWindowActive = 22,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created,
    Funded,
    Completed,
    Disputed,
    Resolved,
    Cancelled,
}

/// Per-milestone lifecycle state.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    /// Work has not started / not submitted.
    Pending,
    /// Freelancer submitted; awaiting client approval or review-window expiry.
    Submitted,
    /// Funds have been transferred to the freelancer.
    Released,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    /// Legacy field kept for ABI compatibility: true iff status == Released.
    pub released: bool,
    pub status: MilestoneStatus,
    /// Ledger timestamp at submission time; 0 when not yet submitted.
    pub submitted_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowContract {
    pub id: u64,
    pub client: Address,
    pub freelancer: Address,
    pub token: Address,
    pub milestones: Vec<Milestone>,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub status: ContractStatus,
    pub reputation_issued: bool,
    /// Seconds the client has to respond after a milestone is submitted before
    /// the freelancer can auto-claim. Range: [1 h, 30 d].
    pub review_window_secs: u64,
}

#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reputation {
    pub completed_contracts: u32,
    pub total_rating: u32,
}

#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Outcomes {
    pub disputes_opened: u32,
    pub disputes_won: u32,
    pub disputes_lost: u32,
    pub cancellations: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatingRecord {
    pub freelancer: Address,
    pub rating: u32,
    pub issued_at: u64,
    pub voided: bool,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Paused,
    NextId,
    Contract(u64),
    Reputation(Address),
    Outcomes(Address),
    Rating(u64),
}

#[contract]
pub struct Escrow;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn admin(env: &Env) -> Result<Address, EscrowError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(EscrowError::NotInitialized)
}

fn require_admin(env: &Env) -> Result<Address, EscrowError> {
    let a = admin(env)?;
    a.require_auth();
    Ok(a)
}

fn require_not_paused(env: &Env) -> Result<(), EscrowError> {
    if env
        .storage()
        .instance()
        .get::<_, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(EscrowError::Paused);
    }
    Ok(())
}

fn load(env: &Env, id: u64) -> Result<EscrowContract, EscrowError> {
    env.storage()
        .persistent()
        .get(&DataKey::Contract(id))
        .ok_or(EscrowError::ContractNotFound)
}

fn save(env: &Env, c: &EscrowContract) {
    let key = DataKey::Contract(c.id);
    env.storage().persistent().set(&key, c);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn transfer(env: &Env, token_addr: &Address, from: &Address, to: &Address, amount: i128) {
    if amount > 0 {
        token::Client::new(env, token_addr).transfer(from, to, &amount);
    }
}

/// Sum of amounts for all milestones in Submitted state.
/// These funds are committed to the freelancer and must never be returned to the client.
fn submitted_backing(milestones: &Vec<Milestone>) -> i128 {
    let mut total: i128 = 0;
    for m in milestones.iter() {
        if m.status == MilestoneStatus::Submitted {
            // Arithmetic overflow is impossible here: total_amount ≤ i128::MAX enforced on create.
            total += m.amount;
        }
    }
    total
}

// ---------------------------------------------------------------------------
// Contract implementation
// ---------------------------------------------------------------------------

#[contractimpl]
impl Escrow {
    /// One-time setup. Sets the admin that controls pausing and dispute resolution.
    pub fn initialize(env: Env, admin: Address) -> Result<(), EscrowError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(EscrowError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::NextId, &1u64);
        bump_instance(&env);
        env.events().publish((symbol_short!("init"),), admin);
        Ok(())
    }

    pub fn version(_env: Env) -> u32 {
        2
    }

    pub fn get_admin(env: Env) -> Result<Address, EscrowError> {
        admin(&env)
    }

    pub fn set_paused(env: Env, paused: bool) -> Result<(), EscrowError> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &paused);
        bump_instance(&env);
        env.events().publish((symbol_short!("paused"),), paused);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Creates a new escrow agreement. Returns the escrow id.
    ///
    /// `review_window_secs` must be in [3_600, 2_592_000] (1 hour to 30 days).
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        token: Address,
        milestone_amounts: Vec<i128>,
        review_window_secs: u64,
    ) -> Result<u64, EscrowError> {
        admin(&env)?;
        require_not_paused(&env)?;
        client.require_auth();
        if client == freelancer {
            return Err(EscrowError::InvalidParties);
        }
        if review_window_secs < MIN_REVIEW_WINDOW_SECS
            || review_window_secs > MAX_REVIEW_WINDOW_SECS
        {
            return Err(EscrowError::InvalidReviewWindow);
        }
        let n = milestone_amounts.len();
        if n == 0 || n > MAX_MILESTONES {
            return Err(EscrowError::InvalidMilestones);
        }
        let mut milestones = Vec::new(&env);
        let mut total: i128 = 0;
        for amount in milestone_amounts.iter() {
            if amount <= 0 {
                return Err(EscrowError::InvalidAmount);
            }
            total = total.checked_add(amount).ok_or(EscrowError::InvalidAmount)?;
            milestones.push_back(Milestone {
                amount,
                released: false,
                status: MilestoneStatus::Pending,
                submitted_at: 0,
            });
        }
        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));
        bump_instance(&env);
        let c = EscrowContract {
            id,
            client: client.clone(),
            freelancer: freelancer.clone(),
            token,
            milestones,
            total_amount: total,
            funded_amount: 0,
            released_amount: 0,
            status: ContractStatus::Created,
            reputation_issued: false,
            review_window_secs,
        };
        save(&env, &c);
        env.events()
            .publish((symbol_short!("created"), id), (client, freelancer, total));
        Ok(id)
    }

    /// Client deposits `amount` of the escrow token into the contract.
    pub fn deposit_funds(env: Env, id: u64, amount: i128) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }
        let new_funded = c
            .funded_amount
            .checked_add(amount)
            .ok_or(EscrowError::InvalidAmount)?;
        if new_funded > c.total_amount {
            return Err(EscrowError::OverFunded);
        }
        transfer(
            &env,
            &c.token,
            &c.client,
            &env.current_contract_address(),
            amount,
        );
        c.funded_amount = new_funded;
        if c.funded_amount == c.total_amount {
            c.status = ContractStatus::Funded;
        }
        save(&env, &c);
        env.events().publish((symbol_short!("deposit"), id), amount);
        Ok(())
    }

    /// Freelancer marks a milestone as delivered, starting the review clock.
    ///
    /// Requirements:
    /// - Caller must be the freelancer.
    /// - Escrow must be Funded.
    /// - Milestone must be Pending.
    /// - The milestone amount must be covered by the available (not yet committed) balance.
    pub fn submit_milestone(env: Env, id: u64, index: u32) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.freelancer.require_auth();
        if c.status != ContractStatus::Funded {
            return Err(EscrowError::InvalidStatus);
        }
        let mut m = c
            .milestones
            .get(index)
            .ok_or(EscrowError::MilestoneNotFound)?;
        if m.status != MilestoneStatus::Pending {
            // Already submitted or released.
            return Err(EscrowError::MilestoneAlreadyReleased);
        }
        // Available = funded - released - already_submitted
        let committed = submitted_backing(&c.milestones);
        let available = c
            .funded_amount
            .checked_sub(c.released_amount)
            .and_then(|v| v.checked_sub(committed))
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if available < m.amount {
            return Err(EscrowError::InsufficientFunds);
        }
        let now = env.ledger().timestamp();
        m.status = MilestoneStatus::Submitted;
        m.submitted_at = now;
        c.milestones.set(index, m);
        save(&env, &c);
        env.events()
            .publish((symbol_short!("submit"), id), (index, now));
        Ok(())
    }

    /// Client approves a milestone and the freelancer is paid immediately.
    ///
    /// Accepts both Pending (early pay) and Submitted milestones.
    pub fn release_milestone(env: Env, id: u64, index: u32) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        let mut m = c
            .milestones
            .get(index)
            .ok_or(EscrowError::MilestoneNotFound)?;
        if m.status == MilestoneStatus::Released {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }
        if c.funded_amount - c.released_amount < m.amount {
            return Err(EscrowError::InsufficientFunds);
        }
        transfer(
            &env,
            &c.token,
            &env.current_contract_address(),
            &c.freelancer,
            m.amount,
        );
        m.released = true;
        m.status = MilestoneStatus::Released;
        c.released_amount += m.amount;
        c.milestones.set(index, m.clone());
        if c.milestones.iter().all(|x| x.status == MilestoneStatus::Released) {
            c.status = ContractStatus::Completed;
        }
        save(&env, &c);
        env.events()
            .publish((symbol_short!("release"), id), (index, m.amount));
        Ok(())
    }

    /// Freelancer claims a submitted milestone after the review window has elapsed.
    ///
    /// Requirements:
    /// - Caller must be the freelancer.
    /// - Milestone must be Submitted.
    /// - `now >= submitted_at + review_window_secs` (checked arithmetic on the addition).
    /// - Escrow must not be Disputed.
    pub fn claim_milestone(env: Env, id: u64, index: u32) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.freelancer.require_auth();
        // Disputed escrows freeze claim_milestone.
        if c.status == ContractStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        // Valid statuses: Funded or Cancelled (Cancelled still holds submitted-milestone funds).
        // Resolved / Completed are also possible edges (claim after resolve), but in those cases
        // the milestone would already be Released, so the milestone-status check below
        // catches it cleanly.  InvalidStatus is the right error for statuses where the
        // contract is definitively over and funds are already disbursed (Resolved / Completed).
        if !matches!(
            c.status,
            ContractStatus::Funded | ContractStatus::Cancelled
        ) {
            return Err(EscrowError::InvalidStatus);
        }
        let mut m = c
            .milestones
            .get(index)
            .ok_or(EscrowError::MilestoneNotFound)?;
        if m.status != MilestoneStatus::Submitted {
            return Err(EscrowError::MilestoneNotSubmitted);
        }
        // Checked add to guard against u64 overflow at timestamp limits.
        let release_after = m
            .submitted_at
            .checked_add(c.review_window_secs)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        let now = env.ledger().timestamp();
        if now < release_after {
            return Err(EscrowError::ReviewWindowActive);
        }
        if c.funded_amount - c.released_amount < m.amount {
            return Err(EscrowError::InsufficientFunds);
        }
        transfer(
            &env,
            &c.token,
            &env.current_contract_address(),
            &c.freelancer,
            m.amount,
        );
        m.released = true;
        m.status = MilestoneStatus::Released;
        c.released_amount += m.amount;
        c.milestones.set(index, m.clone());
        // Only transition to Completed when the contract was Funded AND all milestones are released.
        if c.status == ContractStatus::Funded
            && c.milestones.iter().all(|x| x.status == MilestoneStatus::Released)
        {
            c.status = ContractStatus::Completed;
        }
        save(&env, &c);
        env.events()
            .publish((symbol_short!("claim"), id), (index, m.amount));
        Ok(())
    }

    /// Either party may raise a dispute, freezing releases until the admin resolves it.
    pub fn open_dispute(env: Env, id: u64, caller: Address) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        caller.require_auth();
        if caller != c.client && caller != c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        c.status = ContractStatus::Disputed;
        save(&env, &c);

        let outcome_key = DataKey::Outcomes(caller.clone());
        let mut caller_outcomes: Outcomes = env
            .storage()
            .persistent()
            .get(&outcome_key)
            .unwrap_or_default();
        caller_outcomes.disputes_opened = caller_outcomes
            .disputes_opened
            .checked_add(1)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage()
            .persistent()
            .set(&outcome_key, &caller_outcomes);
        env.storage().persistent().extend_ttl(
            &outcome_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );

        env.events().publish((symbol_short!("dispute"), id), caller);
        Ok(())
    }

    /// Admin resolves a dispute, paying `freelancer_bps` (0..=10000) of the escrowed
    /// balance to the freelancer and refunding the rest to the client.
    pub fn resolve_dispute(env: Env, id: u64, freelancer_bps: u32) -> Result<(), EscrowError> {
        require_admin(&env)?;
        let mut c = load(&env, id)?;
        if c.status != ContractStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        if freelancer_bps as i128 > BPS_DENOMINATOR {
            return Err(EscrowError::InvalidSplit);
        }
        let held = c.funded_amount - c.released_amount;
        let to_freelancer = held * freelancer_bps as i128 / BPS_DENOMINATOR;
        let to_client = held - to_freelancer;
        let this = env.current_contract_address();
        transfer(&env, &c.token, &this, &c.freelancer, to_freelancer);
        transfer(&env, &c.token, &this, &c.client, to_client);
        c.released_amount += to_freelancer;
        c.funded_amount = c.released_amount;
        c.status = ContractStatus::Resolved;
        save(&env, &c);

        let freelancer_key = DataKey::Outcomes(c.freelancer.clone());
        let mut freelancer_outcomes: Outcomes = env
            .storage()
            .persistent()
            .get(&freelancer_key)
            .unwrap_or_default();

        let client_key = DataKey::Outcomes(c.client.clone());
        let mut client_outcomes: Outcomes = env
            .storage()
            .persistent()
            .get(&client_key)
            .unwrap_or_default();

        if freelancer_bps >= DISPUTE_WIN_THRESHOLD_BPS {
            freelancer_outcomes.disputes_won = freelancer_outcomes
                .disputes_won
                .checked_add(1)
                .ok_or(EscrowError::ArithmeticOverflow)?;
            client_outcomes.disputes_lost = client_outcomes
                .disputes_lost
                .checked_add(1)
                .ok_or(EscrowError::ArithmeticOverflow)?;
        } else {
            freelancer_outcomes.disputes_lost = freelancer_outcomes
                .disputes_lost
                .checked_add(1)
                .ok_or(EscrowError::ArithmeticOverflow)?;
            client_outcomes.disputes_won = client_outcomes
                .disputes_won
                .checked_add(1)
                .ok_or(EscrowError::ArithmeticOverflow)?;
        }

        env.storage()
            .persistent()
            .set(&freelancer_key, &freelancer_outcomes);
        env.storage().persistent().extend_ttl(
            &freelancer_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );

        env.storage()
            .persistent()
            .set(&client_key, &client_outcomes);
        env.storage().persistent().extend_ttl(
            &client_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );

        env.events()
            .publish((symbol_short!("resolved"), id), (to_freelancer, to_client));
        Ok(())
    }

    /// Client cancels the agreement.
    ///
    /// Only funds backing **Pending** milestones are refunded. Funds backing **Submitted**
    /// milestones remain in the contract, claimable by the freelancer once the review window
    /// elapses (or disputable). If there are no submitted milestones the behaviour is identical
    /// to the previous implementation.
    pub fn cancel_contract(env: Env, id: u64) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        // Total held balance minus the portion committed to submitted milestones.
        let held = c.funded_amount - c.released_amount;
        let submitted = submitted_backing(&c.milestones);
        let refund = held
            .checked_sub(submitted)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        transfer(
            &env,
            &c.token,
            &env.current_contract_address(),
            &c.client,
            refund,
        );
        c.funded_amount = c
            .funded_amount
            .checked_sub(refund)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        c.status = ContractStatus::Cancelled;
        save(&env, &c);

        if refund > 0 {
            let client_key = DataKey::Outcomes(c.client.clone());
            let mut client_outcomes: Outcomes = env
                .storage()
                .persistent()
                .get(&client_key)
                .unwrap_or_default();
            client_outcomes.cancellations = client_outcomes
                .cancellations
                .checked_add(1)
                .ok_or(EscrowError::ArithmeticOverflow)?;
            env.storage()
                .persistent()
                .set(&client_key, &client_outcomes);
            env.storage().persistent().extend_ttl(
                &client_key,
                PERSISTENT_TTL_THRESHOLD,
                PERSISTENT_TTL_EXTEND,
            );
        }

        env.events().publish((symbol_short!("cancel"), id), refund);
        Ok(())
    }

    /// After completion or dispute resolution the client may rate the freelancer (1..=5) once.
    pub fn issue_reputation(env: Env, id: u64, rating: u32) -> Result<(), EscrowError> {
        let mut c = load(&env, id)?;
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Completed | ContractStatus::Resolved) {
            return Err(EscrowError::InvalidStatus);
        }
        if c.reputation_issued {
            return Err(EscrowError::ReputationAlreadyIssued);
        }
        if !(1..=5).contains(&rating) {
            return Err(EscrowError::InvalidRating);
        }
        let key = DataKey::Reputation(c.freelancer.clone());
        let mut rep: Reputation = env.storage().persistent().get(&key).unwrap_or_default();
        rep.completed_contracts += 1;
        rep.total_rating += rating;
        env.storage().persistent().set(&key, &rep);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);

        let issued_at = env.ledger().timestamp();
        let rating_record = RatingRecord {
            freelancer: c.freelancer.clone(),
            rating,
            issued_at,
            voided: false,
        };
        let rating_key = DataKey::Rating(id);
        env.storage().persistent().set(&rating_key, &rating_record);
        env.storage()
            .persistent()
            .extend_ttl(&rating_key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);

        c.reputation_issued = true;
        save(&env, &c);
        env.events()
            .publish((symbol_short!("rep"), id), (c.freelancer, rating));
        Ok(())
    }

    /// Governance can void fraudulent ratings, reversing their effect and recording an audit trail.
    pub fn void_rating(env: Env, id: u64, reason: Symbol) -> Result<(), EscrowError> {
        require_admin(&env)?;
        require_not_paused(&env)?;

        let rating_key = DataKey::Rating(id);
        let mut record: RatingRecord = env
            .storage()
            .persistent()
            .get(&rating_key)
            .ok_or(EscrowError::RatingNotFound)?;

        if record.voided {
            return Err(EscrowError::RatingAlreadyVoided);
        }

        let rep_key = DataKey::Reputation(record.freelancer.clone());
        let mut rep: Reputation = env.storage().persistent().get(&rep_key).unwrap_or_default();
        rep.total_rating = rep.total_rating.saturating_sub(record.rating);
        rep.completed_contracts = rep.completed_contracts.saturating_sub(1);
        env.storage().persistent().set(&rep_key, &rep);
        env.storage()
            .persistent()
            .extend_ttl(&rep_key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);

        record.voided = true;
        env.storage().persistent().set(&rating_key, &record);
        env.storage()
            .persistent()
            .extend_ttl(&rating_key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);

        env.events().publish(
            (symbol_short!("rep_void"), id),
            (record.freelancer, record.rating, reason),
        );
        Ok(())
    }

    pub fn get_rating(env: Env, id: u64) -> Option<RatingRecord> {
        env.storage().persistent().get(&DataKey::Rating(id))
    }

    pub fn get_contract(env: Env, id: u64) -> Result<EscrowContract, EscrowError> {
        load(&env, id)
    }

    pub fn contract_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<_, u64>(&DataKey::NextId)
            .unwrap_or(1)
            - 1
    }

    pub fn get_reputation(env: Env, freelancer: Address) -> Reputation {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(freelancer))
            .unwrap_or_default()
    }

    pub fn get_outcomes(env: Env, addr: Address) -> Outcomes {
        env.storage()
            .persistent()
            .get(&DataKey::Outcomes(addr))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod test;
