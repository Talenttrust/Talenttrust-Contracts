//! TalentTrust escrow contract for milestone-based freelancer payments on Stellar (Soroban).
//!
//! Lifecycle:
//! 1. `initialize(admin)` once after deployment.
//! 2. A client calls `create_contract(client, freelancer, token, milestone_amounts)`.
//! 3. The client funds the escrow with `deposit_funds` (real token transfer into the contract).
//! 4. The client approves work with `release_milestone`, paying the freelancer per milestone.
//! 5. Either party may `open_dispute`; the admin settles with `resolve_dispute`.
//! 6. The client can `cancel_contract` before completion to recover unreleased funds.
//! 7. After completion the client may `issue_reputation` for the freelancer.
//! 8. The client may `pay_bonus` directly to the freelancer (tip/bonus).
//! 9. After completion or resolution the freelancer may `rate_client` to rate the client.
//! 10. Anyone may `expire_unfunded` an escrow still unfunded past its funding deadline,
//!     refunding any partial deposit to its funders.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Map, Symbol,
    Vec,
};

pub const MAX_MILESTONES: u32 = 20;
/// Maximum number of ids accepted by `get_escrow_summaries`.
pub const MAX_SUMMARY_BATCH: u32 = 25;
pub const BPS_DENOMINATOR: i128 = 10_000;
/// Funding window after creation (14 days); afterwards an escrow still in
/// `Created` can no longer be funded and may be expired by anyone.
pub const FUNDING_WINDOW_SECS: u64 = 14 * 86_400;
/// Minimum amount for a single milestone: 0.1 token unit at 7 decimal places.
pub const MIN_MILESTONE_AMOUNT: i128 = 1_000_000;
/// Maximum total escrow value: 10^18 stroops.
pub const MAX_ESCROW_TOTAL: i128 = 1_000_000_000_000_000_000; // 10^18
/// Minimum value actually released to the freelancer for a rating to count:
/// 1 token unit at 7 decimals (10x `MIN_MILESTONE_AMOUNT`). Sybil resistance
/// comes from this value threshold (farming reputation costs real value), not
/// from counterparty graph analysis.
pub const MIN_REPUTATION_VALUE: i128 = 10_000_000;
/// Escrows with `total_amount >= PANEL_THRESHOLD_AMOUNT` can only be settled
/// by the arbiter panel (`vote_resolution`), not by a single `resolve_dispute`
/// call. 10^12 base units (~100k tokens on a 7-decimal asset).
pub const PANEL_THRESHOLD_AMOUNT: i128 = 1_000_000_000_000;
/// Maximum number of members on the arbiter panel.
pub const PANEL_MAX_MEMBERS: u32 = 7;
/// Maximum number of distinct funders per escrow (`deposit_from` / `deposit_funds`).
pub const MAX_FUNDERS: u32 = 10;
/// Threshold for dispute outcome accounting (5,000 basis points, or 50%).
/// In dispute resolution, if freelancer_bps >= DISPUTE_WIN_THRESHOLD_BPS,
/// the outcome is counted as a win for the freelancer and a loss for the client; otherwise, the reverse.
pub const DISPUTE_WIN_THRESHOLD_BPS: u32 = 5_000;
const PERSISTENT_TTL_THRESHOLD: u32 = 17_280; // ~1 day
const PERSISTENT_TTL_EXTEND: u32 = 518_400; // ~30 days
/// Grace period (in ledgers, ~30 days at ~5s per ledger) after which an unrated
/// terminal escrow may be compacted.
pub const THIRTY_DAYS_IN_LEDGERS: u32 = 518_400;

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
    InvalidTransition = 20,
    NoSurplus = 21,
    EscrowFrozen = 22,
    AdminCannotBeParty = 23,
    MilestoneTooSmall = 24,
    NotCompactable = 25,
    EscrowCompacted = 26,
    InvalidSplitConfig = 27,
    OutOfOrder = 28,
    DuplicateIndex = 29,
    BelowReputationThreshold = 30,
    InvalidPanel = 31,
    PanelRequired = 32,
    RoleUnchanged = 33,
    TooManyFunders = 34,
    FundingExpired = 35,
    NotExpired = 36,
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
    /// Amount refunded to the client when the escrow closed (cancel / dispute split).
    pub refunded_amount: i128,
    pub status: ContractStatus,
    pub reputation_issued: bool,
}

/// Read-only view of an escrow's status and derived balances.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowSummary {
    pub id: u64,
    pub status: ContractStatus,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub held: i128,
    pub remaining_to_fund: i128,
    pub milestones_total: u32,
    pub milestones_released: u32,
    pub next_unreleased: Option<u32>,
    pub next_release_affordable: bool,
}

/// Compact record kept for a terminal escrow after `compact_escrow`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowReceipt {
    pub id: u64,
    pub client: Address,
    pub freelancer: Address,
    pub token: Address,
    pub total_amount: i128,
    pub released_amount: i128,
    pub refunded_amount: i128,
    pub status: ContractStatus,
    pub milestone_count: u32,
    pub closed_at: u64,
    pub reputation_issued: bool,
}

#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reputation {
    pub completed_contracts: u32,
    pub total_rating: u32,
}

/// Delegated operational roles. The admin implicitly holds every role.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Pauser,
    Resolver,
}

/// Admin-managed arbiter panel that settles high-value disputes by M-of-N vote.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Panel {
    pub members: Vec<Address>,
    pub threshold: u32,
}

/// Value-weighted reputation: each rating is weighted by the escrow's released amount.
#[contracttype]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WeightedReputation {
    pub weighted_rating_sum: i128,
    pub weight_total: i128,
    pub rated_contracts: u32,
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
    Bonus(u64),
    BonusTotal(Address),
    Liability(Address),
    Payout(u64),
    Frozen(u64),
    ClosedAt(u64),
    Receipt(u64),
    Team(u64),
    ClientReputation(Address),
    ClientRated(u64),
    Sequential(u64),
    WeightedRep(Address),
    RatingWeight(u64),
    Panel,
    Votes(u64),
    RoleMember(Role, Address),
    Funders(u64),
    FundBy(u64),
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

fn holds_role(env: &Env, role: Role, account: &Address) -> bool {
    if let Ok(admin_addr) = admin(env) {
        if admin_addr == *account {
            return true;
        }
    }
    env.storage()
        .persistent()
        .get::<_, bool>(&DataKey::RoleMember(role, account.clone()))
        .unwrap_or(false)
}

