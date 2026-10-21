//! TalentTrust escrow contract for milestone-based freelancer payments on Stellar (Soroban).
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env,
    Symbol, Vec,
};

pub const MAX_MILESTONES: u32 = 20;
pub const BPS_DENOMINATOR: i128 = 10_000;
const PERSISTENT_TTL_THRESHOLD: u32 = 17_280;
const PERSISTENT_TTL_EXTEND: u32 = 518_400;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    AlreadyInitialized       = 1,
    NotInitialized           = 2,
    Unauthorized             = 3,
    Paused                   = 4,
    ContractNotFound         = 5,
    InvalidParties           = 6,
    InvalidMilestones        = 7,
    InvalidAmount            = 8,
    OverFunded               = 9,
    InvalidStatus            = 10,
    MilestoneNotFound        = 11,
    MilestoneAlreadyReleased = 12,
    InsufficientFunds        = 13,
    InvalidSplit             = 14,
    InvalidRating            = 15,
    ReputationAlreadyIssued  = 16,
    /// milestone_hashes.len() was neither 0 nor the milestone count.
    InvalidMetadata          = 17,
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
#[derive(Clone)]
enum DataKey {
    Admin,
    Paused,
    NextId,
    Contract(u64),
    Reputation(Address),
    Terms(u64),
    MilestoneSpecs(u64),
    Delivery(u64, u32),
}

#[contract]
pub struct Escrow;

fn admin(env: &Env) -> Result<Address, EscrowError> {
    env.storage().instance().get(&DataKey::Admin).ok_or(EscrowError::NotInitialized)
}
fn require_admin(env: &Env) -> Result<Address, EscrowError> {
    let a = admin(env)?; a.require_auth(); Ok(a)
}
fn require_not_paused(env: &Env) -> Result<(), EscrowError> {
    if env.storage().instance().get::<_, bool>(&DataKey::Paused).unwrap_or(false) {
        return Err(EscrowError::Paused);
    }
    Ok(())
}
fn load(env: &Env, id: u64) -> Result<EscrowContract, EscrowError> {
    env.storage().persistent().get(&DataKey::Contract(id)).ok_or(EscrowError::ContractNotFound)
}
fn save(env: &Env, c: &EscrowContract) {
    let key = DataKey::Contract(c.id);
    env.storage().persistent().set(&key, c);
    env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}
