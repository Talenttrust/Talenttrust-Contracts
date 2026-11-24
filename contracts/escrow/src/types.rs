//! Escrow contract types.
//! Public re-exported from the parent `lib.rs` module.

use soroban_sdk::{
    contracttype, Address, Map, Symbol, Vec, BytesN,
};

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

/// Versioned storage envelope for `EscrowContract`. Every new layout adds a
/// variant; `upgrade_to_latest` is the single place converting old variants.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoredEscrow {
    V1(EscrowContract),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpgrade {
    /// The WASM hash that will be installed.
    pub wasm_hash: BytesN<32>,
    /// The earliest ledger timestamp at which `execute_upgrade` may succeed.
    pub eta: u64,
}

/// Schema version carried as the first topic of every structured event.
pub const EVENT_SCHEMA_V: u32 = 1;

/// `(1, "created", id)` — a new escrow was created.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedEvent {
    pub client: Address,
    pub freelancer: Address,
    pub token: Address,
    pub total: i128,
    pub milestone_count: u32,
}

/// `(1, "deposit", id)` — tokens were deposited into an escrow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositEvent {
    pub from: Address,
    pub amount: i128,
    pub funded_amount: i128,
    pub status: ContractStatus,
}

/// `(1, "release", id)` — a milestone was paid out.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseEvent {
    pub index: u32,
    pub amount: i128,
    pub to: Address,
    pub released_amount: i128,
    pub status: ContractStatus,
    /// Protocol fee deducted from `amount` (sent to the treasury).
    pub fee: i128,
}

/// `(1, "dispute", id)` — a party opened a dispute.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeEvent {
    pub by: Address,
    pub status: ContractStatus,
    pub bond: i128,
}

/// `(1, "resolved", id)` — a dispute was settled.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedEvent {
    pub to_freelancer: i128,
    pub to_client: i128,
    pub status: ContractStatus,
    pub bond_recipient: Option<Address>,
    pub bond: i128,
}

/// `(1, "cancel", id)` — the client cancelled; `refund` went back to the
/// funders (the client, or pro-rata to sponsors, see `get_funders`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelEvent {
    pub refund: i128,
    pub to: Address,
    pub status: ContractStatus,
}

/// `(1, "rep", id)` — the client rated the freelancer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationEvent {
    pub freelancer: Address,
    pub rating: u32,
    pub completed_contracts: u32,
    pub total_rating: u32,
    pub weight: i128,
}

/// `(1, "complete", id)` — the last milestone was released.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedEvent {
    pub released_amount: i128,
}

/// `(1, "milestone_dispute", id, index, caller, remaining, bond)` — a dispute was opened for a specific milestone.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneDisputeEvent {
    pub id: u64,
    pub index: u32,
    pub caller: Address,
    pub remaining: i128,
    pub bond: i128,
}

/// `(1, "milestone_resolved", id, index, freelancer_bps, to_freelancer, to_client, bond, recipient)` — a milestone-scoped dispute was resolved.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneResolvedEvent {
    pub id: u64,
    pub index: u32,
    pub freelancer_bps: u32,
    pub to_freelancer: i128,
    pub to_client: i128,
    pub bond: i128,
    pub recipient: Address,
}

/// `(1, "claimable", id, token, address, amount)` — a claimable balance was credited or claimed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimableEvent {
    pub id: u64,
    pub token: Address,
    pub address: Address,
    pub amount: i128,
}

/// A pending two-party mutual settlement proposal.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementProposal {
    pub proposer: Address,
    pub freelancer_amount: i128,
    pub created_at: u64,
}

/// Linear vesting schedule of a stream escrow (see `create_stream`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    pub start: u64,
    pub end: u64,
    pub withdrawn: i128,
    pub stopped_at: Option<u64>,
}

/// A pending mutual amendment of milestone amounts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Amendment {
    pub proposer: Address,
    pub new_amounts: Vec<i128>,
    pub nonce: u32,
}

/// Minimum stream duration: 1 hour.
pub const MIN_STREAM_SECS: u64 = 3_600;

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