fn set_role(env: &Env, role: Role, account: Address, member: bool) -> Result<(), EscrowError> {
    require_admin(env)?;
    let key = DataKey::RoleMember(role, account.clone());
    let current = env
        .storage()
        .persistent()
        .get::<_, bool>(&key)
        .unwrap_or(false);
    if current == member {
        return Err(EscrowError::RoleUnchanged);
    }
    env.storage().persistent().set(&key, &member);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
    env.events()
        .publish((symbol_short!("role_set"), role), (account, member));
    Ok(())
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

fn escrow_frozen(env: &Env, id: u64) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Frozen(id))
        .unwrap_or(false)
}

fn require_not_frozen(env: &Env, id: u64) -> Result<(), EscrowError> {
    if escrow_frozen(env, id) {
        return Err(EscrowError::EscrowFrozen);
    }
    Ok(())
}

fn load(env: &Env, id: u64) -> Result<EscrowContract, EscrowError> {
    env.storage()
        .persistent()
        .get(&DataKey::Contract(id))
        .ok_or(EscrowError::ContractNotFound)
}

fn summarize(c: &EscrowContract) -> EscrowSummary {
    let mut milestones_released = 0u32;
    let mut next_unreleased: Option<u32> = None;
    for (i, m) in c.milestones.iter().enumerate() {
        if m.released {
            milestones_released += 1;
        } else if next_unreleased.is_none() {
            next_unreleased = Some(i as u32);
        }
    }
    let held = c.funded_amount - c.released_amount;
    let next_release_affordable = match next_unreleased {
        Some(index) => c.milestones.get(index).map(|m| held >= m.amount).unwrap_or(false),
        None => false,
    };
    EscrowSummary {
        id: c.id,
        status: c.status,
        total_amount: c.total_amount,
        funded_amount: c.funded_amount,
        released_amount: c.released_amount,
        held,
        remaining_to_fund: c.total_amount - c.funded_amount,
        milestones_total: c.milestones.len(),
        milestones_released,
        next_unreleased,
        next_release_affordable,
    }
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

// Liability helper functions: aggregate per-token amount the contract owes to live escrows.
fn liability_of(env: &Env, token: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::Liability(token.clone()))
        .unwrap_or(0)
}

fn set_liability(env: &Env, token: &Address, value: i128) {
    let key = DataKey::Liability(token.clone());
    env.storage().persistent().set(&key, &value);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn add_liability(env: &Env, token: &Address, amount: i128) -> Result<(), EscrowError> {
    let updated = liability_of(env, token)
        .checked_add(amount)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    set_liability(env, token, updated);
    Ok(())
}

fn sub_liability(env: &Env, token: &Address, amount: i128) -> Result<(), EscrowError> {
    let updated = liability_of(env, token)
        .checked_sub(amount)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    set_liability(env, token, updated);
    Ok(())
}

/// Solvency invariant (test + debug only): the contract's token balance covers all liabilities.
#[cfg(any(test, debug_assertions))]
fn assert_solvent(env: &Env, token: &Address) {
    let balance = token::Client::new(env, token).balance(&env.current_contract_address());
    let liability = liability_of(env, token);
    assert!(
        balance >= liability,
        "Solvency invariant violated: balance {} < liability {}",
        balance,
        liability
    );
}

/// Returns the address that should receive freelancer payouts for `c`.
///
/// If the freelancer has set a dedicated payout address via
/// [`Escrow::set_payout_address`] it is returned; otherwise payouts go to the
/// freelancer's own address (`c.freelancer`). Reputation always accrues to
/// `c.freelancer` regardless of the payout address.
fn payee(env: &Env, c: &EscrowContract) -> Address {
    env.storage()
        .persistent()
        .get(&DataKey::Payout(c.id))
        .unwrap_or_else(|| c.freelancer.clone())
}

/// Records the ledger sequence at which an escrow entered a terminal status.
fn record_closed_at(env: &Env, id: u64) {
    let key = DataKey::ClosedAt(id);
    env.storage()
        .persistent()
        .set(&key, &u64::from(env.ledger().sequence()));
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn is_compacted(env: &Env, id: u64) -> bool {
    env.storage().persistent().has(&DataKey::Receipt(id))
}

fn team_split(env: &Env, id: u64) -> Vec<(Address, u32)> {
    env.storage()
        .persistent()
        .get(&DataKey::Team(id))
        .unwrap_or_else(|| Vec::new(env))
}

/// Distributes `amount` across fixed basis-point shares. Each share is rounded
/// down and the remainder is added to the first (lead) payee, so the sum of all
/// transfers is always exactly `amount`.
fn distribute(env: &Env, token_addr: &Address, payees: &Vec<(Address, u32)>, amount: i128) {
    if amount <= 0 || payees.is_empty() {
        return;
    }
    let mut floor_total: i128 = 0;
    for (_, bps) in payees.iter() {
        floor_total += amount * bps as i128 / BPS_DENOMINATOR;
    }
    let remainder = amount - floor_total;
    let this = env.current_contract_address();
    let mut is_lead = true;
    for (payee, bps) in payees.iter() {
        let mut share = amount * bps as i128 / BPS_DENOMINATOR;
        if is_lead {
            share += remainder;
            is_lead = false;
        }
        transfer(env, token_addr, &this, &payee, share);
    }
}

/// Pays a freelancer-side `amount` to the configured team split, or to the
/// freelancer's payout address (see `payee`) when no split is configured.
fn pay_out(env: &Env, c: &EscrowContract, amount: i128) {
    let payees = team_split(env, c.id);
    if payees.is_empty() {
        let this = env.current_contract_address();
        transfer(env, &c.token, &this, &payee(env, c), amount);
    } else {
        distribute(env, &c.token, &payees, amount);
    }
}

/// Shared builder for `create_contract` and `create_contract_sequential`.
fn build_escrow(
    env: Env,
    client: Address,
    freelancer: Address,
    token: Address,
    milestone_amounts: Vec<i128>,
) -> Result<u64, EscrowError> {
    let admin_addr = admin(&env)?;
    require_not_paused(&env)?;
    client.require_auth();

    // Validation order: parties -> milestones -> amounts.
    // ---- Parties ----
    let contract_addr = env.current_contract_address();
    if client == contract_addr
        || freelancer == contract_addr
        || token == contract_addr
        || client == freelancer
    {
        return Err(EscrowError::InvalidParties);
    }
    if client == admin_addr || freelancer == admin_addr {
        return Err(EscrowError::AdminCannotBeParty);
    }

    // ---- Milestones ----
    let n = milestone_amounts.len();
    if n == 0 || n > MAX_MILESTONES {
        return Err(EscrowError::InvalidMilestones);
    }

    // ---- Amounts ----
    let mut milestones = Vec::new(&env);
    let mut total: i128 = 0;
    for amount in milestone_amounts.iter() {
        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }
        if amount < MIN_MILESTONE_AMOUNT {
            return Err(EscrowError::MilestoneTooSmall);
        }
        total = total.checked_add(amount).ok_or(EscrowError::InvalidAmount)?;
        milestones.push_back(Milestone {
            amount,
            released: false,
        });
    }
    if total > MAX_ESCROW_TOTAL {
        return Err(EscrowError::InvalidAmount);
    }
    let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);
    env.storage().instance().set(&DataKey::NextId, &(id + 1));
    bump_instance(&env);
    let fund_by = env
        .ledger()
        .timestamp()
        .checked_add(FUNDING_WINDOW_SECS)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    let fund_key = DataKey::FundBy(id);
    env.storage().persistent().set(&fund_key, &fund_by);
    env.storage()
        .persistent()
        .extend_ttl(&fund_key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
    let c = EscrowContract {
        id,
        client: client.clone(),
        freelancer: freelancer.clone(),
        token,
        milestones,
        total_amount: total,
        funded_amount: 0,
        released_amount: 0,
        refunded_amount: 0,
        status: ContractStatus::Created,
        reputation_issued: false,
    };
    save(&env, &c);
    env.events()
        .publish((symbol_short!("created"), id), (client, freelancer, total));
    Ok(id)
}

fn is_sequential(env: &Env, id: u64) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::Sequential(id))
        .unwrap_or(false)
}

