/// TalentTrust escrow contract for milestone-based freelancer payments on Stellar (Soroban).
///
/// Lifecycle:
/// 1. `initialize(admin)` once after deployment.
/// 2. A client calls `create_contract(client, freelancer, token, milestone_amounts)` or `create_contract_with_deadlines`.
/// 3. The client funds the escrow with `deposit_funds` (real token transfer into the contract).
/// 4. The client approves work with `release_milestone`, paying the freelancer per milestone.
/// 5. Either party may `open_dispute`; the admin settles with `resolve_dispute`.
/// 6. The client can `cancel_contract` before completion to recover unreleased funds.
/// 7. The client can `reclaim_overdue` individual milestones whose deadline + grace has passed.
/// 8. After completion the client may `issue_reputation` for the freelancer.
#no_std!

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
};

pub const MAX_MILESTONES: u32 = 20;
pub const BPS_DENOMINATOR: i128 = 10_000;
pub const DEADLINE_GRACE_SECS: u64 = 3 * 86_400;
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
    // Append-only: new variants go after this line.
    InvalidDeadline = 17,
    NotOverdue = 18,
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
#[derive(Clone, Default, Eq, PartialEq)]
pub struct Reputation {
    pub completed_contracts: u32,
    pub total_rating: u32,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Paused,
    NextId,
    Contract(u64),
    Reputation(Address),
    Deadlines(u64),
    Reclaimed(u64),
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

fn load_deadlines(env: &Env, id: u64) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::Deadlines(id))
        .unwrap_or(Vec::new(env))
}

fn load_reclaimed(env: &Env, id: u64) -> Vec<u32> {
    env.storage()
        .persistent()
        .get(&DataKey::Reclaimed(id))
        .unwrap_or(Vec::new(env))
}