fn bump_instance(env: &Env) {
    env.storage().instance().extend_ttl(PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}
fn transfer(env: &Env, token_addr: &Address, from: &Address, to: &Address, amount: i128) {
    if amount > 0 { token::Client::new(env, token_addr).transfer(from, to, &amount); }
}
fn bump_key<K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, key: &K) {
    env.storage().persistent().extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

#[contractimpl]
impl Escrow {
    pub fn initialize(env: Env, admin: Address) -> Result<(), EscrowError> {
        if env.storage().instance().has(&DataKey::Admin) { return Err(EscrowError::AlreadyInitialized); }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::NextId, &1u64);
        bump_instance(&env);
        env.events().publish((symbol_short!("init"),), admin);
        Ok(())
    }
    pub fn version(_env: Env) -> u32 { 1 }
    pub fn get_admin(env: Env) -> Result<Address, EscrowError> { admin(&env) }
    pub fn set_paused(env: Env, paused: bool) -> Result<(), EscrowError> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &paused);
        bump_instance(&env);
        env.events().publish((symbol_short!("paused"),), paused);
        Ok(())
    }
    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
    }

    pub fn create_contract(
        env: Env, client: Address, freelancer: Address, token: Address,
        milestone_amounts: Vec<i128>,
    ) -> Result<u64, EscrowError> {
        admin(&env)?; require_not_paused(&env)?; client.require_auth();
        if client == freelancer { return Err(EscrowError::InvalidParties); }
        let n = milestone_amounts.len();
        if n == 0 || n > MAX_MILESTONES { return Err(EscrowError::InvalidMilestones); }
        let mut milestones = Vec::new(&env);
        let mut total: i128 = 0;
        for amount in milestone_amounts.iter() {
            if amount <= 0 { return Err(EscrowError::InvalidAmount); }
            total = total.checked_add(amount).ok_or(EscrowError::InvalidAmount)?;
            milestones.push_back(Milestone { amount, released: false });
        }
        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));
        bump_instance(&env);
        let c = EscrowContract {
            id, client: client.clone(), freelancer: freelancer.clone(), token, milestones,
            total_amount: total, funded_amount: 0, released_amount: 0,
            status: ContractStatus::Created, reputation_issued: false,
        };
        save(&env, &c);
        env.events().publish((symbol_short!("created"), id), (client, freelancer, total));
        Ok(id)
    }

    pub fn deposit_funds(env: Env, id: u64, amount: i128) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?; c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) { return Err(EscrowError::InvalidStatus); }
        if amount <= 0 { return Err(EscrowError::InvalidAmount); }
        let new_funded = c.funded_amount.checked_add(amount).ok_or(EscrowError::InvalidAmount)?;
        if new_funded > c.total_amount { return Err(EscrowError::OverFunded); }
        transfer(&env, &c.token, &c.client, &env.current_contract_address(), amount);
        c.funded_amount = new_funded;
        if c.funded_amount == c.total_amount { c.status = ContractStatus::Funded; }
        save(&env, &c);
        env.events().publish((symbol_short!("deposit"), id), amount);
        Ok(())
    }

    pub fn release_milestone(env: Env, id: u64, index: u32) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?; c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) { return Err(EscrowError::InvalidStatus); }
        let mut m = c.milestones.get(index).ok_or(EscrowError::MilestoneNotFound)?;
        if m.released { return Err(EscrowError::MilestoneAlreadyReleased); }
        if c.funded_amount - c.released_amount < m.amount { return Err(EscrowError::InsufficientFunds); }
        transfer(&env, &c.token, &env.current_contract_address(), &c.freelancer, m.amount);
        m.released = true; c.released_amount += m.amount; c.milestones.set(index, m.clone());
        if c.milestones.iter().all(|x| x.released) { c.status = ContractStatus::Completed; }
        save(&env, &c);
        env.events().publish((symbol_short!("release"), id), (index, m.amount));
        Ok(())
    }

    pub fn open_dispute(env: Env, id: u64, caller: Address) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?; caller.require_auth();
        if caller != c.client && caller != c.freelancer { return Err(EscrowError::Unauthorized); }
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) { return Err(EscrowError::InvalidStatus); }
        c.status = ContractStatus::Disputed; save(&env, &c);
        env.events().publish((symbol_short!("dispute"), id), caller);
        Ok(())
    }

    pub fn resolve_dispute(env: Env, id: u64, freelancer_bps: u32) -> Result<(), EscrowError> {
        require_admin(&env)?;
        let mut c = load(&env, id)?;
        if c.status != ContractStatus::Disputed { return Err(EscrowError::InvalidStatus); }
        if freelancer_bps as i128 > BPS_DENOMINATOR { return Err(EscrowError::InvalidSplit); }
        let held = c.funded_amount - c.released_amount;
        let to_freelancer = held * freelancer_bps as i128 / BPS_DENOMINATOR;
        let to_client = held - to_freelancer;
        let this = env.current_contract_address();
        transfer(&env, &c.token, &this, &c.freelancer, to_freelancer);
        transfer(&env, &c.token, &this, &c.client, to_client);
        c.released_amount += to_freelancer; c.funded_amount = c.released_amount;
        c.status = ContractStatus::Resolved; save(&env, &c);
        env.events().publish((symbol_short!("resolved"), id), (to_freelancer, to_client));
        Ok(())
    }

    pub fn cancel_contract(env: Env, id: u64) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?; c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) { return Err(EscrowError::InvalidStatus); }
        let refund = c.funded_amount - c.released_amount;
        transfer(&env, &c.token, &env.current_contract_address(), &c.client, refund);
        c.funded_amount = c.released_amount; c.status = ContractStatus::Cancelled; save(&env, &c);
        env.events().publish((symbol_short!("cancel"), id), refund);
        Ok(())
    }

    pub fn issue_reputation(env: Env, id: u64, rating: u32) -> Result<(), EscrowError> {
        let mut c = load(&env, id)?; c.client.require_auth();
        if c.status != ContractStatus::Completed { return Err(EscrowError::InvalidStatus); }
        if c.reputation_issued { return Err(EscrowError::ReputationAlreadyIssued); }
        if !(1..=5).contains(&rating) { return Err(EscrowError::InvalidRating); }
        let key = DataKey::Reputation(c.freelancer.clone());
        let mut rep: Reputation = env.storage().persistent().get(&key).unwrap_or_default();
        rep.completed_contracts += 1; rep.total_rating += rating;
        env.storage().persistent().set(&key, &rep);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        c.reputation_issued = true; save(&env, &c);
        env.events().publish((symbol_short!("rep"), id), (c.freelancer, rating));
        Ok(())
    }

    pub fn get_contract(env: Env, id: u64) -> Result<EscrowContract, EscrowError> { load(&env, id) }
    pub fn contract_count(env: Env) -> u64 {
        env.storage().instance().get::<_, u64>(&DataKey::NextId).unwrap_or(1) - 1
    }
    pub fn get_reputation(env: Env, freelancer: Address) -> Reputation {
        env.storage().persistent().get(&DataKey::Reputation(freelancer)).unwrap_or_default()
    }

    // ── Terms and deliverable hashes (issue #1707) ────────────────────────────

    /// Commits a 32-byte terms hash and optional per-milestone deliverable spec hashes.
    /// Client-only; callable only in `Created` status with `funded_amount == 0`.
    /// `milestone_hashes.len()` must be 0 or equal the milestone count (`InvalidMetadata`).
    /// Once any deposit is made, returns `InvalidStatus` (terms are frozen).
    /// Emits `("terms", id)` → `terms_hash`.
    pub fn set_terms(
        env: Env,
        id: u64,
        terms_hash: BytesN<32>,
        milestone_hashes: Vec<BytesN<32>>,
    ) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let c = load(&env, id)?;
        c.client.require_auth();
        if c.status != ContractStatus::Created || c.funded_amount > 0 {
            return Err(EscrowError::InvalidStatus);
        }
        let n = milestone_hashes.len();
        if n != 0 && n != c.milestones.len() {
            return Err(EscrowError::InvalidMetadata);
        }
        let terms_key = DataKey::Terms(id);
        env.storage().persistent().set(&terms_key, &terms_hash);
        bump_key(&env, &terms_key);
        if n > 0 {
            let specs_key = DataKey::MilestoneSpecs(id);
            env.storage().persistent().set(&specs_key, &milestone_hashes);
            bump_key(&env, &specs_key);
        }
        env.events().publish((Symbol::new(&env, "terms"), id), terms_hash);
        Ok(())
    }

    /// Freelancer records a delivery hash for a milestone. Overwritable until released.
    /// Emits `("deliver", id)` → `(index, delivery_hash)`.
    pub fn deliver(
        env: Env,
        id: u64,
        index: u32,
        delivery_hash: BytesN<32>,
    ) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let c = load(&env, id)?;
        c.freelancer.require_auth();
        let m = c.milestones.get(index).ok_or(EscrowError::MilestoneNotFound)?;
        if m.released { return Err(EscrowError::MilestoneAlreadyReleased); }
        let delivery_key = DataKey::Delivery(id, index);
        env.storage().persistent().set(&delivery_key, &delivery_hash);
        bump_key(&env, &delivery_key);
        env.events().publish((Symbol::new(&env, "deliver"), id), (index, delivery_hash));
        Ok(())
    }

    /// Returns `(terms_hash, milestone_spec_hashes)` if terms have been set, else `None`.
    /// Extends TTL on read.
    pub fn get_terms(env: Env, id: u64) -> Option<(BytesN<32>, Vec<BytesN<32>>)> {
        let terms_key = DataKey::Terms(id);
        let terms: BytesN<32> = env.storage().persistent().get(&terms_key)?;
        bump_key(&env, &terms_key);
        let specs_key = DataKey::MilestoneSpecs(id);
        let specs: Vec<BytesN<32>> = env.storage().persistent()
            .get(&specs_key).unwrap_or_else(|| Vec::new(&env));
        if !specs.is_empty() { bump_key(&env, &specs_key); }
        Some((terms, specs))
    }

    /// Returns the delivery hash for a milestone, or `None`. Extends TTL on read.
    pub fn get_delivery(env: Env, id: u64, index: u32) -> Option<BytesN<32>> {
        let k = DataKey::Delivery(id, index);
        let h: Option<BytesN<32>> = env.storage().persistent().get(&k);
        if h.is_some() { bump_key(&env, &k); }
        h
    }
}

#[cfg(test)]
mod test;