fn first_unreleased(c: &EscrowContract) -> Option<u32> {
    c.milestones
        .iter()
        .position(|m| !m.released)
        .map(|i| i as u32)
}

/// Overflow-safe `round(sum * 100 / total)`; `None` when `total <= 0`.
pub fn reputation_score(sum: i128, total: i128) -> Option<u32> {
    if total <= 0 || sum < 0 {
        return None;
    }
    let whole = sum / total;
    let rem = sum % total;
    // rem < total, so rem * 100 only overflows for totals near i128::MAX / 100.
    let frac = match rem.checked_mul(100) {
        Some(scaled) => {
            let q = scaled / total;
            let r = scaled % total;
            // round half up without computing total / 2 + r overflow
            if r >= total - r {
                q + 1
            } else {
                q
            }
        }
        None => rem / (total / 100),
    };
    let score = whole.checked_mul(100)?.checked_add(frac)?;
    u32::try_from(score).ok()
}

/// Splits the held balance of a disputed escrow: `freelancer_bps` (0..=10000)
/// of the held amount goes to the freelancer side, the rest is refunded to the
/// client. Shared by `resolve_dispute` and the arbiter panel (`vote_resolution`).
fn settle(env: &Env, mut c: EscrowContract, freelancer_bps: u32) -> Result<(), EscrowError> {
    let held = c.funded_amount - c.released_amount;
    let to_freelancer = held * freelancer_bps as i128 / BPS_DENOMINATOR;
    let to_client = held - to_freelancer;
    pay_out(env, &c, to_freelancer);
    refund_funders(env, &c, to_client)?;
    c.released_amount += to_freelancer;
    c.refunded_amount = to_client;
    c.funded_amount = c.released_amount;
    transition(&mut c, ContractStatus::Resolved)?;
    record_closed_at(env, c.id);
    sub_liability(env, &c.token, held)?;
    #[cfg(any(test, debug_assertions))]
    assert_solvent(env, &c.token);
    save(env, &c);

    // Record dispute outcomes for both parties
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
        .publish((symbol_short!("resolved"), c.id), (to_freelancer, to_client));
    Ok(())
}

/// Shared deposit logic: auth the funder, validate status/amount/overfunding,
/// transfer tokens, track the funder's contribution, update the liability
/// ledger, save, and emit `("deposit", id) -> (funder, amount)`.
fn apply_deposit(
    env: &Env,
    c: &mut EscrowContract,
    funder: &Address,
    amount: i128,
) -> Result<(), EscrowError> {
    funder.require_auth();
    if c.status != ContractStatus::Created {
        return Err(EscrowError::InvalidStatus);
    }
    // Escrows created before funding deadlines existed have no `FundBy` entry.
    if let Some(fund_by) = env
        .storage()
        .persistent()
        .get::<_, u64>(&DataKey::FundBy(c.id))
    {
        if env.ledger().timestamp() > fund_by {
            return Err(EscrowError::FundingExpired);
        }
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
    let key = DataKey::Funders(c.id);
    let mut funders: Map<Address, i128> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Map::new(env));
    let prev = funders.get(funder.clone());
    if prev.is_none() && funders.len() >= MAX_FUNDERS {
        return Err(EscrowError::TooManyFunders);
    }
    let contribution = prev
        .unwrap_or(0)
        .checked_add(amount)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    funders.set(funder.clone(), contribution);
    transfer(env, &c.token, funder, &env.current_contract_address(), amount);
    env.storage().persistent().set(&key, &funders);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
    c.funded_amount = new_funded;
    if c.funded_amount == c.total_amount {
        transition(c, ContractStatus::Funded)?;
    }
    add_liability(env, &c.token, amount)?;
    #[cfg(any(test, debug_assertions))]
    assert_solvent(env, &c.token);
    save(env, c);
    env.events()
        .publish((symbol_short!("deposit"), c.id), (funder.clone(), amount));
    Ok(())
}