fn save_reclaimed(env: &Env, id: u64, reclaimed: &Vec<u32>) {
    let key = DataKey::Reclaimed(id);
    env.storage().persistent().set(&key, reclaimed);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn is_reclaimed(env: &Env, id: u64, index: u32) -> bool {
    load_reclaimed(env, id).iter().any(|i| i == index)
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
        Self::create_contract_internal(
            &env,
            client,
            freelancer,
            token,
            milestone_amounts,
            None,
        )
    }

    /// Creates a new escrow agreement with optional per-milestone deadlines.
    /// A deadline of 0 means no deadline. Non-zero deadlines must be in the
    /// future and non-decreasing.
    pub fn create_contract_with_deadlines(
        env: Env,
        client: Address,
        freelancer: Address,
        token: Address,
        milestone_amounts: Vec<i128>,
        deadlines: Vec<u64>,
    ) -> Result<u64, EscrowError> {
        Self::create_contract_internal(
            &env,
            client,
            freelancer,
            token,
            milestone_amounts,
            Some(deadlines),
        )
    }

    fn create_contract_internal(
        env: &Env,
        client: Address,
        freelancer: Address,
        token: Address,
        milestone_amounts: Vec<i128>,
        deadlines: Option<Vec<u64>>,
    ) -> Result<u64, EscrowError> {
        admin(env)?;
        require_not_paused(env)?;
        client.require_auth();
        if client == freelancer {
            return Err(EscrowError::InvalidParties);
        }
        let n = milestone_amounts.len();
        if n == 0 || n > MAX_MILESTONES {
            return Err(EscrowError::InvalidMilestones);
        }
        if let Some(ref d) = deadlines {
            if d.len() != n {
                return Err(EscrowError::InvalidDeadline);
            }
            let now = env.ledger().timestamp();
            let mut prev: u64 = 0;
            for dl in d.iter() {
                if dl != 0 {
                    if dl <= now {
                        return Err(EscrowError::InvalidDeadline);
                    }
                    if dl < prev {
                        return Err(EscrowError::InvalidDeadline);
                    }
                    prev = dl;
                }
            }
        }
        let mut milestones = Vec::new(env);
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
        bump_instance(env);
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
        save(env, &c);
        if let Some(d) = deadlines {
            let key = DataKey::Deadlines(id);
            env.storage().persistent().set(&key, &d);
            env.storage()
                .persistent()
                .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        }
        env.events()
            .publish((symbol_short!("created"), id), (client, freelancer, total));
        Ok(id)
    }

    /// Client deposits `amount` of the escrow token into the contract.
    pub fn deposit_funds(env: Env, id: u64, amount: i128) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.client.require_auth();
        if !matches(c.status, ContractStatus::Created | ContractStatus::Funded) {
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
        if !matches(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        if is_reclaimed(&env, id, index) {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }
        let mut m = c.milestones
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
        if all_settled(&env, &c, id) {
            c.status = ContractStatus::Completed;
        }
        save(&env, &c);
        env.events()
            .publish((symbol_short!("release"), id), (index, m.amount));
        Ok(())
    }

    /// Client reclaims the funds of a single overdue milestone without cancelling
    /// the rest of the contract. Requires `now >= deadline + grace`.
    pub fn reclaim_overdue(env: Env, id: u64, index: u32) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.client.require_auth();
        if !matches(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        if is_reclaimed(&env, id, index) {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }
        let mut m = c.milestones
            .get(index)
            .ok_or(EscrowError::MilestoneNotFound)?;
        if m.released {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }
        let deadlines = load_deadlines(&env, id);
        let deadline = deadlines
            .get(index)
            .ok_or(EscrowError::InvalidDeadline)?;
        if deadline == 0 {
            return Err(EscrowError::InvalidDeadline);
        }
        let overdue_at = deadline
            .checked_add(DEADLINE_GRACE_SECS)
            .ok_or(EscrowError::InvalidDeadline)?;
        if env.ledger().timestamp() < overdue_at {
            return Err(EscrowError::NotOverdue);
        }
        let held = c.funded_amount - c.released_amount;
        let refund = if m.amount < held { m.amount } else { held };
        transfer(
            &env,
            &c.token,
            &env.current_contract_address(),
            &c.client,
            refund,
        );
        m.released = true;
        c.milestones.set(index, m.clone());
        c.funded_amount -= refund;
        c.total_amount -= refund;
        let mut reclaimed = load_reclaimed(&env, id);
        reclaimed.push_back(index);
        save_reclaimed(&env, id, &reclaimed);
        if all_settled(&env, &c, id) {
            c.status = ContractStatus::Completed;
        } else if c.released_amount == 0 {
            let any_open = c.milestones.iter().any((x, idx) {
                !is_reclaimed(&env, id, idx) && !x.released
            });
            if !any_open {
                c.status = ContractStatus::Cancelled;
            }
        }
        save(&env, &c);
        env.events()
            .publish((symbol_short!("overdue"), id), (index, refund));
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
        if !matches(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        c.status = ContractStatus::Disputed;
        save(&env, &c);
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
        env.events()
            .publish((symbol_short!("resolved"), id), (to_freelancer, to_client));
        Ok(())
    }

    /// Client cancels the agreement and recovers any unreleased funds.
    pub fn cancel_contract(env: Env, id: u64) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.client.require_auth();
        if !matches(c.status, ContractStatus::Created | ContractStatus::Funded) {
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
        env.events().publish((symbol_short!("cancel"), id), refund);
        Ok(())
    }

    /// Read the per-milestone deadlines (0 = no deadline).
    pub fn get_deadlines(env: Env, id: u64) -> Vec<u64> {
        load_deadlines(&env, id)
    }

    /// After completion the client may rate the freelancer (1-5), once.
    pub fn issue_reputation(env Env, id: u64, rating: u32) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        c.client.require_auth();
        if c.status != ContractStatus::Completed {
            return Err(EscrowError::InvalidStatus);
        }
        if c.reputation_issued {
            return Err(EscrowError::ReputationAlreadyIssued);
        }
        if rating < 1 || rating > 5 {
            return Err(EscrowError::InvalidRating);
        }
        let key = DataKey::Reputation(c.freelancer.clone());
        let mut rep: Reputation = env.storage().persistent().get(&key).unwrap_or(Reputation::default());
        rep.completed_contracts += 1;
        rep.total_rating += rating;
        env.storage().persistent().set(&key, &rep);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        c.reputation_issued = true;
        save(&env, &c);
        env.events()
            .publish((symbol_short!("repute"), id), rating);
        Ok(())
    }

    pub fn get_contract(env: Env, id: u64) -> Result<EscrowContract, EscrowError> {
        load(&env, id)
    }

    pub fn contract_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(1)
            - 1
    }

    pub fn get_reputation(env: Env, addr: Address) -> Reputation {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(addr))
            .unwrap_or(Reputation::default())
    }
}

fn all_settled(env: &Env, c: &EscrowContract, id: u64) -> bool {
    c.milestones.iter().all((x, idx) {
        x.released || is_reclaimed(env, id, idx)
    })
}
