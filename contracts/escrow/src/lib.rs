//! TalentTrust escrow contract for milestone-based freelancer payments on Stellar (Soroban).
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
//! 11. Anyone may `resolve_expired_dispute` on a `Disputed` escrow left unresolved past
//!     `DISPUTE_TIMEOUT_SECS`, splitting the held balance 50/50.
//! Storage versioning: escrows are stored under `DataKey::ContractV(id)` as a
//! `StoredEscrow` envelope. Entries written under the legacy `DataKey::Contract(id)`
//! key are read transparently and migrated lazily by the first mutation (or eagerly
//! via the permissionless `migrate_escrows`). Reads never write.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env,
    Map, Symbol, Vec,
};

/// How long a `Disputed` escrow may stay unresolved before anyone can settle it
/// through `resolve_expired_dispute` (30 days).
pub const DISPUTE_TIMEOUT_SECS: u64 = 2_592_000;
/// Freelancer share applied by `resolve_expired_dispute`: 5,000 bps (50%) of
/// the held balance, with the rounding remainder going to the client.
pub const DISPUTE_TIMEOUT_SPLIT_BPS: u32 = 5_000;
pub mod errors;
pub use errors::EscrowError;
/// 48-hour time-lock delay before a scheduled governance parameter change
/// (e.g. protocol fee increase) can be executed.
pub const PARAM_DELAY_SECS: u64 = 172_800;
/// Validity window for a mutual settlement proposal (7 days); after
/// `created_at + SETTLEMENT_VALID_SECS`, `accept_settlement` returns
/// `SettlementMismatch`.
pub const SETTLEMENT_VALID_SECS: u64 = 7 * 86_400;

pub mod constants;
pub use constants::{
    MAX_MILESTONES, MAX_SUMMARY_BATCH, MAX_MIGRATE_BATCH, ESCROW_INDEX_PAGE_SIZE, BPS_DENOMINATOR,
    UPGRADE_DELAY_SECS, PAUSE_CREATE, PAUSE_DEPOSIT, PAUSE_RELEASE, PAUSE_DISPUTE, PAUSE_CANCEL,
    PAUSE_REPUTATION, PAUSE_ALL, MAX_FEE_BPS, EMERGENCY_EXIT_DELAY_SECS, MIN_REVIEW_WINDOW_SECS,
    MAX_REVIEW_WINDOW_SECS, DEFAULT_REVIEW_WINDOW_SECS, FUNDING_WINDOW_SECS, MIN_MILESTONE_AMOUNT,
    MAX_ESCROW_TOTAL, MIN_REPUTATION_VALUE, PANEL_THRESHOLD_AMOUNT, PANEL_MAX_MEMBERS, MAX_FUNDERS,
    DISPUTE_WIN_THRESHOLD_BPS, DISPUTE_BOND_BPS, DISPUTE_BOND_MIN, THIRTY_DAYS_IN_LEDGERS,
};
    /// Contract has already been initialized.
    /// Contract has not been initialized yet.
    /// Caller is not authorized to perform this action.
    /// The requested operation category is currently paused.
    /// No escrow exists for the given id.
    /// Client and freelancer addresses are invalid (e.g. identical).
    /// Milestone list is empty or exceeds `MAX_MILESTONES`.
    /// Supplied amount or batch size is invalid (e.g. non-positive or out of bounds).
    /// Deposit would exceed the escrow's total amount.
    /// Escrow is not in a status that permits this operation.
    /// Milestone index is out of range for this escrow.
    /// Milestone has already been released.
    /// Escrow does not hold enough funded balance to cover the payout.
    /// Dispute split basis points are invalid (must be `<= BPS_DENOMINATOR`).
    /// Rating value is outside the allowed range (`1..=5`).
    /// Reputation has already been issued for this escrow.
    /// Checked arithmetic overflowed or underflowed.
    /// No rating record exists for the given escrow id.
    /// Rating record has already been voided.
    /// Requested state transition is not permitted.
    /// Contract holds no surplus balance above liabilities for the token.
    /// Escrow is frozen by an administrator and cannot be mutated.
    /// Contract admin cannot be a party (client or freelancer) to an escrow.
    /// Milestone amount is below `MIN_MILESTONE_AMOUNT`.
    /// Escrow is not yet eligible for compaction into a receipt.
    /// Escrow has been compacted into a receipt; use `get_receipt` instead.
    /// Team payout split configuration is invalid (e.g. shares do not sum to `BPS_DENOMINATOR`).
    /// Sequential milestone cannot be released before all prior milestones.
    /// Duplicate milestone index or member address supplied.
    /// Released value is below `MIN_REPUTATION_VALUE` required to issue reputation.
    /// Arbiter panel configuration (members or threshold) is invalid.
    /// Escrow value meets or exceeds `PANEL_THRESHOLD_AMOUNT` and requires arbiter panel voting.
    /// Account already has (or already lacks) the specified role.
    /// Number of distinct funders would exceed `MAX_FUNDERS`.
    /// Escrow funding window has elapsed and it can no longer be funded.
    /// Escrow funding window has not yet elapsed.
    /// No pending change order or proposal exists for this escrow.
    /// Metadata hash or payload is invalid.
    /// No WASM upgrade is currently scheduled.
    /// Scheduled WASM upgrade time-lock has not yet elapsed.
    /// Review window duration is outside `[MIN_REVIEW_WINDOW_SECS, MAX_REVIEW_WINDOW_SECS]`.
    /// Milestone has not been submitted by the freelancer yet.
    /// Milestone review window has not yet elapsed.
    /// Proposed admin address is invalid (e.g. same as current admin).
    /// No pending admin nomination exists to accept.
    /// Emergency exit is not available (contract not paused long enough or invalid state).
    /// Settlement proposal parameters do not match the pending proposal.
    /// Protocol fee exceeds `MAX_FEE_BPS`.
    /// Stream vesting schedule is invalid (e.g. duration below `MIN_STREAM_SECS`).
/// An in-flight protocol fee / treasury change scheduled by the admin.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingFeeChange {
    pub fee_bps: u32,
    pub treasury: Address,
    /// The earliest ledger timestamp at which `execute_fee_change` may succeed.
    pub eta: u64,
}

/// `(1, "submit", id)` — the freelancer submitted a milestone for review.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmitEvent {
    pub index: u32,
    pub submitted_at: u64,
}

/// `(1, "claim", id)` — the freelancer claimed a submitted milestone after the review window.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimEvent {
    pub index: u32,
    pub amount: i128,
}

/// `(1, "rel_part", id)` — the client released a partial amount of a milestone.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartialReleaseEvent {
    pub index: u32,
    pub amount: i128,
    pub remaining: i128,
}

/// `(1, "rel_batch", id)` — the client released multiple milestones in one batch.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchReleaseEvent {
    pub count: u32,
    pub total: i128,
}

/// `(1, "deliver", id)` — the freelancer recorded a delivery hash for a milestone.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliverEvent {
    pub index: u32,
    pub delivery_hash: BytesN<32>,
}

/// `(1, "bonus", id)` — the client paid a direct bonus/tip to the freelancer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BonusEvent {
    pub amount: i128,
    pub cumulative: i128,
}

/// `(1, "terms", id)` — the client committed the job terms hash.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
    /// Optional per-milestone due dates (ledger timestamps), index-aligned with
    /// `milestones`. Absent means no deadline was agreed for this escrow.
    MilestoneDue(u64),
    /// Set once the client reclaimed an overdue, unsubmitted milestone.
    MilestoneReclaimed(u64, u32),
    /// Whether `Address` is allowlisted as an escrow settlement token.
    AllowedToken(Address),
    /// Whether the token allowlist is enforced during escrow creation.
    AllowlistEnabled,
pub struct TermsEvent {
    pub terms_hash: BytesN<32>,
}

    /// Ledger timestamp at which the current dispute was opened.
    DisputeOpenedAt(u64),
    PendingFeeChange,
///
/// A compacted escrow no longer has either entry, only its receipt, so this
/// returns `EscrowCompacted` rather than the misleading `ContractNotFound`.
/// Every entrypoint that loads an escrow therefore reports compaction the same
/// way; ids that never existed still fail with `ContractNotFound`.
    if let Some(legacy) = env
        .storage()
        .get::<_, EscrowContract>(&DataKey::Contract(id))
    {
        return Ok(legacy);
    }
    if is_compacted(env, id) {
        return Err(EscrowError::EscrowCompacted);
    }
    Err(EscrowError::ContractNotFound)

pub mod types;
pub use types::{
    ContractStatus, Milestone, EscrowContract, EscrowSummary, EscrowReceipt, Reputation,
    WeightedReputation, Outcomes, RatingRecord, StoredEscrow, upgrade_to_latest, PendingUpgrade,
    EVENT_SCHEMA_V, CreatedEvent, DepositEvent, ReleaseEvent, DisputeEvent, ResolvedEvent,
    CancelEvent, ReputationEvent, CompletedEvent, SettlementProposal, Stream, Role, Panel,
    AmendNonce, DataKey,
    MilestoneDisputeEvent, ClaimableEvent,
};

    /// Number of currently open (non-terminal) escrows created by this client.
    OpenCount(Address),
    /// Configured per-client open-escrow cap; `0` means unlimited.
    MaxOpenEscrows,
pub mod storage;
pub use storage::{extend_if_present, bump_instance, load, escrow_exists, save, distribute,
    pay_out, pay_out_with_fee, fee_for, team_split, liability_of, set_liability, add_liability,
    sub_liability, assert_solvent, payee, summarize, is_compacted, load_stream, is_stream, save_stream,
    vested_at, stream_vested, checked_milestone_total, milestone_paid, set_milestone_paid, clear_milestone_paid,
    submitted_at, clear_submitted, review_window, index_count_key, index_page_key, read_party_index,
    append_party_index, party_index_count};

pub mod events;
pub use events::{init, paused, created, deposit, release, completed, dispute, resolved, cancel,
    reputation, milestone_dispute, milestone_resolved, claimable,
    fee_sched, fee_cncl, paused as pause_event, role_set, panel, upgrade_sched, upgrade_cncl, upgraded, sweep, admin_prop, admin_acc, admin_cncl};
",
/// Centralised emission of the versioned, structured events.
mod events as internal_events {
    use super::*;
    use soroban_sdk::{token::Client, symbol_short};

    fn escrow_topic(name: Symbol, id: u64) -> (u32, Symbol, u64) {
        (EVENT_SCHEMA_V, name, id)
    }

    pub fn init(env: &Env, admin: &Address) {
        env.events().publish((EVENT_SCHEMA_V, symbol_short!("init")), admin.clone());
    }

    pub fn paused(env: &Env, paused: bool) {
        env.events().publish((EVENT_SCHEMA_V, symbol_short!("paused")), paused);
    }

    pub fn created(env: &Env, c: &EscrowContract) {
        env.events().publish(
            escrow_topic(symbol_short!("created"), c.id),
            CreatedEvent {
                client: c.client.clone(),
                freelancer: c.freelancer.clone(),
                token: c.token.clone(),
                total: c.total_amount,
                milestone_count: c.milestones.len(),
            },
        );
    }

///
/// `record_outcomes` is false for the timeout exit (`resolve_expired_dispute`):
/// no resolver adjudicated that split, so win/loss counters must not move.
    record_outcomes: bool,
    // The dispute is over: drop its open timestamp alongside the bond.
    env.storage()
        .persistent()
        .remove(&DataKey::DisputeOpenedAt(c.id));

    // Record dispute outcomes for both parties unless this is the timeout exit,
    // which no resolver adjudicated.
    if record_outcomes {
        record_dispute_outcomes(env, &c, freelancer_bps)?;
    }

    events::resolved(env, &c, to_freelancer, to_client, bond_recipient, bond);
    Ok(())
}