/// Per-milestone lifecycle state (`get_milestone_status`). Tracked beside the
/// stored `Milestone` (whose `released` flag stays authoritative) so the stored
/// escrow layout does not change.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    /// Not submitted yet.
    Pending,
    /// Freelancer submitted; awaiting approval or review-window expiry.
    Submitted,
    /// Paid to the freelancer.
    Released,
}

/// An in-flight upgrade request placed by the admin.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmendNonce(u64);

/// Converts any historical `StoredEscrow` variant into the current `EscrowContract`.
pub fn upgrade_to_latest(s: StoredEscrow) -> EscrowContract {
    match s {
        StoredEscrow::V1(c) => c,
    }
}

/// Current pause bitmask. Backwards compatible: with no stored flags, a
/// legacy `Paused == true` means `PAUSE_ALL`.
pub fn pause_flags(env: &soroban_sdk::Env) -> u32 {
    if let Some(flags) = env.storage().instance().get::<_, u32>(super::constants::DATA_KEY_PAUSE_FLAGS) {
        return flags;
    }
    if env
        .storage()
        .instance()
        .get::<_, bool>(super::constants::DATA_KEY_PAUSED)
        .unwrap_or(false)
    {
        super::constants::PAUSE_ALL
    } else {
        0
    }
}

/// Stores the pause bitmask (and the legacy boolean), starting the
/// emergency-exit clock on unpaused -> paused and clearing it on full unpause.
pub fn write_pause_flags(env: &soroban_sdk::Env, flags: u32) {
    let was_paused = pause_flags(env) != 0;
    env.storage().instance().set(&super::constants::DATA_KEY_PAUSE_FLAGS, &flags);
    env.storage().instance().set(&super::constants::DATA_KEY_PAUSED, &(flags != 0));
    if flags != 0 && !was_paused {
        env.storage().instance().set(&super::constants::DATA_KEY_PAUSED_AT, &env.ledger().timestamp());
    } else if flags == 0 {
        env.storage().instance().remove(&super::constants::DATA_KEY_PAUSED_AT);
    }
}

/// Fails with `Paused` when the operation category `op` is paused.
pub fn require_not_paused(env: &soroban_sdk::Env, op: u32) -> Result<(), super::errors::EscrowError> {
    if pause_flags(env) & op != 0 {
        return Err(super::errors::EscrowError::Paused);
    }
    // Every gated (mutating) entry point keeps the instance alive.
    soroban_sdk::bump_instance(env);
    Ok(())
}

/// Loads an escrow: the versioned `ContractV` entry first, falling back to the
/// legacy `Contract` key. Never writes; the next `save` performs the lazy migration.
pub fn load(env: &soroban_sdk::Env, id: u64) -> Result<EscrowContract, super::errors::EscrowError> {
    if let Some(stored) = env.storage().persistent().get::<_, super::types::StoredEscrow>(&super::types::DataKey::ContractV(id)) {
        return Ok(super::types::upgrade_to_latest(stored));
    }
    env.storage().persistent().get(&DataKey::Contract(id)).ok_or(super::errors::EscrowError::ContractNotFound)
}

/// Whether an escrow entry exists under either the versioned or the legacy key.
pub fn escrow_exists(env: &soroban_sdk::Env, id: u64) -> bool {
    env.storage().persistent().has(&DataKey::ContractV(id))
        || env.storage().persistent().has(&DataKey::Contract(id))
}