/// Distributes `refund` pro-rata to funders by contribution. The rounding
/// remainder goes to the largest funder so the sum distributed equals `refund`
/// exactly (no dust is lost). Escrows without a funder record (funded before
/// sponsor tracking existed) refund the client, as before.
fn refund_funders(env: &Env, c: &EscrowContract, refund: i128) -> Result<(), EscrowError> {
    if refund <= 0 {
        return Ok(());
    }
    let this = env.current_contract_address();
    let funders: Map<Address, i128> = env
        .storage()
        .persistent()
        .get(&DataKey::Funders(c.id))
        .unwrap_or_else(|| Map::new(env));
    let total_funded = c.funded_amount;
    if funders.is_empty() || total_funded <= 0 {
        transfer(env, &c.token, &this, &c.client, refund);
        return Ok(());
    }
    let mut remaining = refund;
    let mut largest: Option<Address> = None;
    let mut largest_amt: i128 = -1;
    for (funder, contribution) in funders.iter() {
        let share = contribution
            .checked_mul(refund)
            .ok_or(EscrowError::ArithmeticOverflow)?
            / total_funded;
        if share > 0 {
            transfer(env, &c.token, &this, &funder, share);
            remaining = remaining
                .checked_sub(share)
                .ok_or(EscrowError::ArithmeticOverflow)?;
        }
        if contribution > largest_amt {
            largest_amt = contribution;
            largest = Some(funder);
        }
    }
    if remaining > 0 {
        let to = largest.unwrap_or_else(|| c.client.clone());
        transfer(env, &c.token, &this, &to, remaining);
    }
    Ok(())
}

pub fn transition(c: &mut EscrowContract, to: ContractStatus) -> Result<(), EscrowError> {
    use ContractStatus::*;
    let allowed = match (c.status, to) {
        (Created, Funded) => true,
        (Created, Completed) => true,
        (Funded, Completed) => true,
        (Created, Disputed) => true,
        (Funded, Disputed) => true,
        (Disputed, Resolved) => true,
        (Created, Cancelled) => true,
        (Funded, Cancelled) => true,
        _ => false,
    };
    if !allowed {
        return Err(EscrowError::InvalidTransition);
    }
    c.status = to;
    Ok(())
}