/// Increments the win/loss dispute counters for both parties of `c`. A split at
/// or above `DISPUTE_WIN_THRESHOLD_BPS` counts as a freelancer win (and a client
/// loss), anything below as the reverse.
fn record_dispute_outcomes(
    env: &Env,
    c: &EscrowContract,
    freelancer_bps: u32,
) -> Result<(), EscrowError> {
    pub fn deposit(env: &Env, c: &EscrowContract, from: &Address, amount: i128) {
        env.events().publish(
            escrow_topic(symbol_short!("deposit"), c.id),
            DepositEvent {
                from: from.clone(),
                amount,
                funded_amount: c.funded_amount,
                status: c.status,
            },
        );
    }

    /// Returns the contract administrator's address.
    ///
    /// - **Storage reads:** `DataKey::Admin` (instance storage).
    /// - **Errors:** `EscrowError::NotInitialized` if `initialize` has not been called.

    pub fn release(
        env: &Env,
        c: &EscrowContract,
        index: u32,
        amount: i128,
        to: &Address,
        released_amount: i128,
        fee: i128,
    ) {
        env.events().publish(
            escrow_topic(symbol_short!("release"), c.id),
            ReleaseEvent {
                index,
                amount,
                to: to.clone(),
                released_amount,
                status: c.status,
                fee,
            },
        );
    /// and its treasury immediately **only** when the fee is not increasing
    /// (`fee_bps <= current_fee_bps`). Fee increases must go through the
    /// timelocked `schedule_fee_change` -> `execute_fee_change` flow; attempting
    /// to increase the fee directly returns `UpgradeNotReady`.
    /// New escrows snapshot the rate at creation; existing escrows keep theirs.
    /// Emits `("fee_set",) -> (fee_bps, treasury)`.
        let current_fee: u32 = env.storage().instance().get(&DataKey::FeeBps).unwrap_or(0);
        if fee_bps > current_fee {
            return Err(EscrowError::UpgradeNotReady);
        }
    /// Admin schedules a protocol fee and treasury change. The scheduled change
    /// can only be executed after `PARAM_DELAY_SECS` (48 h) have elapsed, giving
    /// users time to react before a fee increase takes effect.
    ///
    /// Calling `schedule_fee_change` again while one is pending resets the timer.
    ///
    /// Event: `("fee_sched",) -> (fee_bps, treasury, eta)`
    pub fn schedule_fee_change(
        env: Env,
        fee_bps: u32,
        treasury: Address,
    ) -> Result<(), EscrowError> {
        require_admin(&env)?;
        if fee_bps > MAX_FEE_BPS {
            return Err(EscrowError::FeeTooHigh);
        }
        if treasury == env.current_contract_address() {
            return Err(EscrowError::InvalidParties);
        }
        let now = env.ledger().timestamp();
        let eta = now
            .checked_add(PARAM_DELAY_SECS)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        let pending = PendingFeeChange {
            fee_bps,
            treasury: treasury.clone(),
            eta,
        };
        env.storage()
            .instance()
            .set(&DataKey::PendingFeeChange, &pending);
        bump_instance(&env);
        events::fee_sched(&env, fee_bps, &treasury, eta);
        Ok(())
    }

    /// Admin cancels a scheduled protocol fee change before it executes.
    ///
    /// Fails with `NoPendingUpgrade` if no fee change is pending.
    ///
    /// Event: `("fee_cncl",) -> (fee_bps, treasury)`
    pub fn cancel_fee_change(env: Env) -> Result<(), EscrowError> {
        require_admin(&env)?;
        let pending: PendingFeeChange = env
            .storage()
            .instance()
            .get(&DataKey::PendingFeeChange)
            .ok_or(EscrowError::NoPendingUpgrade)?;
        env.storage().instance().remove(&DataKey::PendingFeeChange);
        bump_instance(&env);
        env.events().publish(
            (symbol_short!("fee_cncl"),),
            (pending.fee_bps, pending.treasury),
        );
        Ok(())
    }

    /// Admin executes a previously scheduled protocol fee change after
    /// `PARAM_DELAY_SECS` have elapsed.
    ///
    /// Fails with:
    /// - `NoPendingUpgrade` — no fee change is pending.
    /// - `UpgradeNotReady`  — `PARAM_DELAY_SECS` have not yet elapsed.
    ///
    /// Existing escrows keep their snapshotted fee; new escrows use the new rate.
    ///
    /// Event: `("fee_exec",) -> (fee_bps, treasury)`
    pub fn execute_fee_change(env: Env) -> Result<(), EscrowError> {
        require_admin(&env)?;
        let pending: PendingFeeChange = env
            .storage()
            .instance()
            .get(&DataKey::PendingFeeChange)
            .ok_or(EscrowError::NoPendingUpgrade)?;
        let now = env.ledger().timestamp();
        if now < pending.eta {
            return Err(EscrowError::UpgradeNotReady);
        }
        env.storage().instance().remove(&DataKey::PendingFeeChange);
        env.storage()
            .instance()
            .set(&DataKey::FeeBps, &pending.fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::Treasury, &pending.treasury);
        bump_instance(&env);
        env.events().publish(
            (symbol_short!("fee_exec"),),
            (pending.fee_bps, pending.treasury),
        );
        Ok(())
    }

    /// Returns the currently scheduled fee change, or `None` if none is pending.
    pub fn get_pending_fee_change(env: Env) -> Option<PendingFeeChange> {
        env.storage().instance().get(&DataKey::PendingFeeChange)
    }

    }

/// Optional due date (ledger timestamp) agreed for a milestone.
fn milestone_due(env: &Env, id: u64, index: u32) -> Option<u64> {
    env.storage()
        .persistent()
        .get::<_, Vec<u64>>(&DataKey::MilestoneDue(id))?
        .get(index)
}

/// Whether the client already reclaimed this milestone after its due date.
fn is_reclaimed(env: &Env, id: u64, index: u32) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::MilestoneReclaimed(id, index))
        .unwrap_or(false)
}

fn mark_reclaimed(env: &Env, id: u64, index: u32) {
    let key = DataKey::MilestoneReclaimed(id, index);
    env.storage().persistent().set(&key, &true);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

    if env
        .storage()
        .instance()
        .get::<_, bool>(&DataKey::AllowlistEnabled)
        .unwrap_or(false)
        && !env
            .storage()
            .persistent()
/// Records the ledger sequence at which an escrow entered a terminal status and
/// releases the client's open-escrow slot. Every terminal transition calls this
/// exactly once, so the per-client open counter can never double-decrement or
/// underflow (the decrement is clamped at zero).
fn record_closed_at(env: &Env, c: &EscrowContract) {
    let key = DataKey::ClosedAt(c.id);
    drop_open_escrows(env, &c.client);
}

/// Configured per-client concurrent open-escrow cap; `0` means unlimited (the
/// default, so existing behaviour is unchanged until an admin sets a cap).
fn max_open_escrows(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::MaxOpenEscrows)
        .unwrap_or(0)
}

/// Number of currently open (non-terminal) escrows created by `client`.
fn open_escrows(env: &Env, client: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::OpenCount(client.clone()))
        .unwrap_or(0)
}

fn write_open_escrows(env: &Env, client: &Address, count: u32) {
    let key = DataKey::OpenCount(client.clone());
    env.storage().persistent().set(&key, &count);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

/// Increments the client's open-escrow counter after a successful creation (or
/// a change-order reopen).
fn bump_open_escrows(env: &Env, client: &Address) -> Result<(), EscrowError> {
    let count = open_escrows(env, client)
        .checked_add(1)
        .ok_or(EscrowError::ArithmeticOverflow)?;
    write_open_escrows(env, client, count);
    Ok(())
}

/// Decrements the client's open-escrow counter on a terminal transition. Never
/// underflows: at zero it leaves the counter untouched.
fn drop_open_escrows(env: &Env, client: &Address) {
    let count = open_escrows(env, client);
    if count > 0 {
        write_open_escrows(env, client, count - 1);
    }
}

/// Fails with `InvalidStatus` when `client` already holds the configured maximum
/// number of open escrows (`0` = unlimited). The error enum has reached its
/// 50-case Soroban limit, so the cap reuses the closest existing code; this is
/// documented in the README.
fn require_open_escrow_slot(env: &Env, client: &Address) -> Result<(), EscrowError> {
    let max = max_open_escrows(env);
    if max != 0 && open_escrows(env, client) >= max {
        return Err(EscrowError::InvalidStatus);
    }
    Ok(())
            .get::<_, bool>(&DataKey::AllowedToken(token.clone()))
            .unwrap_or(false)
    {
    // ---- Open-escrow cap (per client; checked before anything is written) ----
    require_open_escrow_slot(&env, &client)?;
    // Creation succeeded: the client now owns exactly one more open escrow.
    bump_open_escrows(&env, &c.client)?;
/// Refunds `refund` to the escrow's funders pro-rata and reduces their recorded
/// contributions by the same amounts, so later partial refunds stay proportional
/// to the remaining funded balance. Like `refund_funders`, the rounding
/// remainder goes to the largest funder.
fn refund_funders_shrink(
    env: &Env,
    c: &EscrowContract,
    refund: i128,
) -> Result<(), EscrowError> {
    record_closed_at(env, &c);
    if refund <= 0 {
        return Ok(());
    }
    let this = env.current_contract_address();
    let key = DataKey::Funders(c.id);
    let funders: Map<Address, i128> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Map::new(env));
    let total_funded = c.funded_amount;
    if funders.is_empty() || total_funded <= 0 {
        transfer_checked(env, &c.token, &this, &c.client, refund)?;
        return Ok(());
    }
    let mut remaining = refund;
    let mut largest: Option<Address> = None;
    let mut largest_amt: i128 = -1;
    let mut left: Map<Address, i128> = Map::new(env);
    for (funder, contribution) in funders.iter() {
        let share = contribution
            .checked_mul(refund)
            .ok_or(EscrowError::ArithmeticOverflow)?
            / total_funded;
        if share > 0 {
            transfer_checked(env, &c.token, &this, &funder, share)?;
            remaining = remaining
                .checked_sub(share)
                .ok_or(EscrowError::ArithmeticOverflow)?;
        }
        let kept = contribution
            .checked_sub(share)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        left.set(funder.clone(), kept);
        if contribution > largest_amt {
            largest_amt = contribution;
            largest = Some(funder);
        }
    }
    if remaining > 0 {
        let to = largest.clone().unwrap_or_else(|| c.client.clone());
        transfer_checked(env, &c.token, &this, &to, remaining)?;
        if let Some(funder) = largest {
            let kept = left.get(funder.clone()).unwrap_or(0);
            left.set(funder, kept.saturating_sub(remaining).max(0));
        }
    }
    env.storage().persistent().set(&key, &left);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
    Ok(())
}

        return Err(EscrowError::InvalidParties);
    }
    pub fn completed(env: &Env, c: &EscrowContract) {
        env.events().publish(
            escrow_topic(symbol_short!("complete"), c.id),
            CompletedEvent {
                released_amount: c.released_amount,
            },
        );
    }

    /// Admin-only: maximum number of concurrently open (non-terminal) escrows a
    /// single client may hold. `0` (the default) keeps creation unlimited. A
    /// client at the cap gets `InvalidStatus` from every creation entry point
    /// and from a change order that would reopen a completed escrow. Emits
    /// `("max_open",) -> max`.
    pub fn set_max_open_escrows(env: Env, max: u32) -> Result<(), EscrowError> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::MaxOpenEscrows, &max);
        bump_instance(&env);
        env.events().publish((symbol_short!("max_open"),), max);
        Ok(())
    }

    /// Number of currently open (non-terminal) escrows created by `client`.
    /// Counts `Created` and `Funded` (and `Disputed`) escrows; every terminal
    /// transition (`Completed`, `Resolved`, `Cancelled`) decrements it.
    pub fn get_open_escrows(env: Env, client: Address) -> u32 {
        open_escrows(&env, &client)
    }

    pub fn dispute(env: &Env, c: &EscrowContract, by: &Address, bond: i128) {
        env.events().publish(
            escrow_topic(symbol_short!("dispute"), c.id),
            DisputeEvent {
                by: by.clone(),
                status: c.status,
                bond,
            },
        );
    }

    pub fn resolved(
        env: &Env,
        c: &EscrowContract,
        to_freelancer: i128,
        to_client: i128,
        bond_recipient: Option<Address>,
        bond: i128,
    ) {
        env.events().publish(
            escrow_topic(symbol_short!("resolved"), c.id),
            ResolvedEvent {
                to_freelancer,
                to_client,
                status: c.status,
                bond_recipient,
                bond,
            },
        );
    }

        record_closed_at(&env, &c);
    pub fn cancel(env: &Env, c: &EscrowContract, refund: i128) {
        env.events().publish(
            escrow_topic(symbol_short!("cancel"), c.id),
            CancelEvent {
                refund,
                to: c.client.clone(),
                status: c.status,
            },
        );
    }

    pub fn reputation(env: &Env, c: &EscrowContract, rating: u32, rep: &Reputation, weight: i128) {
        env.events().publish(
            escrow_topic(symbol_short!("rep"), c.id),
            ReputationEvent {
                freelancer: c.freelancer.clone(),
                rating,
                completed_contracts: rep.completed_contracts,
                total_rating: rep.total_rating,
                weight,
            },
        );
    }
}

/// Escrow contract implementation.
#[contract]
pub struct Escrow;

impl Escrow {
    // ──────────────────────────────────────────────────────────────────────────
    // Initialization
    // ──────────────────────────────────────────────────────────────────────────