/// Summarizes an escrow contract into a read-only summary.
pub fn summarize(c: &EscrowContract) -> EscrowSummary {
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

/// Writes the newest envelope variant under `ContractV` (TTL bumped on the new
/// key only) and drops any legacy entry: this is the lazy migration.
pub fn save(env: &soroban_sdk::Env, c: &EscrowContract) {
    let key = super::types::DataKey::ContractV(c.id);
    env.storage().persistent().set(&key, &super::types::StoredEscrow::V1(c.clone()));
    env.storage().persistent().extend_ttl(&key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
    let legacy = DataKey::Contract(c.id);
    if env.storage().persistent().has(&legacy) {
        env.storage().persistent().remove(&legacy);
    }
    soroban_sdk::bump_instance(env);
}

/// Extends a persistent entry's TTL if it exists.
pub fn extend_if_present(env: &soroban_sdk::Env, key: &super::types::DataKey) -> bool {
    if env.storage().persistent().has(key) {
        env.storage().persistent().extend_ttl(key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        true
    } else {
        false
    }
}

/// Bumps the instance TTL.
pub fn bump_instance(env: &soroban_sdk::Env) {
    env.storage().instance().extend_ttl(constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
}

/// Transfers `amount` and, whenever the contract is on either side, verifies
/// its token balance moved by exactly `amount`. Fee-on-transfer, rebasing or
/// misreporting tokens would otherwise let recorded balances drift above what
/// the contract holds, and later payouts would spend other escrows' tokens.
/// Any mismatch fails with `TransferAmountMismatch`, reverting the whole call.
pub fn transfer_checked(
    env: &soroban_sdk::Env,
    token_addr: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), super::errors::EscrowError> {
    if amount <= 0 {
        return Ok(());
    }
    let this = env.current_contract_address();
    let client = soroban_sdk::token::Client::new(env, token_addr);
    let touches_contract = *from == this || *to == this;
    let before = if touches_contract {
        client.balance(&this)
    } else {
        0
    };
    client.transfer(from, to, &amount);
    if touches_contract {
        let after = client.balance(&this);
        let delta = if *to == this {
            after.checked_sub(before)
        } else {
            before.checked_sub(after)
        }
        .ok_or(super::errors::EscrowError::TransferAmountMismatch)?;
        if delta != amount {
            return Err(super::errors::EscrowError::TransferAmountMismatch);
        }
    }
    Ok(())
}

/// Liability helper functions: aggregate per-token amount the contract owes to live escrows.
pub fn liability_of(env: &soroban_sdk::Env, token: &Address) -> i128 {
    env.storage().persistent().get(&DataKey::Liability(token.clone())).unwrap_or(0)
}

pub fn set_liability(env: &soroban_sdk::Env, token: &Address, value: i128) {
    let key = DataKey::Liability(token.clone());
    env.storage().persistent().set(&key, &value);
    env.storage().persistent().extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

pub fn add_liability(env: &soroban_sdk::Env, token: &Address, amount: i128) -> Result<(), super::errors::EscrowError> {
    let updated = liability_of(env, token)
        .checked_add(amount)
        .ok_or(super::errors::EscrowError::ArithmeticOverflow)?;
    set_liability(env, token, updated);
    Ok(())
}

pub fn sub_liability(env: &soroban_sdk::Env, token: &Address, amount: i128) -> Result<(), super::errors::EscrowError> {
    let updated = liability_of(env, token)
        .checked_sub(amount)
        .ok_or(super::errors::EscrowError::ArithmeticOverflow)?;
    set_liability(env, token, updated);
    Ok(())
}

/// Solvency invariant (test + debug only): the contract's token balance covers all liabilities.
#[cfg(any(test, debug_assertions))]
pub fn assert_solvent(env: &soroban_sdk::Env, token: &Address) {
    let balance = soroban_sdk::token::Client::new(env, token).balance(&env.current_contract_address());
    let liability = liability_of(env, token);
    assert!(
        balance >= liability,
        "Solvency invariant violated: balance {} < liability {}", balance, liability
    );
}

/// Returns the address that should receive freelancer payouts for `c`.
/// If the freelancer has set a dedicated payout address via
/// [`Escrow::set_payout_address`] it is returned; otherwise payouts go to the
/// freelancer's own address (`c.freelancer`). Reputation always accrues to
/// `c.freelancer` regardless of the payout address.
pub fn payee(env: &soroban_sdk::Env, c: &EscrowContract) -> Address {
    env.storage().persistent().get(&DataKey::Payout(c.id)).unwrap_or_else(|| c.freelancer.clone())
}

/// Returns the team split for escrow `id`, or an empty vec if none is configured.
pub fn team_split(env: &soroban_sdk::Env, id: u64) -> Vec<(Address, u32)> {
    env.storage().persistent().get(&DataKey::Team(id)).unwrap_or_else(|| Vec::new(env))
}

/// DataKey storage keys for the escrow contract. Order must be preserved for
/// storage serialization compatibility. New variants are appended at the end.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    SettlementToken,
    ContractV(u64),
    Contract(u64),
    FundBy(u64),
    Funders(u64),
    Frozen(u64),
    Payout(u64),
    Team(u64),
    ClosedAt(u64),
    Sequential(u64),
    Terms(u64),
    MilestoneSpecs(u64),
    DisputeBond(u64),
    Outcomes(Address),
    Votes(u64),
    Settlement(u64),
    ReviewWindow(u64),
    EscrowFee(u64),
    Bonus(u64),
    Liability(Address),
    Reputation(Address),
    WeightedRep(Address),
    ClientRated(Address),
    ClientReputation(Address),
    Rating(u64),
    RatingWeight(u64),
    Stream(u64),
    Amendment(u64),
    AmendNonce(u64),
    // New: milestone-scoped dispute tracking
    MilestoneDispute(u64, u32),
    // New: claimable payout fallback
    Claimable(Address, Address),
}

/// Extends a persistent entry's TTL if it exists, and bumps the instance.
pub fn extend_escrow_ttl(env: &soroban_sdk::Env, id: u64) -> Result<(), super::errors::EscrowError> {
    let mut found = false;
    // Try versioned key first
    found |= extend_if_present(env, &super::types::DataKey::ContractV(id));
    // Try legacy key
    found |= extend_if_present(env, &DataKey::Contract(id));
    // Try receipt
    found |= extend_if_present(env, &DataKey::Receipt(id));
    if !found {
        return Err(super::errors::EscrowError::ContractNotFound);
    }
    // Extend all associated keys
    let keys = [
        DataKey::FundBy(id),
        DataKey::Funders(id),
        DataKey::Frozen(id),
        DataKey::Payout(id),
        DataKey::Team(id),
        DataKey::ClosedAt(id),
        DataKey::Sequential(id),
        DataKey::Terms(id),
        DataKey::MilestoneSpecs(id),
        DataKey::DisputeBond(id),
        DataKey::Settlement(id),
        DataKey::ReviewWindow(id),
        DataKey::EscrowFee(id),
        DataKey::Bonus(id),
        DataKey::Votes(id),
        DataKey::ChangeOrder(id),
        DataKey::Resigned(id),
        DataKey::Rating(id),
        DataKey::RatingWeight(id),
        DataKey::ClientRated(id),
        DataKey::Stream(id),
        DataKey::Amendment(id),
        DataKey::AmendNonce(id),
    ];
    for key in keys.iter() {
        extend_if_present(env, key);
    }
    // Milestone-specific keys
    if let Ok(c) = load(env, id) {
        for i in 0..c.milestones.len() {
            extend_if_present(env, &DataKey::Submitted(id, i));
            extend_if_present(env, &DataKey::Delivery(id, i));
            extend_if_present(env, &DataKey::MilestonePaid(id, i));
        }
    }
    bump_instance(env);
    Ok(())
}

pub fn extend_reputation_ttl(env: &soroban_sdk::Env, freelancer: Address) {
    let keys = [
        DataKey::Reputation(freelancer.clone()),
        DataKey::WeightedRep(freelancer.clone()),
        DataKey::ClientReputation(freelancer.clone()),
        DataKey::Outcomes(freelancer.clone()),
        DataKey::BonusTotal(freelancer),
    ];
    for key in keys.iter() {
        extend_if_present(env, key);
    }
    bump_instance(env);
}

/// Returns `true` if the escrow is in a terminal status (no further mutations allowed).
pub fn is_terminal(status: ContractStatus) -> bool {
    matches!(status, ContractStatus::Completed | ContractStatus::Resolved | ContractStatus::Cancelled)
}

/// Returns the address that should receive freelancer payouts for `c`.
/// If the freelancer has set a dedicated payout address via
/// [`Escrow::set_payout_address`] it is returned; otherwise payouts go to the
/// freelancer's own address (`c.freelancer`). Reputation always accrues to
/// `c.freelancer` regardless of the payout address.
pub fn payee(env: &soroban_sdk::Env, c: &EscrowContract) -> Address {
    env.storage().persistent().get(&DataKey::Payout(c.id)).unwrap_or_else(|| c.freelancer.clone())
}

/// Returns the team split for escrow `id`, or an empty vec if none is configured.
pub fn team_split(env: &soroban_sdk::Env, id: u64) -> Vec<(Address, u32)> {
    env.storage().persistent().get(&DataKey::Team(id)).unwrap_or_else(|| Vec::new(env))
}

/// DataKey storage keys for the escrow contract. Order must be preserved for
/// storage serialization compatibility. New variants are appended at the end.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    SettlementToken,
    ContractV(u64),
    Contract(u64),
    FundBy(u64),
    Funders(u64),
    Frozen(u64),
    Payout(u64),
    Team(u64),
    ClosedAt(u64),
    Sequential(u64),
    Terms(u64),
    MilestoneSpecs(u64),
    DisputeBond(u64),
    Outcomes(Address),
    Votes(u64),
    Settlement(u64),
    ReviewWindow(u64),
    EscrowFee(u64),
    Bonus(u64),
    Liability(Address),
    Reputation(Address),
    WeightedRep(Address),
    ClientRated(Address),
    ClientReputation(Address),
    Rating(u64),
    RatingWeight(u64),
    Stream(u64),
    Amendment(u64),
    AmendNonce(u64),
    // New: milestone-scoped dispute tracking
    MilestoneDispute(u64, u32),
    // New: claimable payout fallback
    Claimable(Address, Address),
}

// ========== NEW TYPED EVENTS (Issue #1789) ==========

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementProposedEvent {
    pub proposer: Address,
    pub freelancer_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementAcceptedEvent {
    pub to_freelancer: i128,
    pub to_client: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementWithdrawnEvent {
    pub proposer: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmendmentProposedEvent {
    pub proposer: Address,
    pub nonce: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmendmentAcceptedEvent {
    pub old_total: i128,
    pub new_total: i128,
    pub refunded: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeOrderProposedEvent {
    pub added_count: u32,
    pub added_total: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeOrderAcceptedEvent {
    pub added_count: u32,
    pub added_total: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeOrderRejectedEvent {}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResignEvent {
    pub refund: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpiredEvent {
    pub refund: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyWithdrawEvent {
    pub to_client: i128,
    pub to_freelancer: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamWithdrawEvent {
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamStoppedEvent {
    pub to_freelancer: i128,
    pub to_client: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactEvent {
    pub id: u64,
    }

// ---------- Typed admin/governance events ----------

/// `(1, "fee_sched", id)` – admin schedules a protocol fee change.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeSchedEvent {
    pub fee_bps: u32,
    pub treasury: Address,
    pub eta: u64,
}

/// `(1, "fee_cncl", id)` – admin cancels a scheduled fee change.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeCnclEvent {
    pub fee_bps: u32,
    pub treasury: Address,
}

/// `(1, "paused", id)` – pause state change.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseEvent {
    pub paused: bool,
}

/// `(1, "role_set", id)` – admin changes a role.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleSetEvent {
    pub role: u32,
    pub address: Address,
}

/// `(1, "panel", id)` – panel configuration event.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanelEvent {
    pub threshold: i128,
    pub members: Vec<Address>,
}

/// `(1, "upg_sched", id)` – admin schedules a wasm upgrade.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeSchedEvent {
    pub wasm_hash: BytesN<32>,
    pub eta: u64,
}

/// `(1, "upg_cncl", id)` – admin cancels a scheduled upgrade.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeCnclEvent {
    pub wasm_hash: BytesN<32>,
}

/// `(1, "upgraded", id)` – upgrade executed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradedEvent {
    pub wasm_hash: BytesN<32>,
}

/// `(1, "sweep", id)` – admin sweeps contract balance.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SweepEvent {
    pub to: Address,
    pub amount: i128,
}

/// Generic admin property change.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminPropEvent {
    pub admin: Address,
    pub prop: u32,
}

/// Admin access change.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminAccEvent {
    pub admin: Address,
    pub access: bool,
}

/// Admin cancel event (legacy).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminCnclEvent {
    pub admin: Address,
    pub reason: Symbol,
}

// ------------------------------------------------------
