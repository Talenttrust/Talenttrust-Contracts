//! TalentTrust escrow contract for milestone-based freelancer payments on Stellar (Soroban).
//!
//! Lifecycle:
//! 1. `initialize(admin)` once after deployment.
//! 2. A client calls `create_contract(client, freelancer, token, milestone_amounts)`.
//! 3. The client funds the escrow with `deposit_funds` (real token transfer into the contract).
//! 4. The client approves work with `release_milestone`, paying the freelancer per milestone.
//! 5. Either party may `open_dispute`; the admin resolves it at any time, or either party
//!    triggers a 50/50 fallback after `DISPUTE_TIMEOUT_SECS`.
//! 6. The client can `cancel_contract` before completion to recover unreleased funds.
//! 7. After completion the client may `issue_reputation` for the freelancer.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol,
    Vec,
};

pub const MAX_MILESTONES: u32 = 20;
pub const BPS_DENOMINATOR: i128 = 10_000;
pub const DISPUTE_TIMEOUT_SECS: u64 = 14 * 86_400;
/// Threshold for dispute outcome accounting (5,000 basis points, or 50%).
/// In dispute resolution, if freelancer_bps >= DISPUTE_WIN_THRESHOLD_BPS,
/// the outcome is counted as a win for the freelancer and a loss for the client; otherwise, the reverse.
pub const DISPUTE_WIN_THRESHOLD_BPS: u32 = 5_000;
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
    DisputeNotExpired = 20,
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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
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
    DisputedAt(u64),
    Reputation(Address),
    Outcomes(Address),
    Rating(u64),
}

#[contract]
pub struct Escrow;

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

fn settle_dispute(
    env: &Env,
    c: &mut EscrowContract,
    freelancer_bps: u32,
    event: soroban_sdk::Symbol,
) -> Result<(), EscrowError> {
    if freelancer_bps as i128 > BPS_DENOMINATOR {
        return Err(EscrowError::InvalidSplit);
    }
    let held = c.funded_amount - c.released_amount;
    let to_freelancer = held * freelancer_bps as i128 / BPS_DENOMINATOR;
    let to_client = held - to_freelancer;
    let this = env.current_contract_address();
    transfer(env, &c.token, &this, &c.freelancer, to_freelancer);
    transfer(env, &c.token, &this, &c.client, to_client);
    c.released_amount += to_freelancer;
    c.funded_amount = c.released_amount;
    c.status = ContractStatus::Resolved;
    save(env, c);

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
        .publish((event, c.id), (to_freelancer, to_client));
    Ok(())
}

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
        1
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
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        token: Address,
        milestone_amounts: Vec<i128>,
    ) -> Result<u64, EscrowError> {
        admin(&env)?;
        require_not_paused(&env)?;
        client.require_auth();
        if client == freelancer {
            return Err(EscrowError::InvalidParties);
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

    /// Client approves a milestone and the freelancer is paid.
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
        if m.released {
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
        c.released_amount += m.amount;
        c.milestones.set(index, m.clone());
        if c.milestones.iter().all(|x| x.released) {
            c.status = ContractStatus::Completed;
        }
        save(&env, &c);
        env.events()
            .publish((symbol_short!("release"), id), (index, m.amount));
        Ok(())
    }

    /// Either party may raise a dispute, freezing releases until settlement.
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
        env.storage()
            .persistent()
            .set(&DataKey::DisputedAt(id), &env.ledger().timestamp());
        env.storage().persistent().extend_ttl(
            &DataKey::DisputedAt(id),
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );
        save(&env, &c);

        // Increment disputes_opened for the caller using checked arithmetic
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
    ///
    /// Outcomes:
    /// If freelancer_bps >= DISPUTE_WIN_THRESHOLD_BPS (5_000 bps, or 50%),
    /// freelancer gets a dispute win and client gets a dispute loss.
    /// Otherwise, freelancer gets a loss and client gets a win.
    pub fn resolve_dispute(env: Env, id: u64, freelancer_bps: u32) -> Result<(), EscrowError> {
        require_admin(&env)?;
        let mut c = load(&env, id)?;
        if c.status != ContractStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        settle_dispute(&env, &mut c, freelancer_bps, symbol_short!("resolved"))?;
        Ok(())
    }

    /// After 14 days either party may trigger an even split, with any remainder refunded to the client.
    pub fn settle_expired_dispute(env: Env, id: u64, caller: Address) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        caller.require_auth();
        if caller != c.client && caller != c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        if c.status != ContractStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        let disputed_at: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::DisputedAt(id))
            .ok_or(EscrowError::InvalidStatus)?;
        let deadline = disputed_at
            .checked_add(DISPUTE_TIMEOUT_SECS)
            .ok_or(EscrowError::DisputeNotExpired)?;
        if env.ledger().timestamp() < deadline {
            return Err(EscrowError::DisputeNotExpired);
        }
        settle_dispute(&env, &mut c, 5_000, symbol_short!("disp_exp"))?;
        Ok(())
    }

    /// Client cancels the agreement and recovers any unreleased funds.
    /// If any funds were held and refunded, cancellations is incremented for the client.
    pub fn cancel_contract(env: Env, id: u64) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        let refund = c.funded_amount - c.released_amount;
        transfer(
            &env,
            &c.token,
            &env.current_contract_address(),
            &c.client,
            refund,
        );
        c.funded_amount = c.released_amount;
        c.status = ContractStatus::Cancelled;
        save(&env, &c);

        // Increment cancellations for client only when funds were held
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

    /// After completion or dispute resolution the client may rate the freelancer (1..=5) once per contract.
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
