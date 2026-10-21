//! Versioned, structured event catalogue for the TalentTrust escrow contract.
//!
//! Every mutating entry-point emits exactly one event through this module.
//! Topics always carry a schema-version sentinel `EVENT_SCHEMA_V = 1` as their
//! first element so indexers can branch on version without re-deploying.
//!
//! ## Topic shapes
//! | Class | Topics tuple | Data type |
//! |---|---|---|
//! | Escrow-scoped | `(1u32, symbol, contract_id: u32)` | event struct |
//! | Global (no contract) | `(1u32, symbol)` | event struct |
//!
//! The symbols are kept identical to the legacy ad-hoc events so existing
//! subscriptions that filter on the symbol still work.

use crate::ContractStatus;
use soroban_sdk::{contracttype, Address, Env, Symbol};

/// Schema version sentinel.  Bump this when any payload struct layout changes.
pub const EVENT_SCHEMA_V: u32 = 1;

// ── Payload structs ──────────────────────────────────────────────────────────

/// Emitted when a new escrow contract is created.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedEvent {
    pub client: Address,
    pub freelancer: Address,
    pub token: Option<Address>,
    pub total: i128,
    pub milestone_count: u32,
}

/// Emitted when funds are deposited into an escrow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositEvent {
    pub from: Address,
    pub amount: i128,
    pub funded_amount: i128,
    pub status: ContractStatus,
}

/// Emitted when a milestone is released to the freelancer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseEvent {
    pub index: u32,
    pub amount: i128,
    pub fee: i128,
    pub to: Address,
    pub released_amount: i128,
    pub status: ContractStatus,
}

/// Emitted when all milestones are complete and the contract reaches `Completed`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedEvent {
    pub released_amount: i128,
}

/// Emitted when a dispute is opened on an escrow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeEvent {
    pub by: Address,
    pub status: ContractStatus,
}

/// Emitted when an open dispute is resolved by the arbiter.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedEvent {
    pub to_freelancer: i128,
    pub to_client: i128,
    pub status: ContractStatus,
}

/// Emitted when an escrow contract is cancelled.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelEvent {
    pub refund: i128,
    pub to: Address,
    pub status: ContractStatus,
}

/// Emitted when milestone refunds are issued.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundedEvent {
    pub amount: i128,
    pub status: ContractStatus,
}

/// Emitted when reputation is issued for a freelancer after contract completion.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationEvent {
    pub freelancer: Address,
    pub rating: u32,
    pub completed_contracts: i128,
    pub total_rating: i128,
}

/// Emitted when work evidence is submitted for a milestone.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceEvent {
    pub milestone_index: u32,
    pub freelancer: Address,
}

/// Emitted when the contract is initialized with an admin.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitEvent {
    pub admin: Address,
}

/// Emitted when the contract is paused.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PausedEvent {
    pub admin: Address,
}

/// Emitted when the contract is unpaused.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnpausedEvent {
    pub admin: Address,
}

/// Emitted when the emergency pause is activated.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyActivatedEvent {
    pub admin: Address,
}

/// Emitted when the emergency is resolved.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyResolvedEvent {
    pub admin: Address,
}

/// Emitted when the settlement token is bound.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementTokenBoundEvent {
    pub admin: Address,
    pub token: Address,
}

// ── Emitter functions ────────────────────────────────────────────────────────

pub fn emit_created(env: &Env, contract_id: u32, payload: CreatedEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "created"), contract_id),
        payload,
    );
}

pub fn emit_deposit(env: &Env, contract_id: u32, payload: DepositEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "deposit"), contract_id),
        payload,
    );
}

pub fn emit_release(env: &Env, contract_id: u32, payload: ReleaseEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "mlstn_rls"), contract_id),
        payload,
    );
}

pub fn emit_completed(env: &Env, contract_id: u32, payload: CompletedEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "complete"), contract_id),
        payload,
    );
}

pub fn emit_dispute(env: &Env, contract_id: u32, payload: DisputeEvent) {
    env.events().publish(
        (
            EVENT_SCHEMA_V,
            Symbol::new(env, "dispute_opened"),
            contract_id,
        ),
        payload,
    );
}

pub fn emit_resolved(env: &Env, contract_id: u32, payload: ResolvedEvent) {
    env.events().publish(
        (
            EVENT_SCHEMA_V,
            Symbol::new(env, "dispute_resolved"),
            contract_id,
        ),
        payload,
    );
}

pub fn emit_cancel(env: &Env, contract_id: u32, payload: CancelEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "cancelled"), contract_id),
        payload,
    );
}

pub fn emit_refunded(env: &Env, contract_id: u32, payload: RefundedEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "refunded"), contract_id),
        payload,
    );
}

pub fn emit_reputation(env: &Env, contract_id: u32, payload: ReputationEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "reputation"), contract_id),
        payload,
    );
}

pub fn emit_evidence(env: &Env, contract_id: u32, payload: EvidenceEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "evidence"), contract_id),
        payload,
    );
}

pub fn emit_init(env: &Env, payload: InitEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "init")),
        payload,
    );
}

pub fn emit_paused(env: &Env, payload: PausedEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "paused")),
        payload,
    );
}

pub fn emit_unpaused(env: &Env, payload: UnpausedEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "unpaused")),
        payload,
    );
}

pub fn emit_emergency_activated(env: &Env, payload: EmergencyActivatedEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "emergency_on")),
        payload,
    );
}

pub fn emit_emergency_resolved(env: &Env, payload: EmergencyResolvedEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "emergency_off")),
        payload,
    );
}

pub fn emit_settlement_token_bound(env: &Env, payload: SettlementTokenBoundEvent) {
    env.events().publish(
        (EVENT_SCHEMA_V, Symbol::new(env, "token_bound")),
        payload,
    );
}
