//! Centralised emission of the versioned, structured events.
//! Public re-exported from the parent `lib.rs` module.

use soroban_sdk::Env;
use soroban_sdk::symbol_short;

use super::types::{CreatedEvent, DepositEvent, ReleaseEvent, DisputeEvent, ResolvedEvent, CancelEvent, ReputationEvent, CompletedEvent, EscrowContract, MilestoneDisputeEvent, MilestoneResolvedEvent, ClaimableEvent};

/// Schema version carried as the first topic of every structured event.
pub const EVENT_SCHEMA_V: u32 = 1;

/// `(1, "init", admin)` — the contract was initialized.
pub fn init(env: &Env, admin: &soroban_sdk::Address) {
    env.events().publish((EVENT_SCHEMA_V, symbol_short!("init")), admin.clone());
}

/// `(1, "paused", paused)` — pause state change.
pub fn paused(env: &Env, paused: bool) {
    env.events().publish((EVENT_SCHEMA_V, symbol_short!("paused")), paused);
}

/// `(1, "created", id)` — a new escrow was created.
pub fn created(env: &Env, c: &EscrowContract) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("created")),
        CreatedEvent {
            client: c.client.clone(),
            freelancer: c.freelancer.clone(),
            token: c.token.clone(),
            total: c.total_amount,
            milestone_count: c.milestones.len(),
        },
    );
}

/// `(1, "deposit", id)` — tokens were deposited into an escrow.
pub fn deposit(env: &Env, c: &EscrowContract, from: &soroban_sdk::Address, amount: i128) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("deposit")),
        DepositEvent {
            from: from.clone(),
            amount,
            funded_amount: c.funded_amount,
            status: c.status,
        },
    );
}

/// `(1, "release", id)` — a milestone was paid out.
pub fn release(
    env: &Env,
    c: &EscrowContract,
    index: u32,
    amount: i128,
    to: &soroban_sdk::Address,
    released_amount: i128,
    fee: i128,
) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("release")),
        ReleaseEvent {
            index,
            amount,
            to: to.clone(),
            released_amount,
            status: c.status,
            fee,
        },
    );
}

/// `(1, "completed", id)` — the last milestone was released.
pub fn completed(env: &Env, c: &EscrowContract) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("complete")),
        CompletedEvent {
            released_amount: c.released_amount,
        },
    );
}

/// `(1, "dispute", id)` — a party opened a dispute.
pub fn dispute(env: &Env, c: &EscrowContract, by: &soroban_sdk::Address, bond: i128) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("dispute")),
        DisputeEvent {
            by: by.clone(),
            status: c.status,
            bond,
        },
    );
}

/// `(1, "milestone_dispute", id, index)` — a dispute was opened for a specific milestone.
pub fn milestone_dispute(env: &Env, id: u64, index: u32, caller: &soroban_sdk::Address, remaining: i128, bond: i128) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("milestone_dispute")),
        super::types::MilestoneDisputeEvent {
            id,
            index,
            caller: caller.clone(),
            remaining: remaining,
            bond,
        },
    );
}

/// `(1, "resolved", id)` — a dispute was settled.
pub fn resolved(
    env: &Env,
    c: &EscrowContract,
    to_freelancer: i128,
    to_client: i128,
    bond_recipient: Option<soroban_sdk::Address>,
    bond: i128,
) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("resolved")),
        ResolvedEvent {
            to_freelancer,
            to_client,
            status: c.status,
            bond_recipient,
            bond,
        },
    );
}

/// `(1, "milestone_resolved", id, index, bps, freelancer_amount, client_amount, bond, recipient)` — a milestone-scoped dispute was resolved.
pub fn milestone_resolved(
    env: &Env,
    id: u64,
    index: u32,
    freelancer_bps: u32,
    to_freelancer: i128,
    to_client: i128,
    bond: i128,
    recipient: &soroban_sdk::Address,
) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("milestone_resolved")),
        super::types::MilestoneResolvedEvent {
            id,
            index,
            freelancer_bps,
            to_freelancer,
            to_client,
            bond,
            recipient: recipient.clone(),
        },
    );
}

/// `(1, "cancel", id)` — the client cancelled; `refund` went back to the funders.
pub fn cancel(env: &Env, c: &EscrowContract, refund: i128) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("cancel")),
        CancelEvent {
            refund,
            to: c.client.clone(),
            status: c.status,
        },
    );
}

/// `(1, "rep", id)` — the client rated the freelancer.
pub fn reputation(env: &Env, c: &EscrowContract, rating: u32, rep: &super::types::Reputation, weight: i128) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("rep")),
        ReputationEvent {
            freelancer: c.freelancer.clone(),
            rating,
            completed_contracts: rep.completed_contracts,
            total_rating: rep.total_rating,
            weight,
        },
    );
}

/// `(1, "claimable", id, token, address, amount)` — a claimable balance was credited.
pub fn claimable(env: &Env, id: u64, token: &soroban_sdk::Address, address: &soroban_sdk::Address, amount: i128) {
    env.events().publish(
        (EVENT_SCHEMA_V, symbol_short!("claimable")),
        super::types::ClaimableEvent {
            id,
            token: token.clone(),
            address: address.clone(),
            amount,
        },
    );
}