/// Asserts core escrow lifecycle invariants (test + debug only).
pub fn check_invariants(c: &EscrowContract) {
    // 0 <= released <= funded <= total
    assert!(c.released_amount >= 0, "released_amount must be non-negative");
    assert!(
        c.released_amount <= c.funded_amount,
        "released_amount must be <= funded_amount"
    );
    assert!(
        c.funded_amount <= c.total_amount,
        "funded_amount must be <= total_amount"
    );

    // status == Completed <=> all milestones released
    let all_released = c.milestones.iter().all(|x| x.released);
    assert_eq!(
        c.status == ContractStatus::Completed,
        all_released,
        "status == Completed iff all milestones released"
    );

    // terminal states hold funded == released
    if matches!(
        c.status,
        ContractStatus::Completed | ContractStatus::Resolved | ContractStatus::Cancelled
    ) {
        assert_eq!(
            c.funded_amount, c.released_amount,
            "terminal state holds funded_amount == released_amount"
        );
    }
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

    /// Circuit breaker. Caller must be the admin or hold the `Pauser` role.
    pub fn set_paused(env: Env, caller: Address, paused: bool) -> Result<(), EscrowError> {
        caller.require_auth();
        if !holds_role(&env, Role::Pauser, &caller) {
            return Err(EscrowError::Unauthorized);
        }
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

    /// Returns the active per-token liability counter.
    pub fn get_liability(env: Env, token: Address) -> i128 {
        liability_of(&env, &token)
    }

    /// Admin-only: sweeps untracked token surplus above aggregate liabilities.
    pub fn sweep_surplus(env: Env, token: Address, to: Address) -> Result<i128, EscrowError> {
        require_admin(&env)?;
        let token_client = token::Client::new(&env, &token);
        let this = env.current_contract_address();
        let balance = token_client.balance(&this);
        let liability = liability_of(&env, &token);
        let surplus = balance.checked_sub(liability).unwrap_or(0);
        if surplus <= 0 {
            return Err(EscrowError::NoSurplus);
        }
        token_client.transfer(&this, &to, &surplus);
        env.events()
            .publish((symbol_short!("sweep"), token), (to, surplus));
        Ok(surplus)
    }

    /// Freelancer sets a separate payout address for this escrow. Subsequent
    /// `release_milestone`, `resolve_dispute` and `pay_bonus` payouts go to
    /// `payout` instead of the freelancer's own address. Reputation still
    /// accrues to the freelancer.
    ///
    /// Only callable while the escrow is `Created` or `Funded`; rejected with
    /// `InvalidStatus` while a dispute is open. `payout` must not be the
    /// contract address or the client (`InvalidParties`).
    pub fn set_payout_address(env: Env, id: u64, payout: Address) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        c.freelancer.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        if payout == env.current_contract_address() || payout == c.client {
            return Err(EscrowError::InvalidParties);
        }
        let old = payee(&env, &c);
        let key = DataKey::Payout(id);
        env.storage().persistent().set(&key, &payout);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        env.events()
            .publish((symbol_short!("payout"), id), (old, payout));
        Ok(())
    }

    /// Returns the payout address for `id`, falling back to the freelancer
    /// when no dedicated payout address has been set.
    pub fn get_payout_address(env: Env, id: u64) -> Result<Address, EscrowError> {
        let c = load(&env, id)?;
        Ok(payee(&env, &c))
    }

    /// Freezes a single escrow agreement (admin only). While frozen, every
    /// party-initiated mutation on that escrow returns `EscrowFrozen`;
    /// `resolve_dispute` remains available so the admin can settle it.
    pub fn freeze_escrow(env: Env, id: u64, reason: Symbol) -> Result<(), EscrowError> {
        require_admin(&env)?;
        if !env.storage().persistent().has(&DataKey::Contract(id)) {
            return Err(EscrowError::ContractNotFound);
        }
        if escrow_frozen(&env, id) {
            return Err(EscrowError::EscrowFrozen);
        }
        let key = DataKey::Frozen(id);
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        env.events().publish((symbol_short!("frozen"), id), reason);
        Ok(())
    }

    /// Unfreezes a frozen escrow agreement (admin only).
    pub fn unfreeze_escrow(env: Env, id: u64) -> Result<(), EscrowError> {
        require_admin(&env)?;
        if !env.storage().persistent().has(&DataKey::Contract(id)) {
            return Err(EscrowError::ContractNotFound);
        }
        if !escrow_frozen(&env, id) {
            return Err(EscrowError::InvalidStatus);
        }
        let key = DataKey::Frozen(id);
        env.storage().persistent().set(&key, &false);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        env.events().publish((symbol_short!("unfrozen"), id), ());
        Ok(())
    }

    /// Checks if a specific escrow is frozen.
    pub fn is_frozen(env: Env, id: u64) -> bool {
        escrow_frozen(&env, id)
    }

    /// Creates a new escrow agreement. Returns the escrow id.
    pub fn create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        token: Address,
        milestone_amounts: Vec<i128>,
    ) -> Result<u64, EscrowError> {
        build_escrow(env, client, freelancer, token, milestone_amounts)
    }

    /// Creates an escrow whose milestones must be released strictly in order
    /// (same validation as `create_contract`). Emits `("seq", id) -> true`.
    pub fn create_contract_sequential(
        env: Env,
        client: Address,
        freelancer: Address,
        token: Address,
        milestone_amounts: Vec<i128>,
    ) -> Result<u64, EscrowError> {
        let id = build_escrow(env.clone(), client, freelancer, token, milestone_amounts)?;
        let key = DataKey::Sequential(id);
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        env.events().publish((symbol_short!("seq"), id), true);
        Ok(id)
    }

    /// Lowest unreleased milestone index, or `None` once every milestone is
    /// released (works for both sequential and non-sequential escrows).
    pub fn next_releasable(env: Env, id: u64) -> Result<Option<u32>, EscrowError> {
        let c = load(&env, id)?;
        Ok(first_unreleased(&c))
    }

    /// Freelancer configures fixed basis-point shares for a team of up to five
    /// payees. Only while `Created` and before any deposit; shares must be
    /// non-zero, unique, exclude the client / contract and sum to 10,000 bps.
    pub fn set_team_split(
        env: Env,
        id: u64,
        payees: Vec<(Address, u32)>,
    ) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        c.freelancer.require_auth();
        if c.status != ContractStatus::Created || c.funded_amount != 0 {
            return Err(EscrowError::InvalidStatus);
        }
        let len = payees.len();
        if len == 0 || len > 5 {
            return Err(EscrowError::InvalidSplitConfig);
        }
        let this = env.current_contract_address();
        let mut total: u32 = 0;
        for i in 0..len {
            let (payee, bps) = payees.get(i).unwrap();
            if bps == 0 || payee == c.client || payee == this {
                return Err(EscrowError::InvalidSplitConfig);
            }
            for j in (i + 1)..len {
                if payee == payees.get(j).unwrap().0 {
                    return Err(EscrowError::InvalidSplitConfig);
                }
            }
            total = total
                .checked_add(bps)
                .ok_or(EscrowError::InvalidSplitConfig)?;
        }
        if total as i128 != BPS_DENOMINATOR {
            return Err(EscrowError::InvalidSplitConfig);
        }
        let key = DataKey::Team(id);
        env.storage().persistent().set(&key, &payees);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        env.events()
            .publish((symbol_short!("team"), id), payees.len());
        Ok(())
    }

    /// Returns the configured team split, or an empty vector if none is set.
    pub fn get_team_split(env: Env, id: u64) -> Vec<(Address, u32)> {
        team_split(&env, id)
    }

    /// Client deposits `amount` of the escrow token into the contract.
    pub fn deposit_funds(env: Env, id: u64, amount: i128) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        let client = c.client.clone();
        apply_deposit(&env, &mut c, &client, amount)
    }

    /// Any address (sponsor, grants program, employer...) deposits `amount` of
    /// the escrow token from `funder`. Contributions are tracked per funder and
    /// refunds are routed back pro-rata to the actual funders.
    pub fn deposit_from(env: Env, id: u64, funder: Address, amount: i128) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        apply_deposit(&env, &mut c, &funder, amount)
    }

    /// Returns the map of funder -> contribution for an escrow.
    pub fn get_funders(env: Env, id: u64) -> Map<Address, i128> {
        env.storage()
            .persistent()
            .get(&DataKey::Funders(id))
            .unwrap_or_else(|| Map::new(&env))
    }

    /// Client approves a milestone and the freelancer is paid.
    pub fn release_milestone(env: Env, id: u64, index: u32) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
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
        if is_sequential(&env, id) && first_unreleased(&c) != Some(index) {
            return Err(EscrowError::OutOfOrder);
        }
        if c.funded_amount - c.released_amount < m.amount {
            return Err(EscrowError::InsufficientFunds);
        }
        pay_out(&env, &c, m.amount);
        m.released = true;
        c.released_amount += m.amount;
        c.milestones.set(index, m.clone());
        if c.milestones.iter().all(|x| x.released) {
            transition(&mut c, ContractStatus::Completed)?;
            record_closed_at(&env, id);
        }
        sub_liability(&env, &c.token, m.amount)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, &c);
        env.events()
            .publish((symbol_short!("release"), id), (index, m.amount));
        Ok(())
    }

    /// Client approves several milestones in one call: all indices are
    /// validated first (all-or-nothing), the summed amount is paid out once and
    /// the escrow is saved once. Returns the total paid.
    pub fn release_milestones(env: Env, id: u64, indices: Vec<u32>) -> Result<i128, EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        let count = indices.len();
        if count == 0 || count > MAX_MILESTONES {
            return Err(EscrowError::InvalidMilestones);
        }

        let mut total: i128 = 0;
        for (pos, index) in indices.iter().enumerate() {
            for earlier in indices.iter().take(pos) {
                if earlier == index {
                    return Err(EscrowError::DuplicateIndex);
                }
            }
            let m = c
                .milestones
                .get(index)
                .ok_or(EscrowError::MilestoneNotFound)?;
            if m.released {
                return Err(EscrowError::MilestoneAlreadyReleased);
            }
            total = total
                .checked_add(m.amount)
                .ok_or(EscrowError::ArithmeticOverflow)?;
        }

        // Sequential escrows: every milestone before a requested index must be
        // already released or part of this batch.
        if is_sequential(&env, id) {
            for index in indices.iter() {
                for j in 0..index {
                    let prior = c.milestones.get(j).ok_or(EscrowError::MilestoneNotFound)?;
                    if !prior.released && !indices.contains(j) {
                        return Err(EscrowError::OutOfOrder);
                    }
                }
            }
        }

        let held = c
            .funded_amount
            .checked_sub(c.released_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if held < total {
            return Err(EscrowError::InsufficientFunds);
        }

        pay_out(&env, &c, total);
        for index in indices.iter() {
            let mut m = c.milestones.get(index).unwrap();
            m.released = true;
            c.milestones.set(index, m);
        }
        c.released_amount = c
            .released_amount
            .checked_add(total)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if c.milestones.iter().all(|x| x.released) {
            transition(&mut c, ContractStatus::Completed)?;
            record_closed_at(&env, id);
        }
        sub_liability(&env, &c.token, total)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, &c);
        for index in indices.iter() {
            let amount = c.milestones.get(index).unwrap().amount;
            env.events()
                .publish((symbol_short!("release"), id), (index, amount));
        }
        env.events()
            .publish((symbol_short!("rel_batch"), id), (count, total));
        Ok(total)
    }

    /// Client pays a bonus/tip directly to the freelancer.
    ///
    /// The transfer is made client -> freelancer and never touches the
    /// contract's held balance, so it cannot affect escrow accounting or a
    /// dispute split. Allowed on Funded or Completed escrows.
    pub fn pay_bonus(env: Env, id: u64, amount: i128) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Funded | ContractStatus::Completed) {
            return Err(EscrowError::InvalidStatus);
        }
        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        // Direct client -> freelancer transfer; escrow balance is untouched.
        transfer(&env, &c.token, &c.client, &payee(&env, &c), amount);

        // Cumulative per-escrow bonus.
        let bonus_key = DataKey::Bonus(id);
        let prev_bonus: i128 = env.storage().persistent().get(&bonus_key).unwrap_or(0);
        let cumulative = prev_bonus
            .checked_add(amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().persistent().set(&bonus_key, &cumulative);
        env.storage().persistent().extend_ttl(
            &bonus_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );

        // Cumulative per-freelancer bonus total.
        let total_key = DataKey::BonusTotal(c.freelancer.clone());
        let prev_total: i128 = env.storage().persistent().get(&total_key).unwrap_or(0);
        let new_total = prev_total
            .checked_add(amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().persistent().set(&total_key, &new_total);
        env.storage().persistent().extend_ttl(
            &total_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );

        env.events()
            .publish((symbol_short!("bonus"), id), (amount, cumulative));
        Ok(())
    }

    /// Returns the cumulative bonus paid on a given escrow.
    pub fn get_bonus(env: Env, id: u64) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Bonus(id))
            .unwrap_or(0)
    }

    /// Returns the cumulative bonus paid to a freelancer across all escrows.
    pub fn get_bonus_total(env: Env, freelancer: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::BonusTotal(freelancer))
            .unwrap_or(0)
    }

    /// Either party may raise a dispute, freezing releases until the admin resolves it.
    pub fn open_dispute(env: Env, id: u64, caller: Address) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        caller.require_auth();
        if caller != c.client && caller != c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        transition(&mut c, ContractStatus::Disputed)?;
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

    /// Resolves a dispute (caller must be the admin or hold the `Resolver` role and
    /// must not be a party to the escrow), paying `freelancer_bps` (0..=10000) of the escrowed
    /// balance to the freelancer and refunding the rest to the client.
    ///
    /// Outcomes:
    /// If freelancer_bps >= DISPUTE_WIN_THRESHOLD_BPS (5_000 bps, or 50%),
    /// freelancer gets a dispute win and client gets a dispute loss.
    /// Otherwise, freelancer gets a loss and client gets a win.
    /// Note: this operation remains allowed even when the escrow is frozen.
    /// Escrows at or above `PANEL_THRESHOLD_AMOUNT` must be settled by the
    /// arbiter panel instead and return `PanelRequired`.
    pub fn resolve_dispute(
        env: Env,
        caller: Address,
        id: u64,
        freelancer_bps: u32,
    ) -> Result<(), EscrowError> {
        caller.require_auth();
        if !holds_role(&env, Role::Resolver, &caller) {
            return Err(EscrowError::Unauthorized);
        }
        let c = load(&env, id)?;
        if c.status != ContractStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        if caller == c.client || caller == c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        if c.total_amount >= PANEL_THRESHOLD_AMOUNT {
            return Err(EscrowError::PanelRequired);
        }
        if freelancer_bps as i128 > BPS_DENOMINATOR {
            return Err(EscrowError::InvalidSplit);
        }
        settle(&env, c, freelancer_bps)
    }

    /// Admin replaces the arbiter panel. `threshold` members must agree on the
    /// same split before a dispute settles. 1 <= threshold <= len <= 7, no
    /// duplicate members. Changing the panel mid-dispute is allowed; only
    /// votes from the new members count toward the threshold from then on.
    pub fn set_panel(env: Env, members: Vec<Address>, threshold: u32) -> Result<(), EscrowError> {
        require_admin(&env)?;
        let len = members.len();
        if threshold == 0 || threshold > len || len > PANEL_MAX_MEMBERS {
            return Err(EscrowError::InvalidPanel);
        }
        for i in 0..len {
            let m = members.get(i).unwrap();
            if members.slice(i + 1..).iter().any(|x| x == m) {
                return Err(EscrowError::InvalidPanel);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::Panel, &Panel { members, threshold });
        bump_instance(&env);
        env.events()
            .publish((symbol_short!("panel"),), (len, threshold));
        Ok(())
    }

    /// A current panel member votes on a disputed escrow's split: `freelancer_bps`
    /// (0..=10000) of the held balance to the freelancer. A member may change
    /// their vote at any time; the latest vote is the only one that counts.
    /// Parties to the escrow may not vote. Once `threshold` current members
    /// have voted for the same split, the escrow settles immediately, votes
    /// are cleared, and ("resolved", id) plus one ("vote", id) per counted
    /// vote are emitted.
    pub fn vote_resolution(
        env: Env,
        member: Address,
        id: u64,
        freelancer_bps: u32,
    ) -> Result<(), EscrowError> {
        member.require_auth();
        let c = load(&env, id)?;
        if c.status != ContractStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        if member == c.client || member == c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        if freelancer_bps as i128 > BPS_DENOMINATOR {
            return Err(EscrowError::InvalidSplit);
        }
        let panel: Panel = env
            .storage()
            .instance()
            .get(&DataKey::Panel)
            .ok_or(EscrowError::Unauthorized)?;
        if !panel.members.contains(&member) {
            return Err(EscrowError::Unauthorized);
        }
        let key = DataKey::Votes(id);
        let mut votes: Map<Address, u32> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Map::new(&env));
        votes.set(member.clone(), freelancer_bps);
        env.storage().persistent().set(&key, &votes);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );
        // Only votes from members currently on the panel are tallied; stale
        // votes from removed members never reach the threshold.
        let mut counted: Vec<(Address, u32)> = Vec::new(&env);
        for v in votes.iter() {
            if panel.members.contains(&v.0) {
                counted.push_back((v.0, v.1));
            }
        }
        let mut winning_bps: u32 = 0;
        let mut winning_votes: u32 = 0;
        for (_, bps) in counted.iter() {
            let mut n: u32 = 0;
            for (_, other) in counted.iter() {
                if other == bps {
                    n += 1;
                }
            }
            if n > winning_votes {
                winning_votes = n;
                winning_bps = bps;
            }
        }
        if winning_votes >= panel.threshold {
            env.storage().persistent().remove(&key);
            settle(&env, c, winning_bps)?;
            for (voter, bps) in counted.iter() {
                env.events()
                    .publish((symbol_short!("vote"), id), (voter, bps));
            }
        }
        Ok(())
    }

    /// Anyone may expire an escrow that is still `Created` (not fully funded,
    /// nothing released) after its funding deadline. Any partial deposit is
    /// refunded to its funders (the client unless sponsors contributed) and the
    /// escrow is `Cancelled`. Emits `("expired", id) -> refund`.
    pub fn expire_unfunded(env: Env, id: u64) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        if c.status != ContractStatus::Created || c.released_amount != 0 {
            return Err(EscrowError::InvalidStatus);
        }
        let fund_by: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::FundBy(id))
            .ok_or(EscrowError::NotExpired)?;
        if env.ledger().timestamp() <= fund_by {
            return Err(EscrowError::NotExpired);
        }
        let refund = c.funded_amount;
        refund_funders(&env, &c, refund)?;
        c.refunded_amount = refund;
        c.funded_amount = 0;
        transition(&mut c, ContractStatus::Cancelled)?;
        record_closed_at(&env, id);
        sub_liability(&env, &c.token, refund)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, &c);
        env.events().publish((symbol_short!("expired"), id), refund);
        Ok(())
    }

    /// Funding deadline (ledger timestamp) of an escrow; `0` if none is recorded.
    pub fn get_fund_by(env: Env, id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::FundBy(id))
            .unwrap_or(0)
    }

    /// Client cancels the agreement and recovers any unreleased funds.
    /// If any funds were held and refunded, cancellations is incremented for the client.
    pub fn cancel_contract(env: Env, id: u64) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        let refund = c.funded_amount - c.released_amount;
        refund_funders(&env, &c, refund)?;
        c.refunded_amount = refund;
        c.funded_amount = c.released_amount;
        transition(&mut c, ContractStatus::Cancelled)?;
        record_closed_at(&env, id);
        sub_liability(&env, &c.token, refund)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
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
        // A compacted escrow keeps only an immutable receipt. Compaction of an
        // unrated escrow is blocked until the 30-day grace period has passed,
        // so it cannot be used to block a rating.
        if is_compacted(&env, id) {
            return Err(EscrowError::EscrowCompacted);
        }
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
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
        let weight = c.released_amount;
        if weight < MIN_REPUTATION_VALUE {
            return Err(EscrowError::BelowReputationThreshold);
        }
        let key = DataKey::Reputation(c.freelancer.clone());
        let mut rep: Reputation = env.storage().persistent().get(&key).unwrap_or_default();
        rep.completed_contracts += 1;
        rep.total_rating += rating;
        env.storage().persistent().set(&key, &rep);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);

        let wkey = DataKey::WeightedRep(c.freelancer.clone());
        let mut wrep: WeightedReputation =
            env.storage().persistent().get(&wkey).unwrap_or_default();
        wrep.weighted_rating_sum = weight
            .checked_mul(rating as i128)
            .and_then(|w| wrep.weighted_rating_sum.checked_add(w))
            .ok_or(EscrowError::ArithmeticOverflow)?;
        wrep.weight_total = wrep
            .weight_total
            .checked_add(weight)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        wrep.rated_contracts = wrep
            .rated_contracts
            .checked_add(1)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().persistent().set(&wkey, &wrep);
        env.storage()
            .persistent()
            .extend_ttl(&wkey, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        let weight_key = DataKey::RatingWeight(id);
        env.storage().persistent().set(&weight_key, &weight);
        env.storage()
            .persistent()
            .extend_ttl(&weight_key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);

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
            .publish((symbol_short!("rep"), id), (c.freelancer, rating, weight));
        Ok(())
    }

    /// After completion or resolution the freelancer may rate the client (1..=5) once per contract.
    pub fn rate_client(env: Env, id: u64, rating: u32) -> Result<(), EscrowError> {
        require_not_paused(&env)?;
        let c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        c.freelancer.require_auth();
        if !matches!(c.status, ContractStatus::Completed | ContractStatus::Resolved) {
            return Err(EscrowError::InvalidStatus);
        }
        let rated_key = DataKey::ClientRated(id);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&rated_key)
            .unwrap_or(false)
        {
            return Err(EscrowError::ReputationAlreadyIssued);
        }
        if !(1..=5).contains(&rating) {
            return Err(EscrowError::InvalidRating);
        }
        let rep_key = DataKey::ClientReputation(c.client.clone());
        let mut rep: Reputation = env.storage().persistent().get(&rep_key).unwrap_or_default();
        rep.completed_contracts = rep
            .completed_contracts
            .checked_add(1)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        rep.total_rating = rep
            .total_rating
            .checked_add(rating)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().persistent().set(&rep_key, &rep);
        env.storage()
            .persistent()
            .extend_ttl(&rep_key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        env.storage().persistent().set(&rated_key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&rated_key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        env.events()
            .publish((symbol_short!("rep_cli"), id), (c.client, rating));
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

        // Reverse the value-weighted contribution as well (ratings issued before
        // weighting existed have no stored weight and nothing to reverse).
        if let Some(weight) = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::RatingWeight(id))
        {
            let wkey = DataKey::WeightedRep(record.freelancer.clone());
            let mut wrep: WeightedReputation =
                env.storage().persistent().get(&wkey).unwrap_or_default();
            let contribution = weight.saturating_mul(record.rating as i128);
            wrep.weighted_rating_sum = wrep.weighted_rating_sum.saturating_sub(contribution).max(0);
            wrep.weight_total = wrep.weight_total.saturating_sub(weight).max(0);
            wrep.rated_contracts = wrep.rated_contracts.saturating_sub(1);
            env.storage().persistent().set(&wkey, &wrep);
            env.storage()
                .persistent()
                .extend_ttl(&wkey, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        }

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
        if is_compacted(&env, id) {
            return Err(EscrowError::EscrowCompacted);
        }
        load(&env, id)
    }

    /// Returns a single milestone; `MilestoneNotFound` when out of range.
    pub fn get_milestone(env: Env, id: u64, index: u32) -> Result<Milestone, EscrowError> {
        load(&env, id)?
            .milestones
            .get(index)
            .ok_or(EscrowError::MilestoneNotFound)
    }

    /// Returns the status and derived balances of one escrow. Read-only.
    pub fn get_escrow_summary(env: Env, id: u64) -> Result<EscrowSummary, EscrowError> {
        Ok(summarize(&load(&env, id)?))
    }

    /// Returns up to `MAX_SUMMARY_BATCH` summaries; unknown ids are skipped and
    /// larger batches return `InvalidAmount`. Read-only.
    pub fn get_escrow_summaries(
        env: Env,
        ids: Vec<u64>,
    ) -> Result<Vec<EscrowSummary>, EscrowError> {
        if ids.len() > MAX_SUMMARY_BATCH {
            return Err(EscrowError::InvalidAmount);
        }
        let mut result = Vec::new(&env);
        for id in ids.iter() {
            if let Ok(c) = load(&env, id) {
                result.push_back(summarize(&c));
            }
        }
        Ok(result)
    }

    /// Compacts a terminal escrow into a small receipt to reduce storage rent.
    /// Permissionless. `Cancelled` escrows can be compacted at any time;
    /// `Completed` and `Resolved` escrows (which can still be rated) only once
    /// the rating was issued or `THIRTY_DAYS_IN_LEDGERS` after they closed.
    pub fn compact_escrow(env: Env, id: u64) -> Result<(), EscrowError> {
        if is_compacted(&env, id) {
            return Err(EscrowError::NotCompactable);
        }
        let c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        let now = u64::from(env.ledger().sequence());
        let closed_at: Option<u64> = env.storage().persistent().get(&DataKey::ClosedAt(id));
        match c.status {
            ContractStatus::Cancelled => {}
            ContractStatus::Completed | ContractStatus::Resolved => {
                if !c.reputation_issued {
                    let closed = closed_at.ok_or(EscrowError::NotCompactable)?;
                    let unlock = closed
                        .checked_add(u64::from(THIRTY_DAYS_IN_LEDGERS))
                        .ok_or(EscrowError::ArithmeticOverflow)?;
                    if now < unlock {
                        return Err(EscrowError::NotCompactable);
                    }
                }
            }
            _ => return Err(EscrowError::NotCompactable),
        }

        let receipt = EscrowReceipt {
            id: c.id,
            client: c.client,
            freelancer: c.freelancer,
            token: c.token,
            total_amount: c.total_amount,
            released_amount: c.released_amount,
            refunded_amount: c.refunded_amount,
            status: c.status,
            milestone_count: c.milestones.len(),
            closed_at: closed_at.unwrap_or(now),
            reputation_issued: c.reputation_issued,
        };
        let receipt_key = DataKey::Receipt(id);
        env.storage().persistent().set(&receipt_key, &receipt);
        env.storage().persistent().extend_ttl(
            &receipt_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );
        env.storage().persistent().remove(&DataKey::Contract(id));
        env.storage().persistent().remove(&DataKey::ClosedAt(id));
        env.events().publish((symbol_short!("compact"), id), id);
        Ok(())
    }

    /// Returns the compact receipt for an escrow if it has been compacted.
    pub fn get_receipt(env: Env, id: u64) -> Option<EscrowReceipt> {
        env.storage().persistent().get(&DataKey::Receipt(id))
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

    pub fn get_weighted_reputation(env: Env, freelancer: Address) -> WeightedReputation {
        env.storage()
            .persistent()
            .get(&DataKey::WeightedRep(freelancer))
            .unwrap_or_default()
    }

    /// Value-weighted average rating scaled by 100 (e.g. `437` = 4.37), rounded
    /// half up; `None` when the freelancer has no eligible ratings.
    pub fn get_reputation_score(env: Env, freelancer: Address) -> Option<u32> {
        let w: WeightedReputation = env
            .storage()
            .persistent()
            .get(&DataKey::WeightedRep(freelancer))?;
        reputation_score(w.weighted_rating_sum, w.weight_total)
    }

    /// Grant a role to an account. Admin only; `RoleUnchanged` if already a member.
    pub fn grant_role(env: Env, role: Role, account: Address) -> Result<(), EscrowError> {
        set_role(&env, role, account, true)
    }

    /// Revoke a role from an account. Admin only; `RoleUnchanged` if not a member.
    pub fn revoke_role(env: Env, role: Role, account: Address) -> Result<(), EscrowError> {
        set_role(&env, role, account, false)
    }

    /// Role membership read; the admin implicitly holds every role.
    pub fn has_role(env: Env, role: Role, account: Address) -> bool {
        holds_role(&env, role, &account)
    }

    /// Returns the current arbiter panel, if one has been set.
    pub fn get_panel(env: Env) -> Option<Panel> {
        env.storage().instance().get(&DataKey::Panel)
    }

    /// Returns the current votes for a disputed escrow (member -> freelancer_bps).
    /// Votes are cleared once the escrow settles.
    pub fn get_votes(env: Env, id: u64) -> Map<Address, u32> {
        env.storage()
            .persistent()
            .get(&DataKey::Votes(id))
            .unwrap_or_else(|| Map::new(&env))
    }

    pub fn get_client_reputation(env: Env, client: Address) -> Reputation {
        env.storage()
            .persistent()
            .get(&DataKey::ClientReputation(client))
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