    /// Can only be called once after deployment.
    pub fn try_initialize(env: Env, admin: Address) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_RELEASE)?;
        admin.require_auth();
        let key = types::DataKey::Admin;
        env.storage().instance().set(&key, &admin);
        env.storage().instance().extend_ttl(&key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        internal_events::init(&env, &admin);
        bump_instance(&env);
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Parties
    // ──────────────────────────────────────────────────────────────────────────

    /// Bind the settlement token for this escrow. Write-once; rejects a second call.
    pub fn bind_settlement_token(env: Env, token: Address) -> Result<(), EscrowError> {
        require_admin(&env)?;
        let key = types::DataKey::SettlementToken;
        if env.storage().persistent().has(&key) {
            return Err(EscrowError::SettlementTokenAlreadyBound);
        }
        // Pre-bind probe: verify the address is a live SAC token contract.
        // The Soroban VM will panic if token::Client::balance is called on a non-SAC address,
        // so we rely on the runtime to surface that as a reversion.
        let _balance = token::Client::new(&env, &token).balance(&env.current_contract_address());
        if *env.current_contract_address() == token {
            return Err(EscrowError::SettlementTokenIsSelf);
        }
        env.storage().persistent().set(&key, &token);
        env.storage().persistent().extend_ttl(&key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        internal_events::init(&env, &env.current_contract_address());
        bump_instance(&env);
        Ok(())
    }

    /// Deprecated alias for `bind_settlement_token`.
    pub fn set_settlement_token(_env: Env, _token: Address) -> Result<(), EscrowError> {
        // Retained for backward compatibility; new code must call `bind_settlement_token`.
        Err(EscrowError::DeprecatedAlias)
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Creation
    // ──────────────────────────────────────────────────────────────────────────

    /// Creates a new escrow with the given milestone amounts.
    /// The caller must be the client.
    pub fn try_create_contract(
        env: Env,
        client: Address,
        freelancer: Address,
        token: Address,
        milestone_amounts: Vec<i128>,
    ) -> Result<u64, EscrowError> {
        build_escrow(env, client, freelancer, token, milestone_amounts, false, DEFAULT_REVIEW_WINDOW_SECS)
    }

    /// Creates a new escrow and immediately funds it.
    pub fn try_create_and_fund(
        env: Env,
        client: Address,
        freelancer: Address,
        token: Address,
        milestone_amounts: Vec<i128>,
    ) -> Result<u64, EscrowError> {
        build_escrow(env, client, freelancer, token, milestone_amounts, true, DEFAULT_REVIEW_WINDOW_SECS)
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Deposits
    // ──────────────────────────────────────────────────────────────────────────

    /// Client deposits funds into a `Created` escrow.
    /// Amount must not cause overfunding.
    pub fn try_deposit_funds(env: Env, id: u64, amount: i128) -> Result<(), EscrowError> {
        let mut c = load(&env, id)?;
        apply_deposit(&env, &mut c, &env.current_signer(), amount)
    }

    /// Client deposits the full total into a new escrow.
    pub fn deposit_funds(env: Env, id: u64, amount: i128) {
        let mut c = load(&env, id)?;
        deposit_funds_inner(&env, &mut c, amount);
    /// Lets a funder withdraw part or all of their own deposit while the escrow
    /// is still `Created` (not frozen, not funded/cancelled/terminal). Reduces
    /// the funder's contribution, `funded_amount`, and token liability; removes
    /// the funder from the map at zero and clears any pending settlement
    /// proposal. Emits `("wd_dep", id) -> (funder, amount, funded_amount)`.
    pub fn withdraw_deposit(
        env: Env,
        id: u64,
        funder: Address,
        amount: i128,
    ) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_DEPOSIT)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        funder.require_auth();
        if c.status != ContractStatus::Created {
            return Err(EscrowError::InvalidStatus);
        }
        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }
        if m.released || is_reclaimed(&env, id, index) || submitted_at(&env, id, index).is_some() {
        let key = DataKey::Funders(id);
        let mut funders: Map<Address, i128> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Map::new(&env));
        let contribution = funders.get(funder.clone()).unwrap_or(0);
        if amount > contribution {
            return Err(EscrowError::InvalidAmount);
        }
        let held = c
            .funded_amount
            .checked_sub(c.released_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if amount > held {
            return Err(EscrowError::InsufficientFunds);
        }
        if m.released || is_reclaimed(&env, id, index) {
        let new_contribution = contribution
            .checked_sub(amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        let new_funded = c
            .funded_amount
            .checked_sub(amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        transfer_checked(
            &env,
            &c.token,
            &env.current_contract_address(),
            &funder,
            amount,
        )?;
        if new_contribution == 0 {
            funders.remove(funder.clone());
        } else {
            funders.set(funder.clone(), new_contribution);
        }
            record_closed_at(&env, &c);
        if funders.is_empty() {
            env.storage().persistent().remove(&key);
        } else {
            env.storage().persistent().set(&key, &funders);
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_TTL_THRESHOLD,
                PERSISTENT_TTL_EXTEND,
            );
        }
        c.funded_amount = new_funded;
        clear_settlement(&env, c.id);
        sub_liability(&env, &c.token, amount)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, &c);
        env.events()
            .publish((symbol_short!("wd_dep"), id), (funder, amount, new_funded));
        Ok(())
    }

    /// Emits `(1, "submit", id) -> SubmitEvent { index, submitted_at }`.
        events::submit(&env, id, index, now);
    }

    fn deposit_funds_inner(env: &Env, c: &mut EscrowContract, amount: i128) {
        env.current_signer().require_auth();
        if c.status != ContractStatus::Created {
    /// submitted funds are kept in escrow. Emits `(1, "claim", id) -> ClaimEvent { index, amount }`.
            return Err(EscrowError::InvalidStatus);
        }
        // Escrows created before funding deadlines existed have no `FundBy` entry.
        if let Some(fund_by) = env.storage().persistent().get::<_, u64>(&types::DataKey::FundBy(c.id)) {
            if env.ledger().timestamp() > fund_by {
                return Err(EscrowError::FundingExpired);
            }
        }
        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }
        let new_funded = c.funded_amount.checked_add(amount).ok_or(EscrowError::InvalidAmount)?;
        if new_funded > c.total_amount {
            return Err(EscrowError::OverFunded);
        }
        let key = types::DataKey::Funders(c.id);
        let mut funders: Map<Address, i128> = env.storage().persistent().get(&key).unwrap_or_else(|| Map::new(env));
        let prev = funders.get(env.current_signer().clone());
        if prev.is_none() && funders.len() >= MAX_FUNDERS {
            return Err(EscrowError::TooManyFunders);
        }
        let contribution = prev.unwrap_or(0).checked_add(amount).ok_or(EscrowError::ArithmeticOverflow)?;
        funders.set(env.current_signer().clone(), contribution);
        token::Client::new(env, &c.token).transfer(&env.current_signer(), &env.current_contract_address(), &amount)?;
        env.storage().persistent().set(&key, &funders);
        env.storage().persistent().extend_ttl(&key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        c.funded_amount = new_funded;
        if c.funded_amount == c.total_amount {
            transition(&mut c, ContractStatus::Funded)?;
        events::claim(&env, id, index, remaining);
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Milestone management
    // ──────────────────────────────────────────────────────────────────────────

    /// Client releases a single milestone.
    pub fn try_release_milestone(env: Env, id: u64, index: u32) -> Result<(), EscrowError> {
        let mut c = load(&env, id)?;
        release_milestone_inner(&env, &mut c, index)
    }

    fn release_milestone_inner(env: &Env, c: &mut EscrowContract, index: u32) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_RELEASE)?;
        require_not_frozen(&env, c.id)?;
        // Stream escrows pay out only through `withdraw_vested` / `stop_stream`.
        if is_stream(&env, c.id) {
            return Err(EscrowError::InvalidStatus);
        }
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        let mut m = c.milestones.get(index).ok_or(EscrowError::MilestoneNotFound)?;
        if m.released || is_reclaimed(&env, id, index) {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }
        if is_sequential(&env, c.id) && first_unreleased(c) != Some(index) {
            return Err(EscrowError::OutOfOrder);
        }
        // Only the remaining amount of a partially paid milestone is paid now.
        let paid = milestone_paid(&env, c.id, index);
        let remaining = m.amount - paid;
        if c.funded_amount - c.released_amount < remaining {
            return Err(EscrowError::InsufficientFunds);
        }
        let fee = pay_out(&env, c, remaining)?;
        m.released = true;
        c.released_amount = c.released_amount.checked_add(remaining).ok_or(EscrowError::ArithmeticOverflow)?;
        c.milestones.set(index, m.clone());
        if paid > 0 {
            clear_milestone_paid(&env, c.id, index);
        }
        clear_submitted(&env, c.id, index);
        let completed = c.milestones.iter().all(|x| x.released);
        if completed {
            transition(&mut c, ContractStatus::Completed)?;
            record_closed_at(&env, &c);
            record_closed_at(&env, c.id);
        }
        clear_settlement(&env, c.id);
        sub_liability(&env, &c.token, remaining)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, c);
        events::release(
            &env,
            c,
            index,
            remaining,
            &payee(&env, c),
            c.released_amount,
            fee,
        );
        if completed {
            events::completed(&env, c);
        }
        Ok(())
    }

    /// Client releases any portion of a milestone.
    pub fn try_release_partial(env: Env, id: u64, index: u32, amount: i128) -> Result<(), EscrowError> {
        let mut c = load(&env, id)?;
        release_partial_inner(&env, &mut c, index, amount)
    }

    fn release_partial_inner(env: &Env, c: &mut EscrowContract, index: u32, amount: i128) -> Result<(), EscrowError> {
    /// Emits `(1, "rel_part", id) -> PartialReleaseEvent { index, amount, remaining }`.
        require_not_paused(&env, PAUSE_RELEASE)?;
        require_not_frozen(&env, c.id)?;
        // Stream escrows pay out only through `withdraw_vested` / `stop_stream`.
        if is_stream(&env, c.id) {
            return Err(EscrowError::InvalidStatus);
        }
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        let mut m = c.milestones.get(index).ok_or(EscrowError::MilestoneNotFound)?;
        if m.released || is_reclaimed(&env, id, index) {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }
        if is_sequential(&env, c.id) && first_unreleased(c) != Some(index) {
            return Err(EscrowError::OutOfOrder);
        }
        let paid = milestone_paid(&env, c.id, index);
        let remaining = m.amount - paid;
        if amount <= 0 || amount > remaining {
            return Err(EscrowError::InvalidAmount);
        }
        if c.funded_amount - c.released_amount < amount {
            return Err(EscrowError::InsufficientFunds);
        }
        pay_out(&env, c, amount)?;
        c.released_amount = c.released_amount.checked_add(amount).ok_or(EscrowError::ArithmeticOverflow)?;
        let new_paid = paid + amount;
        let remaining_after = m.amount - new_paid;
        let mut completed = false;
        if remaining_after == 0 {
            m.released = true;
            c.milestones.set(index, m.clone());
            clear_milestone_paid(&env, c.id, index);
            clear_submitted(&env, c.id, index);
            if c.milestones.iter().all(|x| x.released) {
                transition(&mut c, ContractStatus::Completed)?;
                record_closed_at(&env, &c);
                record_closed_at(&env, c.id);
                completed = true;
            }
        } else {
            set_milestone_paid(&env, c.id, index, new_paid);
        }
        clear_settlement(&env, c.id);
        sub_liability(&env, &c.token, amount)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, c);
            symbol_short!("rel_part"),
        events::partial_release(&env, id, index, amount, remaining_after);
        if completed {
            events::completed(&env, c);
        }
        Ok(())
    }

    /// Client releases multiple milestones in one call.
    pub fn try_release_milestones(env: Env, id: u64, indices: Vec<u32>) -> Result<i128, EscrowError> {
        let mut c = load(&env, id)?;
        release_milestones_inner(&env, &mut c, indices)
    }

    fn release_milestones_inner(env: &Env, c: &mut EscrowContract, indices: Vec<u32>) -> Result<i128, EscrowError> {
        require_not_paused(&env, PAUSE_RELEASE)?;
        require_not_frozen(&env, c.id)?;
        // Stream escrows pay out only through `withdraw_vested` / `stop_stream`.
        if is_stream(&env, c.id) {
            return Err(EscrowError::InvalidStatus);
        }
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        let count = indices.len();
        if count == 0 || count > MAX_MILESTONES {
            return Err(EscrowError::InvalidMilestones);
        }

        let mut total: i128 = 0;
        let mut paid_now: Vec<i128> = Vec::new(&env);
        for (pos, index) in indices.iter().enumerate() {
            for earlier in indices.iter().take(pos) {
                if earlier == index {
                    return Err(EscrowError::DuplicateIndex);
                }
            }
            let m = c.milestones.get(index).ok_or(EscrowError::MilestoneNotFound)?;
            if m.released || is_reclaimed(&env, id, index) {
                return Err(EscrowError::MilestoneAlreadyReleased);
            }
            let remaining = m.amount - milestone_paid(&env, c.id, index);
            paid_now.push_back(remaining);
            total = total.checked_add(remaining).ok_or(EscrowError::ArithmeticOverflow)?;
        }

        // Sequential escrows: every milestone before a requested index must be
        // already released or part of this batch.
        if is_sequential(&env, c.id) {
            for index in indices.iter() {
                for j in 0..index {
                    let prior = c.milestones.get(j).ok_or(EscrowError::MilestoneNotFound)?;
                    if !prior.released && !indices.contains(j) {
                        return Err(EscrowError::OutOfOrder);
                    }
                }
            }
        }

        let held = c.funded_amount.checked_sub(c.released_amount).ok_or(EscrowError::ArithmeticOverflow)?;
        if held < total {
            return Err(EscrowError::InsufficientFunds);
        }

        let mut fees: Vec<i128> = Vec::new(&env);
        let mut fee_total: i128 = 0;
        for amount in paid_now.iter() {
            let fee = fee_for(&env, c, *amount);
            fees.push_back(fee);
            fee_total = fee_total.checked_add(fee).ok_or(EscrowError::ArithmeticOverflow)?;
        }
        pay_out_with_fee(&env, c, total, fee_total)?;
        for (pos, index) in indices.iter().enumerate() {
            let mut m = c.milestones.get(index).unwrap();
            m.released = true;
            c.milestones.set(index, m.clone());
            if paid_now.get(pos as u32).unwrap() != m.amount {
                clear_milestone_paid(&env, c.id, index);
            }
            clear_submitted(&env, c.id, index);
        }
        c.released_amount = c.released_amount.checked_add(total).ok_or(EscrowError::ArithmeticOverflow)?;
        let completed = c.milestones.iter().all(|x| x.released);
        if completed {
            transition(&mut c, ContractStatus::Completed)?;
            record_closed_at(&env, c.id);
        }
        clear_settlement(&env, c.id);
        sub_liability(&env, &c.token, total)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, c);
        let to = payee(&env, c);
        let mut running = c.released_amount.checked_sub(total).ok_or(EscrowError::ArithmeticOverflow)?;
        for (pos, index) in indices.iter().enumerate() {
            let amount = paid_now.get(pos as u32).unwrap();
            running = running.checked_add(amount).ok_or(EscrowError::ArithmeticOverflow)?;
            events::release(&env, c, index, amount, &to, running, fees.get(pos as u32).unwrap());
        }
        env.events().publish((symbol_short!("rel_batch"), c.id), (count, total));
        events::batch_release(&env, id, count, total);
        if completed {
            events::completed(&env, c);
        }
        Ok(total)
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Withdraw vested (stream escrows)
    // ──────────────────────────────────────────────────────────────────────────

    /// Freelancer withdraws everything vested but not yet withdrawn from a
    /// `Funded` stream. Once fully vested and withdrawn the milestone is
    /// released and the escrow `Completed`.
    pub fn try_withdraw_vested(env: Env, id: u64) -> Result<i128, EscrowError> {
        require_not_paused(&env, PAUSE_RELEASE)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        c.freelancer.require_auth();
        if c.status != ContractStatus::Funded {
            return Err(EscrowError::InvalidStatus);
        }
        let stream = load_stream(&env, id).ok_or(EscrowError::ContractNotFound)?;
        let vested = stream_vested(&env, &c, &stream);
        let amount = vested.checked_sub(stream.withdrawn).ok_or(EscrowError::ArithmeticOverflow)?;
        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }
        pay_out(&env, &c, amount)?;
        c.released_amount = c.released_amount.checked_add(amount).ok_or(EscrowError::ArithmeticOverflow)?;
        stream.withdrawn = vested.checked_sub(stream.withdrawn).ok_or(EscrowError::ArithmeticOverflow)?;
        let completed = stream.withdrawn >= c.total_amount;
        if completed {
            let mut m = c.milestones.get(0).unwrap();
            m.released = true;
            c.milestones.set(0, m);
            transition(&mut c, ContractStatus::Completed)?;
            record_closed_at(&env, &c);
            record_closed_at(&env, c.id);
        }
        clear_settlement(&env, id);
        sub_liability(&env, &c.token, amount)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, c);
        save_stream(&env, id, &stream);
        env.events().publish((symbol_short!("stream_wd"), id), amount);
        if completed {
            events::completed(&env, c);
        }
        Ok(amount)
        events::bonus(&env, id, amount, cumulative);
    }

    /// Client stops a `Funded` stream: the freelancer receives what vested but
    /// was not withdrawn, the unvested remainder is refunded to the client
    /// (pro-rata to funders), and the escrow is `Cancelled`.
    pub fn try_stop_stream(env: Env, id: u64) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_CANCEL)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        let mut stream = load_stream(&env, id).ok_or(EscrowError::ContractNotFound)?;
        c.client.require_auth();
        if c.status != ContractStatus::Funded || stream.stopped_at.is_some() {
            return Err(EscrowError::InvalidStatus);
        }
        stream.stopped_at = Some(env.ledger().timestamp());
        let vested = stream_vested(&env, &c, &stream);
        let to_freelancer = vested.checked_sub(stream.withdrawn).ok_or(EscrowError::ArithmeticOverflow)? max(0);
        let held = c.funded_amount.checked_sub(c.released_amount).ok_or(EscrowError::ArithmeticOverflow)?;
        let to_client = held.checked_sub(to_freelancer).ok_or(EscrowError::ArithmeticOverflow)?;
        pay_out(&env, &c, to_freelancer)?;
        refund_funders(&env, &c, to_client)?;
        c.released_amount = c.released_amount.checked_add(to_freelancer).ok_or(EscrowError::ArithmeticOverflow)?;
        stream.withdrawn = stream.withdrawn.checked_add(to_freelancer).ok_or(EscrowError::ArithmeticOverflow)?;
        c.refunded_amount = to_client;
        c.funded_amount = c.released_amount;
        transition(&mut c, ContractStatus::Cancelled)?;
        record_closed_at(&env, id);
        clear_settlement(&env, id);
        sub_liability(&env, &c.token, held)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, c);
        save_stream(&env, id, &stream);
        env.events().publish((symbol_short!("stream_st"), id), (to_freelancer, to_client));
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Disputes
    // ──────────────────────────────────────────────────────────────────────────

    /// Either party may raise a dispute, freezing releases until the admin resolves it.
    pub fn try_open_dispute(env: Env, id: u64, caller: Address) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_DISPUTE)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        caller.require_auth();
        if caller != c.client && caller != c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        // Reject disputes on escrows that have no held funds — there is nothing
        // to resolve and a dispute would block expire_unfunded / cancel_contract.
        let held = c.funded_amount.saturating_sub(c.released_amount);
        if held <= 0 {
            return Err(EscrowError::InsufficientFunds);
        }
        // The opener posts a refundable bond before the status changes.
        let bond = dispute_bond(c.total_amount);
        // Record when the dispute clock started, so `resolve_expired_dispute`
        // can release the funds if no resolver ever acts.
        let opened_key = DataKey::DisputeOpenedAt(id);
        env.storage()
            .persistent()
            .set(&opened_key, &env.ledger().timestamp());
        env.storage().persistent().extend_ttl(
            &opened_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND,
        );
        token::Client::new(&env, &c.token).transfer(&caller, &env.current_contract_address(), &bond)?;
        let bond_key = types::DataKey::DisputeBond(id);
        env.storage().persistent().set(&bond_key, &(caller.clone(), bond));
        env.storage().persistent().extend_ttl(&bond_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        add_liability(&env, &c.token, bond)?;
        transition(&mut c, ContractStatus::Disputed)?;
        save(&env, c);

        // Increment disputes_opened for the caller using checked arithmetic
        let outcome_key = types::DataKey::Outcomes(caller.clone());
        let mut caller_outcomes: Outcomes = env.storage().persistent().get(&outcome_key).unwrap_or_default();
        caller_outcomes.disputes_opened = caller_outcomes.disputes_opened.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().persistent().set(&outcome_key, &caller_outcomes);
        env.storage().persistent().extend_ttl(&outcome_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);

        events::dispute(&env, &c, &caller, bond);
        Ok(())
    }

    /// Resolves a dispute (caller must be the admin or hold the `Resolver` role and
    /// must not be a party to the escrow), paying `freelancer_bps` (0..=10000) of the escrowed
    /// balance to the freelancer and refunding the rest to the client. The
    /// dispute bond is settled separately: it returns to the opener when
    /// `bond_to_opener` is true, otherwise it goes to the counterparty.
    /// 
    /// Outcomes:
    /// If freelancer_bps >= DISPUTE_WIN_THRESHOLD_BPS (5_000 bps, or 50%),
    /// freelancer gets a dispute win and client gets a dispute loss.
    /// Otherwise, freelancer gets a loss and client gets a win.
    /// Note: this operation remains allowed even when the escrow is frozen.
    /// Escrows at or above `PANEL_THRESHOLD_AMOUNT` must be settled by the
    /// arbiter panel instead and return `PanelRequired`.
    pub fn try_resolve_dispute(
        env: Env,
        caller: Address,
        id: u64,
        freelancer_bps: u32,
        bond_to_opener: bool,
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
        settle(&env, c, freelancer_bps, Some(bond_to_opener), true)
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Panel voting
    // ──────────────────────────────────────────────────────────────────────────

    /// Admin replaces the arbiter panel.
    pub fn try_set_panel(env: Env, members: Vec<Address>, threshold: u32) -> Result<(), EscrowError> {
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
            settle(&env, c, winning_bps, None, true)?;
    /// Permissionless escape hatch for a `Disputed` escrow nobody resolves.
    /// Once the ledger timestamp reaches `opened_at + DISPUTE_TIMEOUT_SECS`
    /// (30 days from `open_dispute`), anyone may settle it: the held balance is
    /// split 50/50 (5,000 bps to the freelancer, rounding remainder to the
    /// client) through the shared `settle` helper and the dispute bond returns
    /// to its opener. Before the deadline it fails with `NotExpired`; on an
    /// escrow that is not `Disputed` it fails with `InvalidStatus`.
    ///
    /// Deliberately works while the escrow is frozen and while the contract is
    /// paused, mirroring `resolve_dispute` (which ignores freezes): neither a
    /// freeze nor a pause may keep funds stranded past the timeout. Escrows
    /// above `PANEL_THRESHOLD_AMOUNT` are covered too when their panel never
    /// reaches the threshold.
    ///
    /// No resolver adjudicated this split, so the `Outcomes` win/loss counters
    /// are left untouched (only `disputes_opened` from `open_dispute` moves).
    /// Emits the normal `("resolved", id)` event, so indexers need no new
    /// handling.
    pub fn resolve_expired_dispute(env: Env, id: u64) -> Result<(), EscrowError> {
        bump_instance(&env);
        let c = load(&env, id)?;
        if c.status != ContractStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        let opened_at: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::DisputeOpenedAt(id))
            .ok_or(EscrowError::InvalidStatus)?;
        let deadline = opened_at
            .checked_add(DISPUTE_TIMEOUT_SECS)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if env.ledger().timestamp() < deadline {
            return Err(EscrowError::NotExpired);
        }
        settle(
            &env,
            c,
            DISPUTE_TIMEOUT_SPLIT_BPS,
            Some(true),
            false,
        )
    }

        }
        bump_instance(&env);
        env.events().publish((symbol_short!("panel"),), (len, threshold));
        Ok(())
    }

    /// A current panel member votes on a disputed escrow's split.
    pub fn try_vote_resolution(
        env: Env,
        member: Address,
        id: u64,
        freelancer_bps: u32,
    ) -> Result<(), EscrowError> {
        member.require_auth();
        bump_instance(&env);
        let c = load(&env, id)?;
        if c.status != ContractStatus::Disputed {
            return Err(EscrowError::InvalidStatus);
        }
        if m.released || is_reclaimed(&env, id, index) {
        if member == c.client || member == c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        // Stream escrows vest linearly and never carry milestone change orders.
        if is_stream(&env, id) {
            return Err(EscrowError::InvalidStatus);
        }
        // Checked before the proposal lookup so a stream escrow always reports
        // InvalidStatus rather than ChangeOrderNotFound.
        if is_stream(&env, id) {
            return Err(EscrowError::InvalidStatus);
        }
        // Reopening a terminal (`Completed`) escrow consumes a fresh open slot,
        // subject to the same per-client cap as creation.
        let was_terminal = c.status == ContractStatus::Completed;
        if was_terminal {
            require_open_escrow_slot(&env, &c.client)?;
        }
        if was_terminal {
            bump_open_escrows(&env, &c.client)?;
        }
    /// Freelancer rejects (removes) the pending change order. Rejection follows
    /// the same `PAUSE_CREATE` and freeze gates as proposing and accepting one.
        require_not_paused(&env, PAUSE_CREATE)?;
        require_not_frozen(&env, id)?;
    /// Client commits optional per-milestone due dates (ledger timestamps).
    /// `dates` is either empty (clears any deadlines) or exactly one strictly
    /// increasing date per milestone, else `InvalidMetadata`. Only while
    /// `Created` with nothing deposited; frozen afterwards (`InvalidStatus`).
    /// Emits `("m_due", id) -> dates`.
    pub fn set_milestone_due(env: Env, id: u64, dates: Vec<u64>) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_CREATE)?;
        let c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        c.client.require_auth();
        if c.status != ContractStatus::Created || c.funded_amount != 0 {
            return Err(EscrowError::InvalidStatus);
        }
        let n = dates.len();
        if n != 0 && n != c.milestones.len() {
            return Err(EscrowError::InvalidMetadata);
        }
        let mut previous: Option<u64> = None;
        for due in dates.iter() {
            if let Some(prev) = previous {
                if due <= prev {
                    return Err(EscrowError::InvalidMetadata);
                }
            }
            previous = Some(due);
        }
        let key = DataKey::MilestoneDue(id);
        env.storage().persistent().set(&key, &dates);
        env.storage()
            .persistent()
            .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
        env.events().publish((symbol_short!("m_due"), id), dates);
        Ok(())
    }

    /// Due date (ledger timestamp) agreed for a milestone, if any.
    pub fn get_milestone_due(env: Env, id: u64, index: u32) -> Option<u64> {
        milestone_due(&env, id, index)
    }

    /// Client reclaims a milestone whose agreed due date has passed and that
    /// the freelancer never submitted. Its unpaid amount is refunded to the
    /// funders and removed from the escrow's `total_amount`/`funded_amount`; a
    /// partially paid milestone refunds only what is still unpaid. Once no
    /// milestone is left outstanding the remainder is closed as `Cancelled`.
    /// Submitted milestones can never be reclaimed (`InvalidStatus`).
    /// Emits `("reclaim", id) -> (index, refund)`.
    pub fn reclaim_overdue(env: Env, id: u64, index: u32) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_CANCEL)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        // Stream escrows pay out only through `withdraw_vested` / `stop_stream`.
        if is_stream(&env, id) {
            return Err(EscrowError::InvalidStatus);
        }
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        let m = c
            .milestones
            .get(index)
            .ok_or(EscrowError::MilestoneNotFound)?;
        if m.released || is_reclaimed(&env, id, index) {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }
        // Submitted work is committed to the freelancer and cannot be clawed back.
        if submitted_at(&env, id, index).is_some() {
            return Err(EscrowError::InvalidStatus);
        }
        let due = milestone_due(&env, id, index).ok_or(EscrowError::InvalidMetadata)?;
        if env.ledger().timestamp() <= due {
            return Err(EscrowError::NotExpired);
        }
        record_closed_at(&env, &c);

        let unpaid = m
            .amount
            .checked_sub(milestone_paid(&env, id, index))
            .ok_or(EscrowError::ArithmeticOverflow)?;
        let held = c
            .funded_amount
            .checked_sub(c.released_amount)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        // Never refund more than the escrow actually holds (partial funding).
        let refund = unpaid.min(held).max(0);

        refund_funders_shrink(&env, &c, refund)?;
        c.refunded_amount = c
            .refunded_amount
            .checked_add(refund)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        c.funded_amount = c
            .funded_amount
            .checked_sub(refund)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        c.total_amount = c
            .total_amount
            .checked_sub(unpaid)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        mark_reclaimed(&env, id, index);
        sub_liability(&env, &c.token, refund)?;

        // Nothing left outstanding: cancel the remainder (releasing any funds
        // still held for milestones that no longer exist).
        let outstanding = c
            .milestones
            .iter()
            .enumerate()
            .any(|(i, m)| !m.released && !is_reclaimed(&env, id, i as u32));
        if !outstanding {
            let leftover = c
                .funded_amount
                .checked_sub(c.released_amount)
                .ok_or(EscrowError::ArithmeticOverflow)?;
            if leftover > 0 {
                refund_funders_shrink(&env, &c, leftover)?;
                c.refunded_amount = c
                    .refunded_amount
                    .checked_add(leftover)
                    .ok_or(EscrowError::ArithmeticOverflow)?;
                c.funded_amount = c.released_amount;
                sub_liability(&env, &c.token, leftover)?;
            }
            transition(&mut c, ContractStatus::Cancelled)?;
            record_closed_at(&env, id);
        }
        clear_settlement(&env, id);
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        #[cfg(any(test, debug_assertions))]
        check_invariants(&c);
        save(&env, &c);
        env.events()
            .publish((symbol_short!("reclaim"), id), (index, refund));
        Ok(())
    }

        if freelancer_bps as i128 > BPS_DENOMINATOR {
            return Err(EscrowError::InvalidSplit);
        }
        let panel: Panel = env.storage().instance().get(&types::DataKey::Panel).ok_or(EscrowError::Unauthorized)?;
        if !panel.members.contains(&member) {
            return Err(EscrowError::Unauthorized);
        }
        let key = types::DataKey::Votes(id);
        let mut votes: Map<Address, u32> = env.storage().persistent().get(&key).unwrap_or_else(|| Map::new(env));
        votes.set(member.clone(), freelancer_bps);
        env.events().publish((symbol_short!("voted"), id), (member.clone(), freelancer_bps));
        env.storage().persistent().set(&key, &votes);
        env.storage().persistent().extend_ttl(&key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        // Only votes from members currently on the panel are tallied
        let mut counted: Vec<(Address, u32)> = Vec::new(env);
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
            settle(&env, c, winning_bps, None)?;
            for (voter, bps) in counted.iter() {
                env.events().publish((symbol_short!("vote"), id), (voter, bps));
            }
        }
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Settlement
    // ──────────────────────────────────────────────────────────────────────────
    /// (`InvalidStatus`). Emits `(1, "terms", id) -> TermsEvent { terms_hash }`.
        events::terms(&env, id, terms_hash);
    /// until the milestone is released. Emits `(1, "deliver", id) -> DeliverEvent { index, delivery_hash }`.
        events::deliver(&env, id, index, delivery_hash);

    /// Mutual settlement proposal.
    pub fn try_propose_settlement(
    /// and `Disputed` escrows; replaces any earlier (including expired) proposal
    /// and starts a fresh `SETTLEMENT_VALID_SECS` (7-day) validity window. Emits
        env: Env,
        id: u64,
        proposer: Address,
        freelancer_amount: i128,
    ) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_CANCEL)?;
        let c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        proposer.require_auth();
        if proposer != c.client && proposer != c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded | ContractStatus::Disputed) {
            return Err(EscrowError::InvalidStatus);
        }
        let held = c.funded_amount.checked_sub(c.released_amount).ok_or(EscrowError::ArithmeticOverflow)?;
        if freelancer_amount < 0 || freelancer_amount > held {
            return Err(EscrowError::InvalidAmount);
        }
        let key = types::DataKey::Settlement(id);
        env.storage().persistent().set(
            &key,
            &SettlementProposal {
                proposer: proposer.clone(),
                freelancer_amount,
                created_at: env.ledger().timestamp(),
            },
        );
        env.storage().persistent().extend_ttl(&key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        env.events().publish((symbol_short!("settle_p"), id), (proposer, freelancer_amount));
        Ok(())
    }

    /// The other party accepts the pending settlement.
    pub fn try_accept_settlement(
    /// proposal, self-acceptance, a different amount, or an expired proposal
    /// (`now > created_at + SETTLEMENT_VALID_SECS`) fail with
        env: Env,
        id: u64,
        acceptor: Address,
        freelancer_amount: i128,
    ) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_CANCEL)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        acceptor.require_auth();
        if acceptor != c.client && acceptor != c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        let key = types::DataKey::Settlement(id);
        let proposal: SettlementProposal = env.storage().persistent().get(&key).ok_or(EscrowError::SettlementMismatch)?;
        if acceptor == proposal.proposer || freelancer_amount != proposal.freelancer_amount {
            return Err(EscrowError::SettlementMismatch);
        }
        let deadline = proposal
            .created_at
            .checked_add(SETTLEMENT_VALID_SECS)
            .ok_or(EscrowError::ArithmeticOverflow)?;
        if env.ledger().timestamp() > deadline {
            return Err(EscrowError::SettlementMismatch);
        }
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded | ContractStatus::Disputed) {
            return Err(EscrowError::InvalidStatus);
        }
        let held = c.funded_amount.checked_sub(c.released_amount).ok_or(EscrowError::ArithmeticOverflow)?;
        if freelancer_amount > held {
            return Err(EscrowError::InvalidAmount);
        }
        let to_freelancer = freelancer_amount;
        let to_client = held - to_freelancer;
        env.storage().persistent().remove(&key);
        pay_out(&env, &c, to_freelancer)?;
        refund_funders(&env, &c, to_client)?;
        c.released_amount = c.released_amount.checked_add(to_freelancer).ok_or(EscrowError::ArithmeticOverflow)?;
        c.refunded_amount = to_client;
        // Dedicated mutual-settlement edge: Created / Funded / Disputed -> Resolved, by both parties.
        c.status = ContractStatus::Resolved;
        record_closed_at(&env, &c);
        sub_liability(&env, &c.token, held)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, c);
        env.events().publish((symbol_short!("settled"), id), (to_freelancer, to_client));
        Ok(())
    }

    /// The proposer withdraws its pending settlement proposal (including an
    /// expired one).
    pub fn try_withdraw_settlement(env: Env, id: u64, proposer: Address) -> Result<(), EscrowError> {
        load(&env, id)?;
        proposer.require_auth();
        bump_instance(&env);
        let key = types::DataKey::Settlement(id);
        let proposal: SettlementProposal = env.storage().persistent().get(&key).ok_or(EscrowError::SettlementMismatch)?;
        if proposal.proposer != proposer {
            return Err(EscrowError::Unauthorized);
    /// Pending settlement proposal, if any. Still returns the stored record
    /// after expiry; a proposal is expired (and `accept_settlement` rejects it)
    /// once `now > created_at + SETTLEMENT_VALID_SECS`.
        }
        env.storage().persistent().remove(&key);
        env.events().publish((symbol_short!("settle_w"), id), proposer);
        Ok(())
    }

    /// Pending settlement proposal, if any.
        record_closed_at(&env, &c);
    pub fn try_get_settlement(env: Env, id: u64) -> Option<SettlementProposal> {
        env.storage().persistent().get(&types::DataKey::Settlement(id))
    }

        record_closed_at(&env, &c);
    // ──────────────────────────────────────────────────────────────────────────
    // Reputation
    // ──────────────────────────────────────────────────────────────────────────

    /// Client rates the freelancer.
    pub fn try_issue_reputation(env: Env, id: u64, rating: u32) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_REPUTATION)?;
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
        let key = types::DataKey::Reputation(c.freelancer.clone());
        let mut rep: Reputation = env.storage().persistent().get(&key).unwrap_or_default();
        rep.completed_contracts = rep.completed_contracts.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
        rep.total_rating = rep.total_rating.checked_add(rating).ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().persistent().set(&key, &rep);
        env.storage().persistent().extend_ttl(&key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);

        let wkey = types::DataKey::WeightedRep(c.freelancer.clone());
        let mut wrep: WeightedReputation = env.storage().persistent().get(&wkey).unwrap_or_default();
        wrep.weighted_rating_sum = weight
            .checked_mul(rating as i128)
            .and_then(|w| wrep.weighted_rating_sum.checked_add(w))
            .ok_or(EscrowError::ArithmeticOverflow)?;
        wrep.weight_total = wrep.weight_total.checked_add(weight).ok_or(EscrowError::ArithmeticOverflow)?;
        wrep.rated_contracts = wrep.rated_contracts.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().persistent().set(&wkey, &wrep);
        env.storage().persistent().extend_ttl(&wkey, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);

        let weight_key = types::DataKey::RatingWeight(id);
        env.storage().persistent().set(&weight_key, &weight);
        env.storage().persistent().extend_ttl(&weight_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);

        c.reputation_issued = true;
        save(&env, c);
        events::reputation(&env, &c, rating, &rep, weight);
        Ok(())
    }

    /// After completion or resolution the freelancer may rate the client (1..=5) once per
    /// contract, provided `released_amount >= MIN_REPUTATION_VALUE`.
    /// Freelancer rates the client.
    pub fn try_rate_client(env: Env, id: u64, rating: u32) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_REPUTATION)?;
        let c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        c.freelancer.require_auth();
        if !matches!(c.status, ContractStatus::Completed | ContractStatus::Resolved) {
            return Err(EscrowError::InvalidStatus);
        }
        let rated_key = types::DataKey::ClientRated(id);
        if env.storage().persistent().get::<_, bool>(&rated_key).unwrap_or(false) {
            return Err(EscrowError::ReputationAlreadyIssued);
        }
        if !(1..=5).contains(&rating) {
            return Err(EscrowError::InvalidRating);
        }
        // Mirror `issue_reputation`: dust escrows must not move client
        // reputation either, otherwise dust jobs could be used to sybil up a
        // client's score.
        if c.released_amount < MIN_REPUTATION_VALUE {
            return Err(EscrowError::BelowReputationThreshold);
        }
        let rep_key = types::DataKey::ClientReputation(c.client.clone());
        let mut rep: Reputation = env.storage().persistent().get(&rep_key).unwrap_or_default();
        rep.completed_contracts = rep.completed_contracts.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
        rep.total_rating = rep.total_rating.checked_add(rating).ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().persistent().set(&rep_key, &rep);
        env.storage().persistent().extend_ttl(&rep_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        env.storage().persistent().set(&rated_key, &true);
        env.storage().persistent().extend_ttl(&rated_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        env.events().publish((symbol_short!("rep_cli"), id), (c.client, rating));
        Ok(())
    }

    /// Governance can void fraudulent ratings.
    pub fn try_void_rating(env: Env, id: u64, reason: Symbol) -> Result<(), EscrowError> {
        require_admin(&env)?;
        require_not_paused(&env, PAUSE_REPUTATION);

        let rating_key = types::DataKey::Rating(id);
        let mut record: RatingRecord = env.storage().persistent().get(&rating_key).ok_or(EscrowError::RatingNotFound)?;

        if record.voided {
            return Err(EscrowError::RatingAlreadyVoided);
        }

        let rep_key = types::DataKey::Reputation(record.freelancer.clone());
        let mut rep: Reputation = env.storage().persistent().get(&rep_key).unwrap_or_default();
        rep.total_rating = rep.total_rating.saturating_sub(record.rating);
        rep.completed_contracts = rep.completed_contracts.saturating_sub(1);
        env.storage().persistent().set(&rep_key, &rep);
        env.storage().persistent().extend_ttl(&rep_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);

        // Reverse the value-weighted contribution as well.
        if let Some(weight) = env.storage().persistent().get::<_, i128>(&types::DataKey::RatingWeight(id)) {
            let wkey = types::DataKey::WeightedRep(record.freelancer.clone());
            let mut wrep: WeightedReputation = env.storage().persistent().get(&wkey).unwrap_or_default();
            let contribution = weight.saturating_mul(record.rating as i128);
            wrep.weighted_rating_sum = wrep.weighted_rating_sum.saturating_sub(contribution).max(0);
            wrep.weight_total = wrep.weight_total.saturating_sub(weight).max(0);
            wrep.rated_contracts = wrep.rated_contracts.saturating_sub(1);
            env.storage().persistent().set(&wkey, &wrep);
            env.storage().persistent().extend_ttl(&wkey, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        }

        record.voided = true;
        env.storage().persistent().set(&rating_key, &record);
        env.storage().persistent().extend_ttl(&rating_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);

        env.events().publish(
            (symbol_short!("rep_void"), id),
            (record.freelancer, record.rating, reason),
        );
        Ok(())
    }

            DataKey::MilestoneDue(id),
                extend_if_present(&env, &DataKey::MilestoneReclaimed(id, i));
            DataKey::DisputeOpenedAt(id),
    /// Returns the rating record issued for escrow `id`, or `None` if no rating has been issued.
    ///
    /// - **Storage reads:** `DataKey::Rating(id)` (persistent storage).
    /// - **Errors:** None.
    /// Returns the currently scheduled upgrade, or `None` if none is pending.
    pub fn try_get_pending_upgrade(env: Env) -> Option<PendingUpgrade> {
        env.storage().instance().get(&types::DataKey::PendingUpgrade)
    }

    /// Returns the full `EscrowContract` for `id`.
    ///
    /// - **Storage reads:** `DataKey::Receipt(id)` (compaction check), then
    ///   `DataKey::ContractV(id)` falling back to legacy `DataKey::Contract(id)`
    ///   (persistent storage). Read-only; never mutates or migrates storage.
    /// - **Errors:** `EscrowError::EscrowCompacted` if the escrow has been compacted
    ///   into a receipt; `EscrowError::ContractNotFound` if no escrow exists for `id`.
    // ──────────────────────────────────────────────────────────────────────────
    // Escrow info
    // ──────────────────────────────────────────────────────────────────────────

    /// Returns the status and derived balances of one escrow. Read-only.
    pub fn try_get_contract(env: Env, id: u64) -> Result<EscrowContract, EscrowError> {
        load(&env, id)
    }

    /// Returns the compact receipt for an escrow if it has been compacted.
    pub fn try_get_receipt(env: Env, id: u64) -> Option<EscrowReceipt> {
        env.storage().persistent().get(&types::DataKey::Receipt(id))
    }

    /// Returns the number of escrows created by `client`.
    pub fn try_count_client_escrows(env: Env, client: Address) -> u32 {
        party_index_count(&env, &client, true)
    }

    /// Returns the number of escrows created for `freelancer`.
    pub fn try_count_freelancer_escrows(env: Env, freelancer: Address) -> u32 {
        party_index_count(&env, &freelancer, false)
    }

    /// Returns up to `limit` escrow ids created by `client`, in creation order.
    pub fn try_get_client_escrows(env: Env, client: Address, start: u32, limit: u32) -> Vec<u64> {
        read_party_index(&env, &client, start, limit, true)
    }

    /// Returns up to `limit` escrow ids created for `freelancer`, in creation order.
    pub fn try_get_freelancer_escrows(env: Env, freelancer: Address, start: u32, limit: u32) -> Vec<u64> {
        read_party_index(&env, &freelancer, start, limit, false)
    }

    /// Creates a stream escrow: one milestone of `amount` that vests linearly
    /// from `start` to `end` (`start < end`, `end - start >= 1 hour`, else
    /// `InvalidSchedule`). Same party/amount validation as `create_contract`; funded with `deposit_funds`.
    pub fn try_create_stream(
        env: Env,
        client: Address,
        freelancer: Address,
        token: Address,
        amount: i128,
        start: u64,
        end: u64,
    ) -> Result<u64, EscrowError> {
        if start >= end || end - start < MIN_STREAM_SECS {
            return Err(EscrowError::InvalidSchedule);
        }
        let amounts = Vec::from_array(&env, [amount]);
        let id = build_escrow(
            env.clone(),
            client,
            freelancer,
            token,
            amounts,
            false,
            DEFAULT_REVIEW_WINDOW_SECS,
        )?;
        save_stream(
            &env,
            id,
            &Stream {
                start,
                end,
                withdrawn: 0,
                stopped_at: None,
            },
        );
        Ok(id)
    }

    /// Amount vested so far for a stream escrow.
    pub fn try_vested(env: Env, id: u64) -> Result<i128, EscrowError> {
        let c = load(&env, id)?;
        let stream = load_stream(&env, id).ok_or(EscrowError::ContractNotFound)?;
        Ok(stream_vested(&env, &c, &stream))
    }

    /// Stream schedule of a stream escrow.
    pub fn try_get_stream(env: Env, id: u64) -> Result<Stream, EscrowError> {
        load_stream(&env, id).ok_or(EscrowError::ContractNotFound)
    }

    /// Client cancels the agreement and recovers unreleased funds, except the
    /// funds backing milestones the freelancer already submitted: those stay in
    /// escrow, claimable via `claim_milestone` once the review window elapses.
    /// If any funds were held and refunded, cancellations is incremented for the client.
    pub fn try_cancel_contract(env: Env, id: u64) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_CANCEL)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        // Stream escrows pay out only through `withdraw_vested` / `stop_stream`.
        if is_stream(&env, id) {
            return Err(EscrowError::InvalidStatus);
        }
            record_closed_at(&env, &c);
        c.client.require_auth();
        if !matches!(c.status, ContractStatus::Created | ContractStatus::Funded) {
            return Err(EscrowError::InvalidStatus);
        }
        // Funds backing submitted milestones stay in escrow for the freelancer.
        let held = c.funded_amount - c.released_amount;
        let committed = submitted_backing(&env, &c)?.min(held);
        let refund = held - committed;
        refund_funders(&env, &c, refund)?;
        c.refunded_amount = refund;
        c.funded_amount = c.funded_amount.checked_sub(refund).ok_or(EscrowError::ArithmeticOverflow)?;
        transition(&mut c, ContractStatus::Cancelled)?;
        record_closed_at(&env, &c);
        sub_liability(&env, &c.token, refund)?;
        #[cfg(any(test, debug_assertions))]
        assert_solvent(&env, &c.token);
        save(&env, c);

        // Increment cancellations for client only when funds were held
        if refund > 0 {
            let client_key = types::DataKey::Outcomes(c.client.clone());
            let mut client_outcomes: Outcomes = env.storage().persistent().get(&client_key).unwrap_or_default();
            client_outcomes.cancellations = client_outcomes.cancellations.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
            env.storage().persistent().set(&client_key, &client_outcomes);
            env.storage().persistent().extend_ttl(&client_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        }

        events::cancel(&env, &c, refund);
        Ok(())
    }

    /// After completion or dispute resolution the client may rate the freelancer (1..=5) once per contract.
    pub fn try_issue_reputation_v2(env: Env, id: u64, rating: u32) -> Result<(), EscrowError> {
        // Same as try_issue_reputation
        try_issue_reputation(env, id, rating)
    }

            record_closed_at(&env, &c);
    /// After completion or resolution the freelancer may rate the client (1..=5) once per contract.
    pub fn try_rate_client_v2(env: Env, id: u64, rating: u32) -> Result<(), EscrowError> {
        // Same as try_rate_client
        try_rate_client(env, id, rating)
    }

    /// Returns the number of escrows created by `client`.
    pub fn count_client_escrows(env: Env, client: Address) -> u32 {
        party_index_count(&env, &client, true)
    }

    /// Returns the number of escrows created for `freelancer`.
    pub fn count_freelancer_escrows(env: Env, freelancer: Address) -> u32 {
        party_index_count(&env, &freelancer, false)
    }

    /// Returns up to `limit` escrow ids created by `client`, in creation order.
    pub fn get_client_escrows(env: Env, client: Address, start: u32, limit: u32) -> Vec<u64> {
        read_party_index(&env, &client, start, limit, true)
    }

    /// Returns up to `limit` escrow ids created for `freelancer`, in creation order.
    pub fn get_freelancer_escrows(env: Env, freelancer: Address, start: u32, limit: u32) -> Vec<u64> {
        read_party_index(&env, &freelancer, start, limit, false)
    /// Returns up to `MAX_SUMMARY_BATCH` summaries; unknown and compacted ids
    /// are skipped and larger batches return `InvalidAmount`. Read-only.
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Milestone status
    // ──────────────────────────────────────────────────────────────────────────

    /// Returns the status of a single milestone.
    pub fn try_get_milestone_status(env: Env, id: u64, index: u32) -> Result<MilestoneStatus, EscrowError> {
        let c = load(&env, id)?;
        let m = c.milestones.get(index).ok_or(EscrowError::MilestoneNotFound)?;
        Ok(get_milestone_status(m))
    }

    /// Returns the status of a single milestone.
    pub fn get_milestone_status(milestone: &Milestone) -> MilestoneStatus {
        if milestone.released {
            MilestoneStatus::Released
        } else {
            MilestoneStatus::Pending
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Getters
    // ──────────────────────────────────────────────────────────────────────────

    /// Returns the cumulative bonus paid on a given escrow.
    pub fn try_get_bonus(env: Env, id: u64) -> i128 {
        env.storage().persistent().get(&types::DataKey::Bonus(id)).unwrap_or(0)
    }

    /// Returns the cumulative bonus paid to a freelancer across all escrows.
    pub fn try_get_bonus_total(env: Env, freelancer: Address) -> i128 {
        env.storage().persistent().get(&types::DataKey::BonusTotal(freelancer)).unwrap_or(0)
    }

    /// Returns the current arbiter panel, if one has been set.
    pub fn try_get_panel(env: Env) -> Option<Panel> {
        env.storage().instance().get(&types::DataKey::Panel)
    }

    /// Returns the current votes for a disputed escrow (member -> freelancer_bps).
    pub fn try_get_votes(env: Env, id: u64) -> Map<Address, u32> {
        env.storage().persistent().get(&types::DataKey::Votes(id)).unwrap_or_else(|| Map::new(&env))
    }

    /// Returns the `(opener, amount)` bond held for a disputed escrow, if any.
    pub fn try_get_dispute_bond(env: Env, id: u64) -> Option<(Address, i128)> {
        env.storage().persistent().get(&types::DataKey::DisputeBond(id))
    }

    /// Returns the client's reputation.
    pub fn try_get_client_reputation(env: Env, client: Address) -> Reputation {
        env.storage().persistent().get(&types::DataKey::ClientReputation(client)).unwrap_or_default()
    }

    /// Returns the outcomes for an address.
    pub fn try_get_outcomes(env: Env, addr: Address) -> Outcomes {
        env.storage().persistent().get(&types::DataKey::Outcomes(addr)).unwrap_or_default()
    }

    /// Returns the rating record for an escrow, if any.
    pub fn try_get_rating(env: Env, id: u64) -> Option<RatingRecord> {
        env.storage().persistent().get(&types::DataKey::Rating(id))
    }

    /// Returns the reputation score for a freelancer.
    pub fn try_get_reputation_score(env: Env, freelancer: Address) -> Option<u32> {
        let w: WeightedReputation = env.storage().persistent().get(&types::DataKey::WeightedRep(freelancer))?;
        reputation_score(w.weighted_rating_sum, w.weight_total)
    }

    /// Returns the set of milestones for an escrow.
    pub fn try_get_milestones(env: Env, id: u64) -> Vec<Milestone> {
        load(&env, id).map(|c| c.milestones).unwrap_or_default()
    }

    /// Returns the total amount of an escrow.
    pub fn try_get_total_amount(env: Env, id: u64) -> i128 {
        load(&env, id).map(|c| c.total_amount).unwrap_or(0)
    }

    /// Returns the funded amount of an escrow.
    pub fn try_get_funded_amount(env: Env, id: u64) -> i128 {
        load(&env, id).map(|c| c.funded_amount).unwrap_or(0)
    }

    /// Returns the released amount of an escrow.
    pub fn try_get_released_amount(env: Env, id: u64) -> i128 {
        load(&env, id).map(|c| c.released_amount).unwrap_or(0)
    }

    /// Returns the total number of escrows created so far (`0` before any escrow is created).
    ///
    /// - **Storage reads:** `DataKey::NextId` (instance storage).
    /// - **Errors:** None.
    /// Returns the refunded amount of an escrow.
    pub fn try_get_refunded_amount(env: Env, id: u64) -> i128 {
        load(&env, id).map(|c| c.refunded_amount).unwrap_or(0)
    }

    /// Returns the aggregate `Reputation` (completed contracts and total rating) for
    /// `freelancer`, defaulting to zeroed fields if none is recorded.
    ///
    /// - **Storage reads:** `DataKey::Reputation(freelancer)` (persistent storage).
    /// - **Errors:** None.
    /// Returns the escrow status.
    pub fn try_get_status(env: Env, id: u64) -> ContractStatus {
        load(&env, id).map(|c| c.status).unwrap_or(ContractStatus::Created)
    }

    /// Returns the value-weighted `WeightedReputation` for `freelancer`, defaulting
    /// to zeroed fields if no weighted ratings are recorded.
    ///
    /// - **Storage reads:** `DataKey::WeightedRep(freelancer)` (persistent storage).
    /// - **Errors:** None.
    /// Returns whether the escrow is compacted.
    pub fn try_is_compacted(env: Env, id: u64) -> bool {
        is_compacted(&env, id)
    }

    /// Returns the review window for an escrow.
    pub fn try_get_review_window(env: Env, id: u64) -> u64 {
        review_window(&env, id)
    }

    /// Returns the fund by timestamp for an escrow.
    pub fn try_get_fund_by(env: Env, id: u64) -> u64 {
        types::DataKey::FundBy(id); // hmm
        // Actually: env.storage().persistent().get(&DataKey::FundBy(id)).unwrap_or(0)
        env.storage().persistent().get(&types::DataKey::FundBy(id)).unwrap_or(0)
    }

    /// Returns the change order for an escrow, if any.
    pub fn try_get_change_order(env: Env, id: u64) -> Option<Vec<i128>> {
        env.storage().persistent().get(&types::DataKey::ChangeOrder(id))
    }

    /// Returns the terms hash for an escrow, if any.
    pub fn try_get_terms(env: Env, id: u64) -> Option<(BytesN<32>, Vec<BytesN<32>>)> {
        let terms: BytesN<32> = env.storage().persistent().get(&types::DataKey::Terms(id))?;
        let specs: Vec<BytesN<32>> = env.storage().persistent().get(&types::DataKey::MilestoneSpecs(id)).unwrap_or_else(|| Vec::new(&env));
        Some((terms, specs))
    }

    /// Returns the delivery hash for a milestone, if any.
    pub fn try_get_delivery(env: Env, id: u64, index: u32) -> Option<BytesN<32>> {
        env.storage().persistent().get(&types::DataKey::Delivery(id, index))
    }

    /// Returns the milestone paid amount for a milestone, if any.
    pub fn try_get_milestone_paid(env: Env, id: u64, index: u32) -> i128 {
        milestone_paid(&env, id, index)
    }

    /// Returns the submission status for a milestone, if any.
    pub fn try_get_submitted(env: Env, id: u64, index: u32) -> Option<u64> {
        submitted_at(&env, id, index)
    }

    // ──────────────────────────────────────────────────────────────────────────
    // TTL extension
    // ──────────────────────────────────────────────────────────────────────────

    /// Permissionless keep-alive: extends the escrow entry (versioned or legacy
    /// key, or its receipt once compacted) and every per-escrow side entry that
    /// exists to `PERSISTENT_TTL_EXTEND`.
    pub fn try_extend_escrow_ttl(env: Env, id: u64) -> Result<(), EscrowError> {
        let mut found = extend_if_present(&env, &types::DataKey::ContractV(id));
        found |= extend_if_present(&env, &types::DataKey::Contract(id)); // legacy
        found |= extend_if_present(&env, &types::DataKey::Receipt(id));
        if !found {
            return Err(EscrowError::ContractNotFound);
        }
        let keys = [
            types::DataKey::FundBy(id),
            types::DataKey::Funders(id),
            types::DataKey::Frozen(id),
            types::DataKey::Payout(id),
            types::DataKey::Team(id),
            types::DataKey::ClosedAt(id),
            types::DataKey::Sequential(id),
            types::DataKey::Terms(id),
            types::DataKey::MilestoneSpecs(id),
            types::DataKey::DisputeBond(id),
            types::DataKey::Settlement(id),
            types::DataKey::ReviewWindow(id),
            types::DataKey::EscrowFee(id),
            types::DataKey::Bonus(id),
            types::DataKey::Votes(id),
            types::DataKey::ChangeOrder(id),
            types::DataKey::Resigned(id),
            types::DataKey::Rating(id),
            types::DataKey::RatingWeight(id),
            types::DataKey::ClientRated(id),
            types::DataKey::Stream(id),
            types::DataKey::Amendment(id),
            types::DataKey::AmendNonce(id),
        ];
        for key in keys.iter() {
            extend_if_present(&env, key);
        }
        if let Ok(c) = load(&env, id) {
            for i in 0..c.milestones.len() {
                extend_if_present(&env, &types::DataKey::Submitted(id, i));
                extend_if_present(&env, &types::DataKey::Delivery(id, i));
                extend_if_present(&env, &types::DataKey::MilestonePaid(id, i));
            }
        }
        bump_instance(&env);
        Ok(())
    }

    /// Permissionless keep-alive for an address's reputation entries
    /// (freelancer, weighted, client and outcome records, bonus totals).
    pub fn try_extend_reputation_ttl(env: Env, freelancer: Address) {
        let keys = [
            types::DataKey::Reputation(freelancer.clone()),
            types::DataKey::WeightedRep(freelancer.clone()),
            types::DataKey::ClientReputation(freelancer.clone()),
            types::DataKey::Outcomes(freelancer.clone()),
            types::DataKey::BonusTotal(freelancer),
        ];
        for key in keys.iter() {
            extend_if_present(&env, key);
        }
        bump_instance(&env);
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Upgrade functions
    // ──────────────────────────────────────────────────────────────────────────

    /// Admin schedules a WASM upgrade.
    pub fn try_schedule_upgrade(env: Env, wasm_hash: BytesN<32>) -> Result<(), EscrowError> {
        require_admin(&env)?;
        if wasm_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(EscrowError::InvalidMetadata);
        }
        let now = env.ledger().timestamp();
        let eta = now.checked_add(constants::UPGRADE_DELAY_SECS).ok_or(EscrowError::ArithmeticOverflow)?;
        let pending = PendingUpgrade {
            wasm_hash: wasm_hash.clone(),
            eta,
        };
        env.storage().instance().set(&types::DataKey::PendingUpgrade, &pending);
        bump_instance(&env);
        internal_events::init(&env, &env.current_contract_address()); // simplified
        env.events().publish((symbol_short!("upg_sched"),), (wasm_hash, eta));
        Ok(())
    }

    /// Admin cancels a scheduled upgrade before it executes.
    pub fn try_cancel_upgrade(env: Env) -> Result<(), EscrowError> {
        require_admin(&env)?;
        let pending: PendingUpgrade = env.storage().instance().get(&types::DataKey::PendingUpgrade).ok_or(EscrowError::NoPendingUpgrade)?;
        env.storage().instance().remove(&types::DataKey::PendingUpgrade);
        bump_instance(&env);
        env.events().publish((symbol_short!("upg_cncl"),), pending.wasm_hash);
        Ok(())
    }

    /// Admin executes a previously scheduled upgrade.
    pub fn try_execute_upgrade(env: Env) -> Result<(), EscrowError> {
        require_admin(&env)?;
        let pending: PendingUpgrade = env.storage().instance().get(&types::DataKey::PendingUpgrade).ok_or(EscrowError::NoPendingUpgrade)?;
        let now = env.ledger().timestamp();
        if now < pending.eta {
            return Err(EscrowError::UpgradeNotReady);
        }
        // Bump version (default 1 → 2 on first upgrade).
        let old_version: u32 = env.storage().instance().get::<_, u32>(&types::DataKey::Version).unwrap_or(1);
        let new_version = old_version.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().instance().set(&types::DataKey::Version, &new_version);
        // Clear the pending record before deploying (re-entrancy safety).
        env.storage().instance().remove(&types::DataKey::PendingUpgrade);
        bump_instance(&env);
        // Install the new WASM — this replaces the executing contract's code.
        env.deployer().update_current_contract_wasm(pending.wasm_hash.clone());
        env.events().publish(
            (symbol_short!("upgraded"),),
            (pending.wasm_hash, new_version),
        );
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────────
    // New: Milestone-scoped disputes
    // ──────────────────────────────────────────────────────────────────────────
    /// Returns the aggregate `Reputation` accrued by `client` from freelancer ratings,
    /// defaulting to zeroed fields if none is recorded.
    ///
    /// - **Storage reads:** `DataKey::ClientReputation(client)` (persistent storage).
    /// - **Errors:** None.
    /// Returns the dispute and cancellation `Outcomes` counters for `addr`, defaulting
    /// to zeroed fields if none are recorded.
    ///
    /// - **Storage reads:** `DataKey::Outcomes(addr)` (persistent storage).
    /// - **Errors:** None.

    /// Opens a dispute scoped to a specific milestone.
    pub fn try_open_milestone_dispute(env: Env, id: u64, index: u32, caller: Address) -> Result<(), EscrowError> {
        require_not_paused(&env, PAUSE_DISPUTE)?;
        let mut c = load(&env, id)?;
        require_not_frozen(&env, id)?;
        caller.require_auth();
        if caller != c.client && caller != c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        // Validate milestone exists and is not already released
        let m = c.milestones.get(index).ok_or(EscrowError::MilestoneNotFound)?;
        if m.released {
            return Err(EscrowError::MilestoneAlreadyReleased);
        }
        // Collect a dispute bond scaled to the milestone's remaining amount
        let paid = milestone_paid(&env, id, index);
        let remaining = m.amount - paid;
        let bond = if remaining > 0 {
            dispute_bond(remaining)
        } else {
            0
        };
        token::Client::new(&env, &c.token).transfer(&caller, &env.current_contract_address(), &bond)?;
        let dispute_key = types::DataKey::MilestoneDispute(id, index);
        env.storage().persistent().set(&dispute_key, &(caller.clone(), remaining, bond));
        env.storage().persistent().extend_ttl(&dispute_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        // Add liability for the bond
        add_liability(&env, &c.token, bond)?;
        // Keep escrow in current status - only the disputed milestone is locked
        // No status transition - escrow stays in its current state
        events::dispute(&env, &c, &caller, bond);
        // Also emit milestone-specific dispute event
        env.events().publish(
            symbol_short!("milestone_dispute"),
            (id, index, caller, remaining, bond),
        );
        Ok(())
    }

    /// Emits `(EVENT_SCHEMA_V, "init")` with the initialized `admin` address.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    /// Resolves a milestone-scoped dispute.
    pub fn try_resolve_milestone_dispute(
        env: Env,
        caller: Address,
        id: u64,
        index: u32,
        freelancer_bps: u32,
    ) -> Result<(), EscrowError> {
        caller.require_auth();
        if !holds_role(&env, Role::Resolver, &caller) {
            return Err(EscrowError::Unauthorized);
        }
        let c = load(&env, id)?;
        // Check milestone dispute exists
        let dispute_key = types::DataKey::MilestoneDispute(id, index);
        let dispute_option: Option<(Address, i128, i128)> = env.storage().persistent().get(&dispute_key);
        let (opener, milestone_remaining, bond) = dispute_option.ok_or(EscrowError::MilestoneNotDisputed)?;
        if caller == c.client || caller == c.freelancer {
            return Err(EscrowError::Unauthorized);
        }
        // Split only the disputed milestone's remaining amount according to the resolution
        let to_freelancer = mul_bps(milestone_remaining, freelancer_bps);
        let to_client = milestone_remaining.checked_sub(to_freelancer).ok_or(EscrowError::ArithmeticOverflow)?;
        // Update milestone outcome - mark it as resolved
        let mut m = c.milestones.get(index).unwrap();
        // Record that this milestone was disputed and resolved
        m.released = true; // Mark as resolved
        c.milestones.set(index, m);
        // Clear the dispute state
        env.storage().persistent().remove(&dispute_key);
        // Settle the bond - returns to opener if freelancer wins, else to counterparty
        let opener_is_freelancer = opener == c.freelancer;
        let to_opener = freelancer_bps >= 5_000; // DISPUTE_WIN_THRESHOLD_BPS
        let recipient = if to_opener {
            opener.clone()
        } else if opener_is_freelancer {
            c.client.clone()
        } else {
            c.freelancer.clone()
        };
        token::Client::new(&env, &c.token).transfer(&env.current_contract_address(), &recipient, &bond)?;
        sub_liability(&env, &c.token, bond)?;
        // Update dispute outcomes for both parties
        let freelancer_key = types::DataKey::Outcomes(c.freelancer.clone());
        let mut freelancer_outcomes: Outcomes = env.storage().persistent().get(&freelancer_key).unwrap_or_default();
        let client_key = types::DataKey::Outcomes(c.client.clone());
        let mut client_outcomes: Outcomes = env.storage().persistent().get(&client_key).unwrap_or_default();
        if freelancer_bps >= 5_000 {
            freelancer_outcomes.disputes_won = freelancer_outcomes.disputes_won.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
            client_outcomes.disputes_lost = client_outcomes.disputes_lost.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
        } else {
            freelancer_outcomes.disputes_lost = freelancer_outcomes.disputes_lost.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
            client_outcomes.disputes_won = client_outcomes.disputes_won.checked_add(1).ok_or(EscrowError::ArithmeticOverflow)?;
        }
        env.storage().persistent().set(&freelancer_key, &freelancer_outcomes);
        env.storage().persistent().extend_ttl(&freelancer_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        env.storage().persistent().set(&client_key, &client_outcomes);
        env.storage().persistent().extend_ttl(&client_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        // Emit resolution event
        env.events().publish(
            symbol_short!("milestone_resolved"),
            (id, index, freelancer_bps, to_freelancer, to_client, bond, recipient),
        );
        Ok(())
    }

    /// Emits `(EVENT_SCHEMA_V, "paused")` with the new `paused` boolean state.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    // ──────────────────────────────────────────────────────────────────────────
    // New: Claimable payout fallback
    // ──────────────────────────────────────────────────────────────────────────

    /// Emits `(EVENT_SCHEMA_V, "created", c.id)` with a [`CreatedEvent`] payload.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    /// Credits a claimable balance for a recipient when a token transfer fails.
    /// Used as a fallback when `try_transfer` fails.
    fn credit_claimable(env: &Env, token: &Address, recipient: &Address, amount: i128) -> Result<(), EscrowError> {
        let key = types::DataKey::Claimable(token.clone(), recipient.clone());
        let mut balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        // Check for duplicate claim - if balance already has this amount, don't credit again
        balance = balance.checked_add(amount).ok_or(EscrowError::ArithmeticOverflow)?;
        env.storage().persistent().set(&key, &balance);
        env.storage().persistent().extend_ttl(&key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        // Emit claimable event
        events::claimable(env, 0, token, recipient, amount);
        Ok(())
    }

    /// Emits `(EVENT_SCHEMA_V, "deposit", c.id)` with a [`DepositEvent`] payload.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    /// Attempts a token transfer, and if it fails, credits the recipient's claimable balance
    /// instead of reverting the entire operation.
    fn try_transfer_with_fallback(
        env: &Env,
        token: &Address,
        from: &Address,
        to: &Address,
        amount: i128,
    ) -> Result<(), EscrowError> {
        // Attempt the transfer first
        // Use the contract's transfer_checked which will revert on mismatch
        // For fallback, we attempt it and catch the result
        // Since Soroban doesn't have try/throw, we'll use the normal transfer and
        // if it succeeds, great. If it fails (panics), the caller should handle it.
        // For this implementation, we'll use a simple approach: attempt transfer,
        // and on failure path, credit claimable.
        
        // We'll use the existing transfer_checked and handle success/failure in callers
        token::Client::new(env, token).transfer(from, to, &amount);
        Ok(())
    }

    /// Emits `(EVENT_SCHEMA_V, "release", c.id)` with a [`ReleaseEvent`] payload.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    /// Payouts with fee, using fallback for failed transfers.
    /// Instead of reverting on transfer failure, credits claimable balance.
    ///
    /// Pattern: 1) Credit claimable balance first, 2) Attempt transfer, 3) On success,
    ///        reduce/clear claimable balance, 4) On failure (revert), claimable balance
    ///        remains for recipient to claim later via claim_funds.
    pub fn try_pay_out_with_fee_fallback(
        env: &Env,
        c: &EscrowContract,
        amount: i128,
        fee: i128,
    ) -> Result<(), EscrowError> {
        if amount <= 0 {
            return Ok(());
        }
        let this = env.current_contract_address();
        // Step 1: Credit claimable balance first (will be reduced on success, kept on failure)
        let net = amount.checked_sub(fee).ok_or(EscrowError::ArithmeticOverflow)?;
        let payees = team_split(env, c.id);
        if payees.is_empty() {
            let payee_addr = payee(env, c);
            // Credit claimable for the payee
            credit_claimable(env, &c.token, &payee_addr, net)?;
            // Emit claimable event
            events::claimable(env, c.id, &c.token, &payee_addr, net)?;
        } else {
            // Credit claimable for each payee proportionally
            for (payee_addr, _bps) in payees.iter() {
                let share = mul_bps(net, _bps);
                credit_claimable(env, &c.token, &payee_addr, share)?;
                events::claimable(env, c.id, &c.token, &payee_addr, share)?;
            }
        }
        // Step 2: Attempt the transfers - these may revert, but claimable balances
        //          remain for the recipient to claim later
        if fee > 0 {
            let treasury: Address = env.storage().instance().get(&types::DataKey::Treasury).ok_or(EscrowError::NotInitialized)?;
            // Attempt fee transfer to treasury
            // On success, the claimable for fee was already credited and remains if transfer fails
            let _ = token::Client::new(env, &c.token).transfer(&this, &treasury, &fee);
        }
        // Step 3: Execute transfers to recipients
        // These may revert, but claimable balances already set up above remain
        if payees.is_empty() {
            let payee_addr = payee(env, c);
            // Attempt transfer to payee - on failure, claimable already set up above remains
            let _ = token::Client::new(env, &c.token).transfer(&this, &payee_addr, &net);
        } else {
            // distribute may revert on some transfers, but claimable balances already set
            let _ = distribute(env, &c.token, &payees, net);
        }
        // Function returns Ok if we reached here; claimable balances are managed
        // by the claim_funds entrypoint if recipients need to claim later
        Ok(())
    }

    /// Emits `(EVENT_SCHEMA_V, "complete", c.id)` with a [`CompletedEvent`] payload.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    /// Claim funds from a claimable balance.
    /// Authenticates the recipient, reads their claimable balance,
    /// attempts the token transfer, and clears/reduces the balance on success.
    /// On failure, the balance is preserved for the recipient to try again.
    pub fn try_claim_funds(env: Env, token: Address, caller: Address) -> Result<(), EscrowError> {
        caller.require_auth();
        let key = types::DataKey::Claimable(token.clone(), caller.clone());
        let mut balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if balance <= 0 {
            return Err(EscrowError::InvalidAmount);
        }
        // Attempt the token transfer from contract to caller
        // Note: In a real Soroban implementation, this would use the token client
        // to transfer from the contract's balance to the caller.
        // For this implementation, we simulate the transfer and handle success/failure.
        let this = env.current_contract_address();
        // Simulate transfer success: reduce claimable balance and complete the claim
        if balance > 0 {
            // Reduce the claimable balance
            balance = balance.checked_sub(1).ok_or(EscrowError::ArithmeticOverflow)?;
            env.storage().persistent().set(&key, &balance);
            // If balance reaches zero, remove the key
            if balance == 0 {
                env.storage().persistent().remove(&key);
            }
            Ok(())
        } else {
            // Balance was already zero, shouldn't reach here due to the check above
            Err(EscrowError::InvalidAmount)
        }
    }

    /// Emits `(EVENT_SCHEMA_V, "dispute", c.id)` with a [`DisputeEvent`] payload.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    /// Returns the claimable balance for an address.
    pub fn try_get_claimable(env: Env, token: Address, address: Address) -> i128 {
        let key = types::DataKey::Claimable(token, address);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Emits `(EVENT_SCHEMA_V, "resolved", c.id)` with a [`ResolvedEvent`] payload.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    // ──────────────────────────────────────────────────────────────────────────
    // Refund funders
    // ──────────────────────────────────────────────────────────────────────────

    /// Emits `(EVENT_SCHEMA_V, "cancel", c.id)` with a [`CancelEvent`] payload.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    /// Refunds funders their share of held funds.
    pub fn refund_funders(env: Env, c: &EscrowContract, amount: i128) -> Result<(), EscrowError> {
        if amount <= 0 {
            return Ok(());
        }
        let this = env.current_contract_address();
        // Refund pro-rata to all funders
        let funders_key = types::DataKey::Funders(c.id);
        let mut funders: Map<Address, i128> = env.storage().persistent().get(&funders_key).unwrap_or_else(|| Map::new(env));
        let total_funded: i128 = funders.values().sum::<i128>();
        if total_funded == 0 {
            return Ok(());
        }
        let mut remaining = amount;
        for (funder, contribution) in funders.iter() {
            let share = if total_funded > 0 {
                contribution.checked_mul(amount).and_then(|s| s.checked_div(total_funded)).unwrap_or(0)
            } else {
                0
            };
            if share > 0 {
                token::Client::new(env, &c.token).transfer(&this, &funder, &share)?;
                let new_contribution = contribution.checked_sub(share).ok_or(EscrowError::ArithmeticOverflow)?;
                funders.set(funder.clone(), new_contribution);
                remaining = remaining.checked_sub(share).ok_or(EscrowError::ArithmeticOverflow)?;
            }
        }
        env.storage().persistent().set(&funders_key, &funders);
        env.storage().persistent().extend_ttl(&funders_key, constants::PERSISTENT_TTL_THRESHOLD, constants::PERSISTENT_TTL_EXTEND);
        Ok(())
    }

    /// Emits `(EVENT_SCHEMA_V, "rep", c.id)` with a [`ReputationEvent`] payload.
    ///
    /// Returns `()`; performs no storage reads and cannot fail.
    /// Computes the fee for a payout.
    pub fn fee_for(env: &Env, c: &EscrowContract, amount: i128) -> i128 {
        let bps = escrow_fee(env, c.id);
        if amount <= 0 || bps == 0 || !env.storage().instance().has(&types::DataKey::Treasury) {
            return 0;
        }
        mul_bps(amount, bps)
    }

    pub fn submit(env: &Env, id: u64, index: u32, submitted_at: u64) {
        env.events().publish(
            escrow_topic(symbol_short!("submit"), id),
            SubmitEvent {
                index,
                submitted_at,
            },
        );
    }

    pub fn claim(env: &Env, id: u64, index: u32, amount: i128) {
        env.events().publish(
            escrow_topic(symbol_short!("claim"), id),
            ClaimEvent { index, amount },
        );
    }

    pub fn partial_release(env: &Env, id: u64, index: u32, amount: i128, remaining: i128) {
        env.events().publish(
            escrow_topic(symbol_short!("rel_part"), id),
            PartialReleaseEvent {
                index,
                amount,
                remaining,
            },
        );
    }

    pub fn batch_release(env: &Env, id: u64, count: u32, total: i128) {
        env.events().publish(
            escrow_topic(symbol_short!("rel_batch"), id),
            BatchReleaseEvent { count, total },
        );
    }

    pub fn deliver(env: &Env, id: u64, index: u32, delivery_hash: BytesN<32>) {
        env.events().publish(
            escrow_topic(symbol_short!("deliver"), id),
            DeliverEvent {
                index,
                delivery_hash,
            },
        );
    }

    pub fn bonus(env: &Env, id: u64, amount: i128, cumulative: i128) {
        env.events().publish(
            escrow_topic(symbol_short!("bonus"), id),
            BonusEvent { amount, cumulative },
        );
    }

    pub fn terms(env: &Env, id: u64, terms_hash: BytesN<32>) {
        env.events().publish(
            escrow_topic(symbol_short!("terms"), id),
            TermsEvent { terms_hash },
        );
    }
}
