//! TalentTrust escrow contract for milestone-based freelancer payments.
//!
//! The crate root exposes the Soroban contract and still owns several public
//! entrypoints directly: initialization, settlement-token binding, deposits,
//! milestone release/refund/cancel flows, reputation, work evidence, protocol
//! fee withdrawal, and dispute entrypoints. Supporting modules keep reusable
//! validation, storage, governance, and lifecycle helpers close to the paths
//! that use them.
//!
//! ## Escrow source tree map
//!
//! | Source | Responsibility | Storage keys owned or touched |
//! | --- | --- | --- |
//! | `lib.rs` | Contract wrapper plus root entrypoints for setup, custody, money movement, reads, reputation, work evidence, pause/emergency, fee withdrawal, and dispute orchestration. | `DataKey::Initialized`, `Admin`, `SettlementToken`, `Paused`, `Emergency`, `ReadinessChecklist`, `Contract(id)`, `(Contract(id), "milestones")`, `MilestoneApprovals`, `AccumulatedProtocolFees`, `ReputationIssued`, `PendingReputationCredits`, `Reputation`, `ReputationComment` |
//! | `amount_validation` | Stateless validation and checked arithmetic for stroop amounts and milestone totals. | None directly; callers write validated amounts to `Contract(id)` and milestone vectors. |
//! | `approvals` | Temporary milestone release approvals and release-authorization checks. | Temporary `DataKey::MilestoneApprovals(contract_id, milestone_index)`; reads `Contract(id)` and `(Contract(id), "milestones")`. |
//! | `deposit` | Deposit preflight and post-transfer accounting used by `deposit_funds`. | `DataKey::Contract(contract_id)` and `(DataKey::Contract(contract_id), "milestones")`. |
//! | `finalize` | Immutable finalization records, finalization guards, and final contract summaries. | `DataKey::Finalization(contract_id)`; reads `Contract(id)`, `(Contract(id), "milestones")`, `Paused`, and `Emergency`. |
//! | `migration` | Client migration proposals, acceptance checks, cancellation, and pending-migration reads. | Temporary `DataKey::PendingClientMigration(contract_id)`; reads and updates `DataKey::Contract(contract_id)`. |
//! | `ttl` | TTL constants plus helpers for temporary and persistent storage renewal. | Extends caller-provided keys, especially `Contract(id)`, `(Contract(id), "milestones")`, `NextContractId`, participant indexes, approvals, and migrations. |
//! | `types` | Shared Soroban types, error enums, summaries, governance records, dispute records, and the canonical `DataKey` enum. | Declares storage key schema only; does not access storage itself. |
//! | `utils` | Small deterministic helpers shared by entrypoints, currently ledger timestamp access. | None. |
//! | `create_contract` | Contract creation, participant/milestone validation, ID allocation, and creation events. | `DataKey::Contract(id)`, `(DataKey::Contract(id), "milestones")`, `NextContractId`, and `GovernedParameters`. |
//! | `dispute` | Pure dispute payout arithmetic and final-status selection for dispute resolution. | None directly; root dispute entrypoints update `DataKey::Contract(contract_id)`. |
//! | `governance` | Admin-controlled protocol fee, governed parameter, readiness, and admin-rotation entrypoints. | `DataKey::Admin`, `ProtocolFeeBps`, `PendingAdmin`, `GovernedParameters`, and `ReadinessChecklist`. |
//!
//! Generate this map with `cargo doc -p escrow --no-deps` and open
//! `target/doc/escrow/index.html`.
#![no_std]
#![allow(clippy::derivable_impls)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::useless_vec)]
#![allow(clippy::let_and_return)]
#![allow(clippy::inconsistent_digit_grouping)]
#![allow(clippy::int_plus_one)]
#![allow(clippy::duplicated_attributes)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::module_inception)]
#![allow(clippy::single_match)]
#![allow(clippy::useless_conversion)]

mod amount_validation;
mod approvals;
mod deposit;
mod finalize;
mod migration;
mod ttl;
mod types;
mod utils;

use crate::utils::now_seconds;
use soroban_sdk::{
    contract, contracterror, contractimpl, log, symbol_short, token, Address, Env, String, Symbol,
    Vec,
};

pub use amount_validation::accumulate_amounts;
pub use amount_validation::safe_add_amounts;
pub use amount_validation::safe_subtract_amounts;
pub use amount_validation::validate_deposit_amount;
pub use amount_validation::validate_milestone_amounts;
pub use amount_validation::validate_single_amount;
pub use dispute::final_status_after_resolution;
pub use dispute::resolution_payouts;
pub use migration::PendingClientMigration;
pub use ttl::{ADMIN_ROTATION_MIN_DELAY_LEDGERS, PENDING_MIGRATION_TTL_LEDGERS};
// Keep shared storage keys and escrow domain types centralized in `types.rs`.
// `DisputeResolution` and `DisputeSplit` are defined once in `types.rs` and
// re-exported here; `dispute.rs` uses them via `crate::DisputeResolution`.
pub use types::{
    Contract, ContractBounds, ContractStatus, ContractSummary, DataKey, DepositMode,
    DisputeResolution, DisputeSplit, Error, GovernedParameters, Milestone, MilestoneApprovals,
    MilestoneSummary, PendingAdminProposal, ReadinessChecklist, ReleaseAuthorization, Reputation,
    SplitAmounts, CONTRACT_SUMMARY_SCHEMA_VERSION,
};

// Maximum bounds constants - re-export from amount_validation for API visibility
pub const MAX_MILESTONES: u32 = 10;
pub const MAX_SINGLE_AMOUNT_STROOPS: i128 = crate::amount_validation::MAX_SINGLE_AMOUNT_STROOPS;
pub const MAX_TOTAL_ESCROW_STROOPS: i128 = MAX_SINGLE_AMOUNT_STROOPS;

#[contract]
pub struct Escrow;

mod create_contract;
mod dispute;
mod governance;

/// Governance-level errors for admin-gated operations.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowError {
    InvalidParticipant = 1,
    EmptyMilestones = 2,
    InvalidMilestoneAmount = 3,
    InvalidDepositAmount = 4,
    InvalidMilestone = 5,
    ContractNotFound = 6,
    EmptyRefundRequest = 7,
    DuplicateMilestoneInRefund = 8,
    AlreadyReleased = 9,
    AlreadyRefunded = 10,
    InsufficientFunds = 11,
    AlreadyInitialized = 12,
    InsufficientAccumulatedFees = 13,
    /// Returned by lifecycle entrypoints when `initialize` has not been called.
    ///
    /// All money-flow operations require initialization so the admin-controlled
    /// safety rails (pause, emergency controls, protocol fees) are always in
    /// scope before any funds can move.
    NotInitialized = 14,
    UnauthorizedRole = 15,
    ContractPaused = 16,
    EmergencyActive = 17,
    InvalidState = 18,
    InvalidRating = 19,
    SelfRating = 20,
    ReputationAlreadyIssued = 21,
    NotCompleted = 22,
    FreelancerMismatch = 23,
    InvalidStatusTransition = 24,
    ArbiterRequired = 25,
    InvalidDisputeSplit = 26,
    AccountingInvariantViolated = 27,
    PotentialOverflow = 28,
    AlreadyFinalized = 29,
    AmountMustBePositive = 30,
    /// No settlement token has been bound for custody transfers.
    SettlementTokenNotConfigured = 31,
    /// A settlement token has already been bound.
    SettlementTokenAlreadyBound = 32,
    /// The sum of milestone amounts exceeded the configured maximum or overflowed.
    TotalCapExceeded = 33,
    /// Too many milestones were provided.
    TooManyMilestones = 34,
    /// An arbiter was required by the release authorization mode but not provided.
    MissingArbiter = 35,
    /// The provided arbiter is invalid (same as client or freelancer).
    InvalidArbiter = 36,
    /// Contract is cancelled and must not accept further value-moving operations.
    ContractCancelled = 37,
    /// Contract has been refunded and is terminal for value-moving operations.
    ContractRefunded = 38,
    /// The address supplied as settlement token is not a valid token contract.
    /// The pre-bind probe called `token::Client::balance` against the escrow
    /// contract address and the call panicked — the address does not implement
    /// the SAC token interface.
    InvalidSettlementToken = 39,
    /// The address supplied as settlement token is the escrow contract itself.
    /// Binding self would create a circular custody reference and brick all
    /// transfer paths.
    SettlementTokenIsSelf = 40,
    /// The address supplied as settlement token is the escrow admin.
    /// Binding the admin as the custody asset conflates governance authority
    /// with the settlement token role.
    SettlementTokenIsAdmin = 41,
    /// Reputation feedback comment was empty.
    EmptyComment = 42,
    /// Reputation feedback comment exceeded the 200-character maximum.
    CommentTooLong = 43,
}

impl Escrow {
    /// Get the settlement token address from the canonical `DataKey` binding.
    pub(crate) fn read_settlement_token(env: &Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::SettlementToken)
    }

    /// Persist the settlement token address under the canonical `DataKey` binding.
    pub(crate) fn write_settlement_token(env: &Env, token: &Address) {
        env.storage()
            .persistent()
            .set(&DataKey::SettlementToken, token);
    }
}

#[contractimpl]
impl Escrow {
    /// Bind the single Stellar Asset Contract (SAC) token this escrow instance will custody.
    ///
    /// This is a **write-once** step: once a token is recorded under
    /// [`DataKey::SettlementToken`] all subsequent money-flow entrypoints
    /// (`deposit_funds`, `release_milestone`, `refund_unreleased_milestones`,
    /// `cancel_contract`, `withdraw_protocol_fees`) read that address to execute SAC
    /// `transfer` calls.  A second call with any token address is rejected with
    /// `SettlementTokenAlreadyBound`.
    ///
    /// # Pre-bind probe (issue #723)
    ///
    /// Before persisting the token address, this entrypoint performs a **read-only
    /// probe** to verify the supplied address is a live SAC token contract:
    ///
    /// 1. Calls `token::Client::balance(env.current_contract_address())` against
    ///    the candidate address. If the address does not implement the SAC token
    ///    interface, the call panics and the bind is rejected with
    ///    `InvalidSettlementToken`.
    /// 2. Rejects `env.current_contract_address()` (the escrow contract itself)
    ///    with `SettlementTokenIsSelf` — binding self creates a circular custody
    ///    reference.
    /// 3. Rejects the stored admin address with `SettlementTokenIsAdmin` —
    ///    conflating governance authority with the settlement token role is a
    ///    privilege-separation violation.
    ///
    /// # Reentrancy mitigation
    ///
    /// All downstream money-flow entrypoints (`deposit_funds`, `release_milestone`,
    /// `cancel_contract`, `refund_unreleased_milestones`) follow strict
    /// **state-before-transfer** (Checks-Effects-Interactions) ordering: contract
    /// state is finalized *before* any `token::Client::transfer` call.  A
    /// malicious token contract that re-enters the escrow during a transfer will
    /// observe the already-mutated state and cannot double-spend or front-run
    /// the operation.  The probe itself performs no state mutation — it only
    /// reads the token balance — so it cannot be used as a reentrancy vector.
    ///
    /// See [`docs/escrow/sac-custody.md`](../../../docs/escrow/sac-custody.md) for the
    /// full custody model, accounting invariant, and lifecycle sequence diagram.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `admin` - The admin address (must match stored admin)
    /// * `token` - The SAC token address
    ///
    /// # Errors
    /// * `NotInitialized` if `initialize` has not been called
    /// * `UnauthorizedRole` if `admin` is not the stored admin
    /// * `SettlementTokenAlreadyBound` if a token is already bound
    /// * `InvalidSettlementToken` if the probe call to `token::Client::balance` panics
    /// * `SettlementTokenIsSelf` if `token == env.current_contract_address()`
    /// * `SettlementTokenIsAdmin` if `token == stored_admin`
    ///
    /// # Events
    /// On a successful, authorized bind this publishes a `settlement_token_bound`
    /// event so off-chain indexers and monitoring dashboards can observe which
    /// asset an escrow settles in, and when the binding happened.
    ///
    /// * Topics: `(Symbol "settlement_token_bound",)`
    /// * Data: `(admin: Address, token: Address, timestamp: u64)`
    ///
    /// The event only fires after the write succeeds. Rejected binds
    /// (uninitialized, unauthorized, invalid token, self, admin) panic before
    /// this point and therefore publish nothing. All payload fields are public
    /// configuration.
    pub fn bind_settlement_token(env: Env, admin: Address, token: Address) -> bool {
        Self::require_initialized(&env);
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));

        if admin != stored_admin {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }
        admin.require_auth();

        // Reject double-bind: once a settlement token is recorded, any
        // subsequent bind attempt is rejected. This is a write-once field.
        if Self::read_settlement_token(&env).is_some() {
            env.panic_with_error(EscrowError::SettlementTokenAlreadyBound);
        }

        // ── Pre-bind probe (issue #723) ─────────────────────────────────────
        //
        // Reject the escrow contract's own address — binding self would create
        // a circular custody reference and brick every transfer path.
        if token == env.current_contract_address() {
            env.panic_with_error(EscrowError::SettlementTokenIsSelf);
        }

        // Reject the admin address — conflating governance authority with the
        // settlement token role is a privilege-separation violation.
        if token == stored_admin {
            env.panic_with_error(EscrowError::SettlementTokenIsAdmin);
        }

        // Read-only probe: call `token::Client::balance` against the escrow
        // contract address. If `token` does not implement the SAC token
        // interface, the host panics and we translate that into
        /// `InvalidSettlementToken`.
        //
        // This is safe because:
        // - `balance` is a read-only entrypoint (no state mutation on the
        //   token contract).
        // - We have not yet written anything to storage — a panic here leaves
        //   no partial state.
        // - The probe cannot be used for reentrancy: it calls `balance`, not
        //   `transfer`, and the escrow has no callback the token could invoke.
        let token_client = token::Client::new(&env, &token);
        let _probe: i128 = token_client.balance(&env.current_contract_address());

        Self::write_settlement_token(&env, &token);

        // Emit after the binding write succeeds so indexers can track the bound
        // asset. Consistent topic naming with `init` / `protocol_fee_bps` events.
        env.events().publish(
            (Symbol::new(&env, "settlement_token_bound"),),
            (admin, token, env.ledger().timestamp()),
        );
        true
    }

    /// Deprecated thin delegate for [`bind_settlement_token`](Self::bind_settlement_token).
    ///
    /// Retained for backward compatibility with external callers that used the historical API name.
    /// Delegates directly to [`bind_settlement_token`](Self::bind_settlement_token) and inherits
    /// every security guard (`SettlementTokenAlreadyBound`, admin auth check, SAC interface probe,
    /// self/admin validation) and event emission.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `admin` - The admin address (must match stored admin)
    /// * `token` - The SAC token address
    ///
    /// # Deprecated
    /// Use [`bind_settlement_token`](Self::bind_settlement_token) instead.
    #[deprecated(note = "Use bind_settlement_token instead.")]
    pub fn set_settlement_token(env: Env, admin: Address, token: Address) -> bool {
        Self::bind_settlement_token(env, admin, token)
    }

    /// Returns the bound settlement token, or `None` if no token has been bound.
    pub fn get_settlement_token(env: Env) -> Option<Address> {
        Self::read_settlement_token(&env)
    }

    /// Returns `true` exactly when a settlement token is bound.
    ///
    /// This is the recommended cheap pre-flight readiness check before calling
    /// `deposit_funds`, which panics when no settlement token has been bound.
    /// Integrators that only need to know *whether* the escrow can accept
    /// deposits — without caring about the specific token address — should use
    /// this instead of fetching and discarding the `Address` from
    /// `get_settlement_token`.
    ///
    /// Read-only and auth-free: it performs no state mutation (no TTL write is
    /// needed for the simple binding key).
    ///
    /// # Returns
    /// * `true` if a settlement token is bound
    /// * `false` if no settlement token has been bound yet
    pub fn is_settlement_token_bound(env: Env) -> bool {
        Self::read_settlement_token(&env).is_some()
    }

    // ── Initialization ───────────────────────────────────────────────────────

    /// Initializes the escrow contract with the operational admin.
    ///
    /// Single-use. Stores the admin address that controls pause, emergency,
    /// protocol-fee, and governance operations. All escrow lifecycle operations
    /// (create, deposit, release, refund, cancel) call `require_initialized`
    /// so that these safety rails are always bound before money can move.
    pub fn initialize(env: Env, admin: Address) -> bool {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().persistent().set(&DataKey::Initialized, &true);
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::NextContractId, &1u32);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default();
        checklist.initialized = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (symbol_short!("init"), Symbol::new(&env, "admin_set")),
            (admin.clone(), env.ledger().timestamp()),
        );

        true
    }

    /// Returns the stored governance admin address.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    /// Returns the protocol-wide hard-coded bounds used by validation paths.
    ///
    /// Callers and off-chain indexers should query this endpoint to discover
    /// the limits enforced by `create_contract` without relying on hard-coded
    /// constants:
    ///
    /// - `max_milestones`: maximum number of milestones per contract.
    /// - `max_single_milestone_stroops`: maximum amount for any single milestone.
    /// - `max_total_escrow_stroops`: maximum sum of all milestone amounts.
    /// - `max_fee_bps`: protocol fee ceiling in basis points (10 000 = 100 %).
    ///
    /// These are compile-time constants — the return value never changes
    /// between calls on the same contract binary. The function is read-only
    /// and requires no authorization.
    ///
    /// # Returns
    /// A [`ContractBounds`] value containing only limit fields. Unlike
    /// [`get_contract_summary`], this type carries no per-contract participant
    /// or accounting data and its schema version tracks the limits API only.
    pub fn get_bounds(_env: Env) -> ContractBounds {
        ContractBounds {
            max_milestones: MAX_MILESTONES,
            max_single_milestone_stroops: MAX_SINGLE_AMOUNT_STROOPS,
            max_total_escrow_stroops: MAX_TOTAL_ESCROW_STROOPS,
            max_fee_bps: 10_000,
        }
    }

    /// Returns the current mainnet readiness checklist.
    ///
    /// The checklist tracks critical configuration steps that must be completed
    /// before the escrow contract is considered ready for mainnet production:
    ///
    /// - **`initialized`**: Flipped to `true` when `initialize` completes successfully.
    ///   Ensures that an admin has been bound to the contract.
    /// - **`governed_params_set`**: Flipped to `true` when governance/protocol parameters
    ///   (such as fees and maximum caps) are configured. Flipped during `initialize_protocol_governance`
    ///   or parameter updates.
    /// - **`emergency_controls_enabled`**: Flipped to `true` when emergency pause controls are exercised
    ///   for the first time (via `activate_emergency_pause`). This verifies the operator has functioning
    ///   emergency access.
    ///
    /// # Implications for a Clean Deploy
    /// Activating the emergency pause to flip the `emergency_controls_enabled` flag leaves the contract
    /// in a paused state. To complete a clean deploy and allow normal operations, the operator must
    /// subsequently call `resolve_emergency` to unpause the contract.
    pub fn get_mainnet_readiness_info(env: Env) -> ReadinessChecklist {
        env.storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default()
    }

    /// Creates a new escrow contract with the specified client, freelancer, and milestone amounts.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `client` - The address of the client funding the contract
    /// * `freelancer` - The address of the freelancer performing the work
    /// * `arbiter` - Optional arbiter address for dispute resolution
    /// * `milestones` - Vector of milestone amounts (in stroops)
    /// * `release_authorization` - Authorization mode for milestone releases
    ///
    /// # Returns
    /// The unique contract ID
    ///
    /// # Errors
    /// * `InvalidParticipants` - If client and freelancer are the same address
    /// * `EmptyMilestones` - If no milestones are provided
    /// * `InvalidMilestoneAmount` - If any milestone amount is <= 0
    /// Pull the settlement-token deposit from the client into the escrow contract address.
    ///
    /// Executes `SAC::transfer(from: client, to: escrow_address, amount)` and advances
    /// status from `Created` to `Funded` once the full milestone sum has been deposited.
    /// Requires `bind_settlement_token` to have been called first; panics with
    /// `SettlementTokenNotConfigured` otherwise.
    ///
    /// See [`docs/escrow/sac-custody.md`](../../../docs/escrow/sac-custody.md) for the
    /// full custody model and accounting invariant.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `caller` - The address of the caller (must be the client)
    /// * `amount` - The amount to deposit (in stroops)
    ///
    /// # Returns
    /// `true` if deposit was successful
    ///
    /// # Errors
    /// * `SettlementTokenNotConfigured` - If `bind_settlement_token` has not been called
    /// * `AmountMustBePositive` - If amount is <= 0
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `InvalidState` - If contract is not in Created state
    /// * `UnauthorizedRole` - If caller is not the client
    pub fn deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool {
        Self::require_initialized(&env);
        Self::require_not_paused(&env);

        // Validate all contract-local preconditions before any SAC transfer so
        // rejected deposits cannot debit the client and then fail state checks.
        let validated = deposit::validate_deposit(&env, contract_id, &caller, amount);

        let token = Self::read_settlement_token(&env)
            .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured));

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&caller, &env.current_contract_address(), &amount);

        deposit::apply_validated_deposit(&env, contract_id, caller, validated)
    }

    /// Finalize an escrow contract by writing immutable close metadata.
    ///
    /// `finalizer` must authorize the call and must be the stored client,
    /// freelancer, or assigned arbiter. Finalization is allowed only while the
    /// contract is `Completed` or `Disputed`. Once finalized, future
    /// contract-specific mutations fail with `AlreadyFinalized`.
    ///
    /// # Errors
    /// - `ContractPaused` when pause or emergency controls are active.
    /// - `ContractNotFound` when `contract_id` is unknown.
    /// - `AlreadyFinalized` when a close record already exists.
    /// - `UnauthorizedRole` when `finalizer` is not a contract participant.
    /// - `InvalidStatusTransition` unless status is `Completed` or `Disputed`.
    pub fn finalize_contract(env: Env, contract_id: u32, finalizer: Address) -> bool {
        finalize::finalize_contract_impl(&env, contract_id, finalizer)
    }

    /// Return immutable close metadata for `contract_id`, if it has been finalized.
    pub fn get_finalization_record(
        env: Env,
        contract_id: u32,
    ) -> Option<finalize::FinalizationRecord> {
        finalize::get_finalization_record_impl(&env, contract_id)
    }

    /// Propose a client migration for an existing contract.
    ///
    /// Canonical public entrypoint; delegates to `propose_client_migration_impl`.
    /// The current client must authorize the call. The proposed client address
    /// must not be the freelancer or the current client. The pending migration
    /// is stored in temporary storage with TTL.
    pub fn propose_client_migration(
        env: Env,
        contract_id: u32,
        current_client: Address,
        new_client: Address,
    ) -> bool {
        Self::require_not_paused(&env);
        Self::propose_client_migration_impl(&env, contract_id, current_client, new_client)
    }

    /// Accept a live pending client migration and update the contract.
    ///
    /// Canonical public entrypoint; delegates to `accept_client_migration_impl`.
    /// Only the proposed client address may authorize acceptance.
    pub fn accept_client_migration(env: Env, contract_id: u32, new_client: Address) -> bool {
        Self::require_not_paused(&env);
        Self::accept_client_migration_impl(&env, contract_id, new_client)
    }

    /// Return true if a live pending client migration exists.
    ///
    /// Canonical public entrypoint; delegates to `has_pending_client_migration_impl`.
    pub fn has_pending_client_migration(env: Env, contract_id: u32) -> bool {
        Self::has_pending_client_migration_impl(&env, contract_id)
    }

    /// Return the live pending client migration record.
    ///
    /// Canonical public entrypoint; delegates to `get_pending_client_migration_impl`.
    /// Panics with `InvalidState` when no live pending migration exists.
    pub fn get_pending_client_migration(env: Env, contract_id: u32) -> PendingClientMigration {
        Self::get_pending_client_migration_impl(&env, contract_id)
    }

    /// Approves a milestone for release.
    ///
    /// Records the caller's approval in temporary storage with a TTL of
    /// `PENDING_APPROVAL_TTL_LEDGERS` (~7 days). Each call resets the TTL.
    /// Duplicate approvals from the same party are rejected.
    ///
    /// Required approvers per mode:
    /// - `ClientOnly` — client only
    /// - `ArbiterOnly` — arbiter only
    /// - `ClientAndArbiter` — client or arbiter (one is enough)
    /// - `MultiSig` — both client and freelancer must approve
    ///
    /// # Errors
    /// * `ContractPaused` - If the contract is paused while not in emergency mode
    /// * `EmergencyActive` - If the contract is in an active emergency pause
    /// * `AlreadyFinalized` - If the contract has already been finalized
    /// * Approval/auth/state errors bubbled up from `approvals::approve_milestone`
    ///
    /// # Security
    /// * Pause/emergency gate runs BEFORE finalization checks, auth, TTL extension,
    ///   and approval staging so no approval state mutates while the contract is frozen.
    ///
    /// See `docs/escrow/approvals-and-release.md` for the full flow.
    pub fn approve_milestone_release(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        Self::require_not_paused(&env);
        Self::require_not_finalized(&env, contract_id);
        approvals::approve_milestone(&env, contract_id, milestone_index, &caller)
            .unwrap_or_else(|e| env.panic_with_error(e))
    }

    /// Grants exactly one pending reputation credit to the freelancer.
    ///
    /// This is called exactly once when a contract successfully transitions to
    /// the `Completed` state, either through the final milestone release
    /// or via dispute resolution. Credits accumulate independently for each
    /// completed contract and are consumed one at a time by `issue_reputation`.
    /// A `Refunded` contract never calls this helper and therefore earns no credit.
    fn grant_pending_reputation_credit(env: &Env, freelancer: &Address) {
        let pending_key = DataKey::PendingReputationCredits(freelancer.clone());
        let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
        env.storage().persistent().set(&pending_key, &(pending + 1));
    }

    /// Releases a specific milestone, transferring the net payout to the freelancer.
    ///
    /// Executes `SAC::transfer(from: escrow_address, to: freelancer, milestone.amount − fee)`.
    /// The protocol fee is retained inside the contract under
    /// `DataKey::AccumulatedProtocolFees` and stays commingled with the escrow balance
    /// until `withdraw_protocol_fees` is called.
    ///
    /// See [`docs/escrow/sac-custody.md`](../../../docs/escrow/sac-custody.md) for the
    /// full custody model and accounting invariant.
    ///
    /// The target milestone must be fully funded through per-milestone deposit
    /// allocation before it can be released.
    ///
    /// Requires valid, non-expired approvals based on the contract's ReleaseAuthorization mode.
    ///
    /// MultiSig semantics are client-and-freelancer approval. A MultiSig
    /// milestone can be released only by the stored client or freelancer after
    /// both of those addresses have approved the same milestone.
    ///
    /// Approvals are cleared from temporary storage after a successful release.
    /// Missing or expired approvals are fail-closed — they produce
    /// `InsufficientApprovals` and the call panics without mutating state.
    ///
    /// See `approve_milestone_release`, `get_milestone_approvals`, and
    /// `docs/escrow/approvals-and-release.md` for the full flow.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `caller` - The address of the caller (must be authorized)
    /// * `milestone_index` - The index of the milestone to release
    ///
    /// # Returns
    /// `true` if release was successful
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `InvalidState` - If contract is not in Funded state
    /// * `InvalidMilestone` - If milestone index is out of bounds
    /// * `AlreadyReleased` - If milestone was already released
    /// * `AlreadyRefunded` - If milestone was already refunded
    /// * `InsufficientFunds` - If the milestone or aggregate contract balance is underfunded
    /// * `InsufficientApprovals` - If required approvals are missing
    /// * `ApprovalExpired` - If approvals have expired
    /// * `UnauthorizedRole` - If caller is not authorized to release
    ///
    /// # Security
    /// - Requires valid approvals that haven't expired
    /// - Approvals are cleared after successful release
    /// - Fail-closed: missing or expired approvals prevent release
    ///
    /// # Events
    /// Emits `("mlstn_rls", contract_id)` with payload
    /// `(milestone_index, amount, fee, new_released_amount, caller, timestamp)`
    /// on every successful release.
    ///
    /// Additionally emits `("ctrct_cmp", contract_id)` with payload
    /// `(caller, timestamp)` when the release transitions the contract to
    /// `Completed` (i.e. all milestones are released or refunded).
    pub fn release_milestone(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
    ) -> bool {
        Self::require_not_paused(&env);
        // Authenticate caller before any state-dependent logic
        caller.require_auth();

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Extend TTL on contract read
        ttl::extend_contract_ttl(&env, contract_id);

        Self::require_not_finalized(&env, contract_id);

        // Verify contract is in Funded state before release (deposit transitions
        // Created → Funded when fully funded, so release must accept Funded).
        if contract.status != ContractStatus::Funded {
            env.panic_with_error(Error::InvalidState);
        }

        // Check caller is authorized for this release authorization mode
        let is_client = caller == contract.client;
        let is_freelancer = caller == contract.freelancer;
        let is_arbiter = contract.arbiter.as_ref() == Some(&caller);

        match contract.release_authorization {
            ReleaseAuthorization::ClientOnly => {
                if !is_client {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ArbiterOnly => {
                if !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::ClientAndArbiter => {
                if !is_client && !is_arbiter {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
            ReleaseAuthorization::MultiSig => {
                if !is_client && !is_freelancer {
                    env.panic_with_error(EscrowError::UnauthorizedRole);
                }
            }
        }

        let mut milestones: Vec<Milestone> = ttl::load_milestones(&env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap().clone();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }

        if milestone.refunded {
            env.panic_with_error(EscrowError::AlreadyRefunded);
        }

        // Check for valid approvals
        approvals::check_approvals(&env, &contract, contract_id, milestone_index)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let milestone_key = Symbol::new(&env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap();

        // Extend TTL on milestone read
        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap().clone();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }

        if milestone.refunded {
            env.panic_with_error(Error::AlreadyRefunded);
        }

        // Check contract-level funding (per-milestone funded_amount is set after
        // release, so we check the aggregate contract balance here).
        let available =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available < milestone.amount {
            env.panic_with_error(Error::InsufficientFunds);
        }

        let gross_amount = milestone.amount;

        // Compute the protocol fee up-front so the available-balance check can
        // account for both the net payout and the fee that stays in the contract.
        //
        /// `protocol_fee` — the portion of `gross_amount` retained by the
        /// protocol. Deducted from the gross milestone amount before transfer
        /// so the escrow balance is never overdrawn.
        let protocol_fee: i128 = if Self::is_initialized(&env) {
            let fee_bps = Self::read_protocol_fee_bps(&env);
            if fee_bps > 0 {
                Self::calculate_protocol_fee(&env, gross_amount, fee_bps)
            } else {
                0
            }
        } else {
            0
        };

        /// `net_amount` — the amount actually transferred to the freelancer
        /// after deducting the protocol fee.
        let net_amount = gross_amount - protocol_fee;

        // The available balance must cover the full gross milestone amount
        // (net payout + fee) without dipping into already-accumulated fees or
        // other milestones' funds.
        let accumulated_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);
        let available_balance = contract.funded_amount
            - contract.released_amount
            - contract.refunded_amount
            - accumulated_fees;
        if available_balance < gross_amount {
            env.panic_with_error(EscrowError::InsufficientFunds);
        }

        // Transfer the net amount (gross minus fee) to the freelancer.
        // The fee portion remains in the contract's token balance and is
        // tracked separately in AccumulatedProtocolFees.
        let token = Self::read_settlement_token(&env)
            .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured));
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &contract.freelancer,
            &net_amount,
        );

        // Accrue the fee into the protocol's accumulated balance.
        if protocol_fee > 0 {
            env.storage().persistent().set(
                &DataKey::AccumulatedProtocolFees,
                &(accumulated_fees + protocol_fee),
            );
        }

        milestone.released = true;
        // Record the funded amount on the milestone so it is self-describing.
        milestone.funded_amount = gross_amount;
        milestones.set(milestone_index, milestone.clone());
        // released_amount tracks net amounts paid out to freelancers.
        // accumulated_fees tracks protocol fees retained in the contract.
        // Together: released_amount + refunded_amount + accumulated_fees <= funded_amount.
        contract.released_amount = contract
            .released_amount
            .checked_add(net_amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));

        // Accounting invariant: net released + refunded + all accumulated fees
        // must never exceed the total funded amount.
        let new_accumulated = accumulated_fees + protocol_fee;
        let invariant_sum = contract.released_amount + contract.refunded_amount + new_accumulated;
        if invariant_sum > contract.funded_amount {
            env.panic_with_error(EscrowError::AccountingInvariantViolated);
        }

        // Clear approvals after successful release
        approvals::clear_approvals(&env, contract_id, milestone_index);

        // Check if all milestones are released or refunded; if so, complete.
        let all_released = milestones.iter().all(|m| m.released || m.refunded);
        let old_release_status = contract.status;
        if all_released {
            contract.status = ContractStatus::Completed;
            Self::grant_pending_reputation_credit(&env, &contract.freelancer);
        }

        ttl::store_milestones(&env, contract_id, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        // Extend TTL on contract write (milestone TTL already extended by store_milestones)
        ttl::extend_contract_ttl(&env, contract_id);

        // ── Events ──────────────────────────────────────────────────────────
        //
        // Emitted only after all state mutations succeed (fail-closed guarantee:
        // if execution reaches here, the release was accepted). Events contain
        // no secrets — all fields are already public contract state or
        // caller-supplied arguments.

        /// `mlstn_rls` — fired on every successful milestone release.
        ///
        /// Topics : `(symbol_short!("mlstn_rls"), contract_id: u32)`
        /// Data   : `(milestone_index: u32, amount: i128, fee: i128,
        ///            new_released_amount: i128, caller: Address, timestamp: u64)`
        env.events().publish(
            (symbol_short!("mlstn_rls"), contract_id),
            (
                milestone_index,
                gross_amount,
                protocol_fee,
                contract.released_amount,
                caller.clone(),
                env.ledger().timestamp(),
            ),
        );

        // `ctrct_cmp` — fired only when this release completes the contract.
        //
        /// Topics : `(symbol_short!("ctrct_cmp"), contract_id: u32)`
        /// Data   : `(caller: Address, timestamp: u64)`
        if all_released {
            env.events().publish(
                (symbol_short!("ctrct_cmp"), contract_id),
                (caller.clone(), env.ledger().timestamp()),
            );

            env.events().publish(
                (symbol_short!("ctrct_st"), contract_id),
                (
                    old_release_status as u32,
                    ContractStatus::Completed as u32,
                    contract.funded_amount,
                    contract.released_amount,
                    contract.refunded_amount,
                    env.ledger().timestamp(),
                ),
            );
        }

        true
    }

    /// Checks if a specific milestone is overdue based on its deadline.
    ///
    /// A milestone is considered overdue if:
    /// - It has a deadline set (Some value)
    /// - The current time is strictly greater than the deadline (now > deadline)
    /// - The milestone has not been released
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_index` - The index of the milestone to check
    ///
    /// # Returns
    /// `true` if the milestone is overdue, `false` otherwise
    ///
    /// # Note
    /// - Returns `false` if milestone has no deadline (None)
    /// - Returns `false` if milestone is already released
    /// - Boundary condition: at exactly the deadline (now == deadline), returns `false`
    ///   because the deadline hasn't passed yet (uses strictly > comparison)
    ///
    /// # Security
    /// Uses `now_seconds(&env)` which is the single source of truth for ledger time.
    /// Time cannot be manipulated by contract callers.
    pub fn is_milestone_overdue(env: Env, contract_id: u32, milestone_index: u32) -> bool {
        let contract: Contract = match env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
        {
            Some(c) => c,
            None => return false, // Contract not found, not overdue
        };

        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = match env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
        {
            Some(m) => m,
            None => return false, // No milestones, not overdue
        };

        if milestone_index >= milestones.len() {
            return false; // Index out of bounds, not overdue
        }

        let milestone = milestones.get(milestone_index).unwrap();

        // Return false if already released
        if milestone.released {
            return false;
        }

        // Return false if no deadline set
        match milestone.deadline {
            None => false,
            Some(deadline) => {
                // Overdue if now > deadline (strictly greater)
                now_seconds(&env) > deadline
            }
        }
    }

    /// Refunds unreleased milestones back to the client.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_indices` - Vector of milestone indices to refund
    ///
    /// # Returns
    /// The total amount refunded
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `EmptyRefundRequest` - If milestone_indices is empty
    /// * `DuplicateMilestoneInRefund` - If the same milestone appears multiple times
    /// * `IndexOutOfBounds` - If any milestone index is out of bounds
    /// * `AlreadyReleased` - If any milestone was already released
    /// * `AlreadyRefunded` - If any milestone was already refunded
    /// * `InsufficientFunds` - If contract doesn't have enough balance to refund
    /// * `AlreadyFinalized` - If a finalization record already exists for this contract
    /// * `InvalidState` - If contract status is not Created, Funded, or Disputed
    pub fn refund_unreleased_milestones(
        env: Env,
        contract_id: u32,
        milestone_indices: Vec<u32>,
    ) -> i128 {
        Self::require_not_paused(&env);
        // Validate non-empty request
        if milestone_indices.is_empty() {
            env.panic_with_error(EscrowError::EmptyRefundRequest);
        }

        // Check for duplicates
        for i in 0..milestone_indices.len() {
            for j in (i + 1)..milestone_indices.len() {
                if milestone_indices.get(i).unwrap() == milestone_indices.get(j).unwrap() {
                    env.panic_with_error(EscrowError::DuplicateMilestoneInRefund);
                }
            }
        }

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Extend TTL on contract read
        ttl::extend_contract_ttl(&env, contract_id);

        Self::require_not_finalized(&env, contract_id);

        // Only allow refunds while the contract is still in an active,
        // unreleased state. Cancelled, Completed, and Refunded contracts
        // must not be refundable again.
        if contract.status != ContractStatus::Created
            && contract.status != ContractStatus::Funded
            && contract.status != ContractStatus::Disputed
        {
            env.panic_with_error(EscrowError::InvalidState);
        }

        contract.client.require_auth();

        let mut milestones: Vec<Milestone> = ttl::load_milestones(&env, contract_id);

        let mut total_refund_amount: i128 = 0;

        // Validate all milestones first
        for idx in milestone_indices.iter() {
            if idx >= milestones.len() {
                env.panic_with_error(Error::IndexOutOfBounds);
            }

            let milestone = milestones.get(idx).unwrap();

            // SECURITY: Check if milestone is already released
            if milestone.released {
                env.panic_with_error(Error::AlreadyReleased);
            }

            // SECURITY: Check if milestone is already refunded
            if milestone.refunded {
                env.panic_with_error(EscrowError::AlreadyRefunded);
            }

            // SECURITY: Check timeout refund conditions - milestone must be overdue if deadline is set
            if let Some(deadline) = milestone.deadline {
                // Milestone has a deadline - check if it's overdue
                if !Self::is_milestone_overdue(env.clone(), contract_id, idx) {
                    // Deadline set but milestone not yet overdue
                    env.panic_with_error(Error::MilestoneNotOverdue);
                }
                // SECURITY: is_milestone_overdue already verified: now > deadline AND unreleased
            }
            // If no deadline (None), allow refund anytime (backward compatibility)

            total_refund_amount += milestone.amount;
        }

        // Check if there's enough balance
        let available_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if available_balance < total_refund_amount {
            env.panic_with_error(EscrowError::InsufficientFunds);
        }

        // Transfer tokens from contract to client
        let token = Self::read_settlement_token(&env)
            .unwrap_or_else(|| env.panic_with_error(Error::SettlementTokenNotConfigured));

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &contract.client,
            &total_refund_amount,
        );

        // Mark milestones as refunded
        for idx in milestone_indices.iter() {
            let mut milestone = milestones.get(idx).unwrap();
            milestone.refunded = true;
            milestone.refunded_amount = milestone.amount;
            milestones.set(idx, milestone);
        }

        contract.refunded_amount = contract
            .refunded_amount
            .checked_add(total_refund_amount)
            .unwrap_or_else(|| env.panic_with_error(Error::InsufficientFunds));

        // Check if all unreleased milestones are refunded
        let all_refunded_or_released = milestones.iter().all(|m| m.released || m.refunded);
        let old_refund_status = contract.status;
        if all_refunded_or_released {
            let all_refunded = milestones.iter().all(|m| m.refunded);
            if all_refunded {
                contract.status = ContractStatus::Refunded;
            } else {
                // Some released, some refunded
                contract.status = ContractStatus::Completed;
                Self::grant_pending_reputation_credit(&env, &contract.freelancer);
            }
        }

        ttl::store_milestones(&env, contract_id, &milestones);
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        // Extend TTL on contract write (milestone TTL already extended by store_milestones)
        ttl::extend_contract_ttl(&env, contract_id);

        // Emit `refunded` event after all state mutations succeed.
        //
        // Topics : `(symbol_short!("refunded"), contract_id: u32)`
        // Data   : `(total_refund_amount: i128, new_status: ContractStatus, timestamp: u64)`
        env.events().publish(
            (symbol_short!("refunded"), contract_id),
            (
                total_refund_amount,
                contract.status,
                env.ledger().timestamp(),
            ),
        );

        env.events().publish(
            (symbol_short!("ctrct_st"), contract_id),
            (
                old_refund_status as u32,
                contract.status as u32,
                contract.funded_amount,
                contract.released_amount,
                contract.refunded_amount,
                env.ledger().timestamp(),
            ),
        );

        total_refund_amount
    }

    /// Checks whether a contract with the given ID exists in storage.
    ///
    /// This is a cheap, non-panicking existence probe that returns `true` if
    /// the contract record is present and `false` otherwise. Unlike `get_contract`,
    /// this function does **not** panic with `ContractNotFound` for missing IDs,
    /// making it safe for indexers and clients iterating over ID ranges.
    ///
    /// # Security
    /// This is a read-only operation that does **not** extend the contract's TTL.
    /// Probing for contract existence cannot be abused to keep entries alive.
    /// Only actual contract operations (reads/writes) extend TTL.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID to check
    ///
    /// # Returns
    /// * `true` if the contract exists
    /// * `false` if the contract does not exist
    ///
    /// # Examples
    /// ```
    /// // Safe iteration over a range of IDs
    /// for id in 1..=100 {
    ///     if escrow.contract_exists(id) {
    ///         let contract = escrow.get_contract(id);
    ///         // process contract
    ///     }
    /// }
    /// ```
    pub fn contract_exists(env: Env, contract_id: u32) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Contract(contract_id))
    }

    /// Retrieves contract information.
    pub fn get_contract(env: Env, contract_id: u32) -> Contract {
        let contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        // Extend TTL on contract read
        ttl::extend_contract_ttl(&env, contract_id);
        contract
    }

    /// Returns the next contract ID to be allocated (the high-water mark).
    ///
    /// This reader returns the current value of `NextContractId`, which represents
    /// the next ID that will be assigned when `create_contract` is called.
    /// Indexers can use this to determine the allocation high-water mark and
    /// safely iterate over the allocated ID range `[1, get_next_contract_id() - 1]`.
    ///
    /// # Security
    /// This is a read-only operation that does not mutate contract state or extend TTL.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// The next contract ID to be allocated (always ≥ 1)
    ///
    /// # Examples
    /// ```
    /// // Get the high-water mark
    /// let next_id = escrow.get_next_contract_id();
    /// // All allocated IDs are in the range [1, next_id - 1]
    /// for id in 1..next_id {
    ///     if escrow.contract_exists(id) {
    ///         let contract = escrow.get_contract(id);
    ///         // process contract
    ///     }
    /// }
    /// ```
    pub fn get_next_contract_id(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextContractId)
            .unwrap_or(1)
    }

    /// Returns a structured summary of the contract and its milestones.
    ///
    /// Extends contract and milestone TTL on read without requiring caller auth.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    ///
    /// # Returns
    /// The detailed `ContractSummary` for off-chain consumption
    ///
    /// # Errors
    /// * `ContractNotFound` - If contract doesn't exist
    pub fn get_contract_summary(env: Env, contract_id: u32) -> ContractSummary {
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        // Extend TTL on contract and milestones read
        ttl::extend_contract_and_milestones_ttl(&env, contract_id);

        let milestones = ttl::load_milestones(&env, contract_id);
        let total_amount: i128 =
            crate::amount_validation::accumulate_amounts(milestones.iter().map(|m| m.amount))
                .unwrap_or_else(|_| env.panic_with_error(EscrowError::PotentialOverflow));
        let released_milestone_count = milestones.iter().filter(|m| m.released).count() as u32;

        let mut milestone_summaries = Vec::new(&env);
        for (idx, m) in milestones.iter().enumerate() {
            milestone_summaries.push_back(MilestoneSummary {
                index: idx as u32,
                amount: m.amount,
                released: m.released,
                refunded: m.refunded,
            });
        }

        let reputation_issued = env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::ReputationIssued(contract_id))
            .unwrap_or(contract.reputation_issued);

        let refundable_balance =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;

        ContractSummary {
            schema_version: CONTRACT_SUMMARY_SCHEMA_VERSION,
            client: contract.client,
            freelancer: contract.freelancer,
            arbiter: contract.arbiter,
            status: contract.status,
            reputation_issued,
            total_amount,
            funded_amount: contract.funded_amount,
            released_amount: contract.released_amount,
            refundable_balance,
            released_milestone_count,
            milestones: milestone_summaries,
        }
    }

    /// Retrieves all milestones for a contract.
    pub fn get_milestones(env: Env, contract_id: u32) -> Vec<Milestone> {
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_milestone_ttl(&env, contract_id);
        milestones
    }

    /// Retrieves a single milestone by index for a contract.
    ///
    /// This is the bounds-checked single-item counterpart to
    /// `get_milestones`. Off-chain callers that only need one milestone's
    /// state (amount, funded/released/refunded flags, deadline, work evidence)
    /// can avoid fetching and decoding the full `Vec<Milestone>`.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `milestone_index` - The zero-based index of the milestone to read
    ///
    /// # Returns
    /// * `Some(Milestone)` if `milestone_index` is in bounds
    /// * `None` if `milestone_index` is out of bounds
    ///
    /// # Panics
    /// Panics with `ContractNotFound` if the contract's milestones were never
    /// allocated (i.e. the contract id is unknown), matching
    /// `get_milestones`.
    ///
    /// # Side effects
    /// Extends the milestones vector TTL on a successful read, consistent with
    /// `get_milestones`. Auth-free and otherwise non-mutating.
    pub fn get_milestone(env: Env, contract_id: u32, milestone_index: u32) -> Option<Milestone> {
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_milestone_ttl(&env, contract_id);
        milestones.get(milestone_index)
    }

    /// Returns funded minus released minus refunded for `contract_id`.
    pub fn get_refundable_balance(env: Env, contract_id: u32) -> i128 {
        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_contract_ttl(&env, contract_id);
        contract.funded_amount - contract.released_amount - contract.refunded_amount
    }

    /// Retrieves approval status for a milestone.
    ///
    /// Returns `None` when no approval record exists or when the TTL has
    /// elapsed. Treat `None` and an all-`false` struct identically — neither
    /// unblocks `release_milestone`.
    ///
    /// On a successful read, this entrypoint renews the temporary approval
    /// record's TTL using `PENDING_APPROVAL_BUMP_THRESHOLD` /
    /// `PENDING_APPROVAL_TTL_LEDGERS`, consistent with the approval write path.
    /// Missing or expired entries still return `None` without writing.
    ///
    /// # Cost Semantics
    /// This is a storage-touching read of temporary state, not a zero-cost pure
    /// getter. Integrators that poll approval state should account for the host
    /// storage access and TTL bump behavior.
    ///
    /// See `approve_milestone_release` and `docs/escrow/authorization.md`.
    pub fn get_milestone_approvals(
        env: Env,
        contract_id: u32,
        milestone_index: u32,
    ) -> Option<MilestoneApprovals> {
        let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
        let approvals = env.storage().temporary().get(&approval_key);
        if approvals.is_some() {
            env.storage().temporary().extend_ttl(
                &approval_key,
                ttl::PENDING_APPROVAL_BUMP_THRESHOLD,
                ttl::PENDING_APPROVAL_TTL_LEDGERS,
            );
        }
        approvals
    }

    /// Retrieves approval status for a milestone.
    ///
    /// Returns ledgers remaining, computed against ttl::compute_expiry.
    /// `None` when no live approval exists,
    /// distinguishing "never approved" from "approved and evicted".
    pub fn get_approval_deadline(env: Env, contract_id: u32, milestone_index: u32) -> Option<u32> {
        let approval_key = DataKey::MilestoneApprovals(contract_id, milestone_index);
        if !env.storage().temporary().has(&approval_key) {
            return None;
        }

        Some(ttl::compute_expiry(&env, ttl::PENDING_APPROVAL_TTL_LEDGERS))
    }

    // ── Pause / unpause ──────────────────────────────────────────────────────

    /// Pause all state-changing escrow operations.
    ///
    /// Requires the stored admin's authorization. While paused, all mutating
    /// entrypoints panic with `ContractPaused`. Read-only queries are never blocked.
    ///
    /// # Events
    /// Emits `("paused", timestamp)` with `(admin,)` payload.
    pub fn pause(env: Env) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &true);

        env.events()
            .publish((symbol_short!("pause"), env.ledger().timestamp()), (admin,));
        true
    }

    /// Unpause operations, clearing the `Paused` flag.
    ///
    /// Blocked while `Emergency` is active — use `resolve_emergency` instead.
    /// Requires the stored admin's authorization.
    ///
    /// # Events
    /// Emits `("unpaused", timestamp)` with `(admin,)` payload.
    pub fn unpause(env: Env) -> bool {
        Self::require_initialized(&env);
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Emergency)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::EmergencyActive);
        }
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &false);

        env.events().publish(
            (symbol_short!("unpaused"), env.ledger().timestamp()),
            (admin,),
        );
        true
    }

    /// Returns `true` if the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    // ── Emergency pause ──────────────────────────────────────────────────────

    /// Activate emergency pause, setting both `Emergency` and `Paused` flags.
    ///
    /// Requires the stored admin's authorization. While emergency is active,
    /// all mutating entrypoints panic with `EmergencyActive` or `ContractPaused`,
    /// and `unpause` is blocked.
    ///
    /// # Events
    /// Emits `("emergency", "activated")` with `(admin, timestamp)` payload.
    /// Sets `emergency_controls_enabled` in the readiness checklist.
    pub fn activate_emergency_pause(env: Env) -> bool {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));

        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            admin.require_auth();
        }
        env.storage().persistent().set(&DataKey::Emergency, &true);
        env.storage().persistent().set(&DataKey::Paused, &true);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default();
        checklist.emergency_controls_enabled = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);

        env.events().publish(
            (
                Symbol::new(&env, "emergency"),
                Symbol::new(&env, "activated"),
            ),
            (
                env.storage()
                    .persistent()
                    .get::<_, Address>(&DataKey::Admin)
                    .unwrap(),
                env.ledger().timestamp(),
            ),
        );
        true
    }

    /// Resolve emergency, clearing both `Emergency` and `Paused` flags.
    ///
    /// Requires the stored admin's authorization. After resolution, all
    /// operations resume normally.
    ///
    /// # Events
    /// Emits `("emergency", "resolved")` with `(admin, timestamp)` payload.
    /// Sets `emergency_controls_enabled` in the readiness checklist.
    pub fn resolve_emergency(env: Env) -> bool {
        Self::require_initialized(&env);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Emergency, &false);
        env.storage().persistent().set(&DataKey::Paused, &false);

        let mut checklist: ReadinessChecklist = env
            .storage()
            .persistent()
            .get(&DataKey::ReadinessChecklist)
            .unwrap_or_default();
        checklist.emergency_controls_enabled = true;
        env.storage()
            .persistent()
            .set(&DataKey::ReadinessChecklist, &checklist);
        env.events().publish(
            (
                Symbol::new(&env, "emergency"),
                Symbol::new(&env, "resolved"),
            ),
            (admin, env.ledger().timestamp()),
        );
        true
    }

    pub fn is_emergency(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Emergency)
            .unwrap_or(false)
    }

    // ── Cancel contract ──────────────────────────────────────────────────────

    /// Cancels a contract before any milestone has been released.
    ///
    /// The caller must be the stored client and must authorize the call. The
    /// contract must be in `Created` or `Funded` state, with no released
    /// balance, and the full remaining refundable balance is sent back to the
    /// client via the configured Stellar Asset Contract before the contract is
    /// marked `Cancelled`. A zero-funded cancellation does not invoke a token
    /// transfer and leaves unrelated contracts' escrowed token balances intact.
    ///
    /// # Errors
    /// * `ContractPaused` - If the contract is paused while not in emergency mode.
    /// * `EmergencyActive` - If the contract is in an active emergency pause.
    /// * `ContractNotFound` - If the contract does not exist.
    /// * `UnauthorizedRole` - If the caller is not the stored client.
    /// * `AlreadyCancelled` - If the contract was already cancelled.
    /// * `InvalidStatusTransition` - If the contract is not `Created`/`Funded` or has already released funds.
    pub fn cancel_contract(env: Env, contract_id: u32, client: Address) -> bool {
        Self::require_not_paused(&env);
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));
        ttl::extend_contract_ttl(&env, contract_id);

        Self::require_not_finalized(&env, contract_id);

        if client != contract.client {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        if contract.status == ContractStatus::Cancelled {
            env.panic_with_error(Error::AlreadyCancelled);
        }

        if contract.status != ContractStatus::Created && contract.status != ContractStatus::Funded {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        if contract.released_amount != 0 {
            env.panic_with_error(EscrowError::InvalidStatusTransition);
        }

        client.require_auth();

        let old_status = contract.status;
        let refund_amount =
            contract.funded_amount - contract.released_amount - contract.refunded_amount;
        if refund_amount > 0 {
            let token = Self::read_settlement_token(&env)
                .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
            token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &client,
                &refund_amount,
            );
        }

        contract.refunded_amount = contract
            .refunded_amount
            .checked_add(refund_amount)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::InsufficientFunds));
        contract.status = ContractStatus::Cancelled;

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);
        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("cancelled"), contract_id),
            (client, refund_amount, env.ledger().timestamp()),
        );

        env.events().publish(
            (symbol_short!("ctrct_st"), contract_id),
            (
                old_status as u32,
                ContractStatus::Cancelled as u32,
                contract.funded_amount,
                contract.released_amount,
                contract.refunded_amount,
                env.ledger().timestamp(),
            ),
        );

        true
    }

    // ── Dispute management ────────────────────────────────────────────────────

    // ── Reputation ───────────────────────────────────────────────────────────

    /// Issues reputation credit for a completed contract.
    ///
    /// # Comment length
    /// `comment` must be between 1 and 200 **bytes** (inclusive). Because Soroban
    /// `String::len()` returns the UTF-8 byte length, a multi-byte character (e.g.
    /// a 3-byte emoji) counts as 3 toward the limit. ASCII characters are 1 byte each.
    ///
    /// # Errors
    /// * `ContractPaused` - If the contract is paused while not in emergency mode
    /// * `EmergencyActive` - If the contract is in an active emergency pause
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `UnauthorizedRole` - If caller is not the stored client
    /// * `FreelancerMismatch` - If `freelancer` does not match the stored freelancer
    /// * `InvalidRating` - If rating is not in [1, 5]
    /// * `EmptyComment` - If comment is 0 bytes
    /// * `CommentTooLong` - If comment exceeds 200 bytes
    /// * `NotCompleted` - If contract status is not `Completed`
    /// * `ReputationAlreadyIssued` - If reputation was already issued
    /// * `SelfRating` - If client and freelancer are the same address
    ///
    /// # Security
    /// * Pause/emergency gate runs BEFORE contract state read so paused
    ///   contracts cannot have reputation mutated while paused.
    /// * The 200-byte cap prevents unbounded on-chain storage growth.
    pub fn issue_reputation(
        env: Env,
        contract_id: u32,
        caller: Address,
        rating: u32,
        comment: String,
    ) -> bool {
        Self::require_not_paused(&env);
        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));
        ttl::extend_contract_ttl(&env, contract_id);

        if caller != contract.client {
            env.panic_with_error(Error::UnauthorizedRole);
        }

        if rating < 1 || rating > 5 {
            env.panic_with_error(Error::InvalidRating);
        }

        if comment.len() == 0 {
            env.panic_with_error(Error::EmptyComment);
        }

        if comment.len() > 200 {
            env.panic_with_error(Error::CommentTooLong);
        }

        if contract.status != ContractStatus::Completed {
            env.panic_with_error(Error::NotCompleted);
        }

        if contract.reputation_issued {
            env.panic_with_error(Error::ReputationAlreadyIssued);
        }
        if contract.client == contract.freelancer {
            env.panic_with_error(Error::SelfRating);
        }

        caller.require_auth();
        contract.reputation_issued = true;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);
        env.storage()
            .persistent()
            .set(&DataKey::ReputationIssued(contract_id), &true);
        env.storage().persistent().extend_ttl(
            &DataKey::ReputationIssued(contract_id),
            ttl::PERSISTENT_BUMP_THRESHOLD,
            ttl::PERSISTENT_TTL_LEDGERS,
        );

        let pending_key = DataKey::PendingReputationCredits(contract.freelancer.clone());
        let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
        if pending <= 0 {
            env.panic_with_error(Error::InvalidState);
        }
        env.storage().persistent().set(&pending_key, &(pending - 1));

        let rep_key = DataKey::Reputation(contract.freelancer.clone());
        let mut rep: types::Reputation =
            env.storage().persistent().get(&rep_key).unwrap_or_default();
        rep.completed_contracts += 1;
        rep.total_rating += rating as i128;
        rep.last_rating = rating as i128;
        env.storage().persistent().set(&rep_key, &rep);

        let comment_key = DataKey::ReputationComment(contract_id);
        env.storage().persistent().set(&comment_key, &comment);
        env.storage().persistent().extend_ttl(
            &comment_key,
            ttl::PERSISTENT_BUMP_THRESHOLD,
            ttl::PERSISTENT_TTL_LEDGERS,
        );

        true
    }

    /// Returns the written feedback provided by the client when reputation was issued.
    /// Returns `None` if reputation has not been issued for this contract.
    pub fn get_reputation_comment(env: Env, contract_id: u32) -> Option<String> {
        let comment_key = DataKey::ReputationComment(contract_id);
        let comment: Option<String> = env.storage().persistent().get(&comment_key);
        if comment.is_some() {
            env.storage().persistent().extend_ttl(
                &comment_key,
                ttl::PERSISTENT_BUMP_THRESHOLD,
                ttl::PERSISTENT_TTL_LEDGERS,
            );
        }
        comment
    }

    pub fn get_reputation(env: Env, address: Address) -> Option<types::Reputation> {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(address))
    }

    /// Returns the freelancer's average rating scaled to basis points (×10 000),
    /// or `None` if no reputation record exists or no contracts have been completed.
    ///
    /// # Scaling
    /// `result = total_rating * 10_000 / completed_contracts`
    ///
    /// A raw rating of 5 on a single contract returns `50_000` (5.0000 on a
    /// 1–5 scale).  Clients divide by `10_000` to recover the decimal value.
    ///
    /// Checked arithmetic is used throughout; division by zero is impossible
    /// because `None` is returned whenever `completed_contracts == 0`.
    pub fn get_average_rating(env: Env, address: Address) -> Option<i128> {
        /// Basis-point scaling factor (×10 000 preserves four decimal places).
        const SCALE: i128 = 10_000;

        let rep: types::Reputation = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(address))?;

        if rep.completed_contracts == 0 {
            return None;
        }

        rep.total_rating
            .checked_mul(SCALE)
            .and_then(|scaled| scaled.checked_div(rep.completed_contracts))
    }

    /// Returns the number of completed contracts awaiting a reputation rating.
    ///
    /// This value increments once per completed contract and decrements once
    /// per successful `issue_reputation` call. Refunded contracts do not accrue
    /// pending reputation credits.
    pub fn get_pending_reputation_credits(env: Env, address: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::PendingReputationCredits(address))
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Work evidence
    // -----------------------------------------------------------------------

    /// Records a deliverable reference (e.g. IPFS CID or URL hash) for an
    /// unreleased milestone.
    ///
    /// Only the contract's freelancer may call this. The contract must be in
    /// `Funded` status and the target milestone must not yet be released or
    /// refunded. Evidence may be overwritten before release.
    ///
    /// # Arguments
    /// * `contract_id` - The escrow contract to update
    /// * `caller`      - Must equal the stored `freelancer`; requires auth
    /// * `milestone_index` - Zero-based index of the milestone
    /// * `evidence`    - Deliverable reference; max 256 bytes
    ///
    /// # Errors
    /// * `NotInitialized`     — `initialize` has not been called
    /// * `ContractPaused` / `EmergencyActive` — pause/emergency gate
    /// * `ContractNotFound`   — unknown `contract_id`
    /// * `AlreadyFinalized`   — contract has been finalized
    /// * `UnauthorizedRole`   — `caller` is not the freelancer
    /// * `InvalidState`       — contract is not `Funded`
    /// * `IndexOutOfBounds`   — `milestone_index` exceeds milestone count
    /// * `MilestoneAlreadyReleased` — milestone is already released
    /// * `AlreadyRefunded`    — milestone has been refunded
    /// * `EvidenceTooLong`    — evidence string exceeds 256 bytes
    pub fn submit_work_evidence(
        env: Env,
        contract_id: u32,
        caller: Address,
        milestone_index: u32,
        evidence: String,
    ) -> bool {
        /// Gate: contract must have been initialized so pause and emergency rails
        /// are always in scope before any state mutation can occur.
        Self::require_initialized(&env);
        Self::require_not_paused(&env);
        caller.require_auth();

        let contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);

        if caller != contract.freelancer {
            env.panic_with_error(EscrowError::UnauthorizedRole);
        }

        if contract.status != ContractStatus::Funded {
            env.panic_with_error(EscrowError::InvalidState);
        }

        // Bound evidence to 256 bytes to prevent storage bloat.
        if evidence.len() > 256 {
            env.panic_with_error(Error::EvidenceTooLong);
        }

        let milestone_key = Symbol::new(&env, "milestones");
        let mut milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key.clone()))
            .unwrap_or_else(|| env.panic_with_error(EscrowError::ContractNotFound));

        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            env.panic_with_error(Error::IndexOutOfBounds);
        }

        let mut milestone = milestones.get(milestone_index).unwrap();

        if milestone.released {
            env.panic_with_error(Error::MilestoneAlreadyReleased);
        }
        if milestone.refunded {
            env.panic_with_error(EscrowError::AlreadyRefunded);
        }

        milestone.work_evidence = Some(evidence.clone());
        milestones.set(milestone_index, milestone);

        ttl::store_milestones(&env, contract_id, &milestones);

        // Extend TTL on contract write (milestone TTL already extended by store_milestones)
        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("evidence"), contract_id),
            (
                milestone_index,
                contract.freelancer,
                env.ledger().timestamp(),
            ),
        );

        true
    }

    /// Returns the work evidence for a single milestone, or `None` if the
    /// milestone index is out of bounds or no evidence was submitted.
    ///
    /// # Arguments
    /// * `contract_id` - The escrow contract ID
    /// * `milestone_index` - Zero-based index of the milestone
    ///
    /// # Returns
    /// `Some(String)` with the evidence reference if it exists,
    /// `None` when the index is out of bounds or the milestone has no evidence.
    ///
    /// # Panics
    /// Panics with `ContractNotFound` if `contract_id` was never allocated.
    ///
    /// # TTL
    /// Extends the milestones vector's persistent TTL on read,
    /// consistent with `get_milestones`.
    pub fn get_work_evidence(env: Env, contract_id: u32, milestone_index: u32) -> Option<String> {
        let milestone_key = Symbol::new(&env, "milestones");
        let milestones: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&(DataKey::Contract(contract_id), milestone_key))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_milestone_ttl(&env, contract_id);

        if milestone_index >= milestones.len() {
            return None;
        }

        milestones.get(milestone_index).unwrap().work_evidence
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    // ── Finalization ─────────────────────────────────────────────────────────

    // ── Governance ───────────────────────────────────────────────────────────

    /// Returns the total accumulated protocol fees in stroops.
    ///
    /// The balance defaults to `0` when no fees have accrued. This public
    /// reader requires no authorization and does not mutate contract state.
    ///
    /// # Returns
    /// The fees currently available for protocol withdrawal.
    ///
    /// See [`docs/escrow/protocol-fees.md`](../../../docs/escrow/protocol-fees.md) for
    /// storage details and the full withdrawal flow.
    pub fn get_accumulated_protocol_fees(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get::<_, i128>(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0)
    }

    /// Drains accrued protocol fees from the escrow contract to a treasury address.
    ///
    /// Executes `SAC::transfer(from: escrow_address, to: treasury, amount)`.  Protocol
    /// fees accumulate in `DataKey::AccumulatedProtocolFees` as each milestone is
    /// released; they remain commingled with the escrow's SAC balance until this
    /// entrypoint is called.
    ///
    /// See [`docs/escrow/sac-custody.md`](../../../docs/escrow/sac-custody.md) for the
    /// full custody model, accounting invariant, and security notes on commingled fees.
    ///
    /// See [`docs/escrow/protocol-fees.md`](../../../docs/escrow/protocol-fees.md) for
    /// the complete fee lifecycle — basis-point model, accrual, withdrawal authorization,
    /// worked examples, and the release-to-withdrawal sequence diagram.
    ///
    /// Requires the stored admin's authorization. Only an amount up to the
    /// currently accumulated fees can be withdrawn.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `amount` - The amount of fees to withdraw
    /// * `to` - The destination address for the withdrawn fees
    pub fn withdraw_protocol_fees(env: Env, amount: i128, to: Address) -> bool {
        Self::require_initialized(&env);

        // Block withdrawal while paused or in emergency — consistent with all
        // other mutating entrypoints in this contract.
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            env.panic_with_error(EscrowError::ContractPaused);
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));

        admin.require_auth();

        if amount <= 0 {
            env.panic_with_error(EscrowError::AmountMustBePositive);
        }

        let accumulated: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedProtocolFees)
            .unwrap_or(0);

        if amount > accumulated {
            env.panic_with_error(EscrowError::InsufficientAccumulatedFees);
        }

        let token = match Self::read_settlement_token(&env) {
            Some(t) => t,
            None => env.panic_with_error(Error::SettlementTokenNotConfigured),
        };

        let new_accumulated = accumulated - amount;
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedProtocolFees, &new_accumulated);

        env.storage().persistent().extend_ttl(
            &DataKey::AccumulatedProtocolFees,
            ttl::PERSISTENT_BUMP_THRESHOLD,
            ttl::PERSISTENT_TTL_LEDGERS,
        );

        let token_client = soroban_sdk::token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &to, &amount);

        env.events().publish(
            (symbol_short!("fee"), symbol_short!("withdraw")),
            (admin, to, amount, env.ledger().timestamp()),
        );

        true
    }

    /// Returns the ledger sequence at which the pending admin proposal was made.
    ///
    /// Returns `None` if there is no pending proposal. This allows off-chain
    /// indexers and governance dashboards to compute the remaining timelock
    /// before the proposal can be accepted via `accept_governance_admin`.
    pub fn get_pending_admin_proposed_at(env: Env) -> Option<u32> {
        let proposal: Option<PendingAdminProposal> =
            env.storage().persistent().get(&DataKey::PendingAdmin);
        proposal.map(|p| p.proposed_at_ledger)
    }

    // ── Protocol fee helpers ─────────────────────────────────────────────────

    /// Reads the stored protocol fee in basis points (0 = no fee).
    ///
    /// See [`docs/escrow/protocol-fees.md`](../../../docs/escrow/protocol-fees.md) for
    /// the full basis-point model, formula, and fee lifecycle.
    pub(crate) fn read_protocol_fee_bps(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0)
    }

    /// Computes the protocol fee for a given `amount` at `fee_bps` basis points.
    ///
    /// Uses integer **floor division**: `fee = amount * fee_bps / 10_000`.
    /// The result always rounds down — it never rounds up — so the freelancer
    /// receives at least `amount - fee` stroops and the protocol receives at most
    /// the floored value.  Callers must ensure `fee <= amount` holds; this is
    /// guaranteed for any `fee_bps` in `[0, 10_000]` and a non-negative `amount`.
    ///
    /// # Basis-point unit
    /// `10_000 bps = 100%`. The maximum configurable rate is `10_000`. A rate of
    /// `0` is the default and disables fee collection entirely.
    ///
    /// See [`docs/escrow/protocol-fees.md`](../../../docs/escrow/protocol-fees.md) for
    /// the full formula, rounding rules, worked numeric examples, and the sequence
    /// diagram from release through treasury withdrawal.
    ///
    /// # Short-circuit
    /// Returns `0` immediately when `fee_bps == 0`, skipping the multiplication.
    ///
    /// # Panics
    /// Panics with `PotentialOverflow` (error code 28) if `amount * fee_bps`
    /// overflows `i128`.  Callers should keep `amount` well below `i128::MAX /
    /// fee_bps` to avoid this guard.
    pub fn calculate_protocol_fee(env: &Env, amount: i128, fee_bps: u32) -> i128 {
        if fee_bps == 0 {
            return 0;
        }
        let product = amount
            .checked_mul(fee_bps as i128)
            .unwrap_or_else(|| env.panic_with_error(Error::PotentialOverflow));
        product / 10_000
    }

    // ── Internal guards ──────────────────────────────────────────────────────

    /// Panics with `NotInitialized` unless `initialize` has been called.
    pub(crate) fn require_initialized(env: &Env) {
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            env.panic_with_error(Error::NotInitialized);
        }
    }

    fn is_initialized(env: &Env) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Dispute management
    // -----------------------------------------------------------------------

    /// Opens a dispute for a funded or partially funded escrow contract.
    ///
    /// This entrypoint transitions the contract status to `Disputed`, preventing
    /// further milestone releases until an assigned arbiter resolves the dispute.
    /// Only the client or freelancer can open a dispute, and an arbiter must be
    /// assigned to the contract.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `caller` - The address opening the dispute (must be client or freelancer)
    ///
    /// # Returns
    /// `true` if the dispute was successfully opened
    ///
    /// # Errors
    /// * `NotInitialized` - If `initialize` has not been called
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `UnauthorizedRole` - If caller is not client or freelancer
    /// * `ArbiterRequired` - If no arbiter is assigned to the contract
    /// * `InvalidState` - If contract is not in a disputable state
    /// * `ContractPaused` - If pause or emergency controls are active
    /// * `AlreadyFinalized` - If contract has been finalized
    ///
    /// # Security
    /// - Only contract parties (client/freelancer) can open disputes
    /// - Requires arbiter assignment for resolution
    /// - Blocks milestone releases while disputed
    /// - Respects pause and emergency controls
    pub fn raise_dispute(env: Env, contract_id: u32, caller: Address) -> bool {
        /// Gate: contract must have been initialized so pause and emergency rails
        /// are always in scope before any state mutation can occur.
        Self::require_initialized(&env);
        Self::require_not_paused(&env);
        caller.require_auth();

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);

        // Verify caller is client or freelancer
        if caller != contract.client && caller != contract.freelancer {
            env.panic_with_error(Error::UnauthorizedRole);
        }

        // Require arbiter assignment
        if contract.arbiter.is_none() {
            env.panic_with_error(Error::ArbiterRequired);
        }

        // Verify contract is in a disputable state (Funded or PartiallyFunded)
        match contract.status {
            ContractStatus::Funded | ContractStatus::PartiallyFunded => {}
            _ => env.panic_with_error(Error::InvalidState),
        }

        let old_status = contract.status;
        contract.status = ContractStatus::Disputed;
        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("dispute"), symbol_short!("opened")),
            (contract_id, caller.clone()),
        );

        env.events().publish(
            (symbol_short!("ctrct_st"), contract_id),
            (
                old_status as u32,
                ContractStatus::Disputed as u32,
                contract.funded_amount,
                contract.released_amount,
                contract.refunded_amount,
                env.ledger().timestamp(),
            ),
        );

        true
    }

    /// Resolves an open dispute by applying the arbiter-selected resolution.
    ///
    /// This entrypoint applies the dispute resolution (FullRefund, PartialRefund,
    /// FullPayout, or custom Split) to the remaining escrowed balance. The resolution
    /// must be authorized by the assigned arbiter and must conserve the available funds.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `contract_id` - The contract ID
    /// * `arbiter` - The arbiter address (must match contract's assigned arbiter)
    /// * `resolution` - The resolution decision (FullRefund, PartialRefund, FullPayout, or Split)
    ///
    /// # Returns
    /// `true` if the dispute was successfully resolved
    ///
    /// # Errors
    /// * `NotInitialized` - If `initialize` has not been called
    /// * `ContractNotFound` - If contract doesn't exist
    /// * `UnauthorizedRole` - If caller is not the assigned arbiter
    /// * `InvalidStatusTransition` - If contract is not in Disputed state
    /// * `InvalidDisputeSplit` - If custom split doesn't match available balance
    /// * `AccountingInvariantViolated` - If accounting state is inconsistent
    /// * `PotentialOverflow` - If amount calculations would overflow
    /// * `ContractPaused` - If pause or emergency controls are active
    /// * `AlreadyFinalized` - If contract has been finalized
    ///
    /// # Security
    /// - Only the assigned arbiter can resolve disputes
    /// - Split amounts must exactly match available balance
    /// - Updates released_amount and refunded_amount atomically
    /// - Emits dispute resolution event for indexers
    /// - Sets final contract status based on resolution outcome
    pub fn resolve_dispute(
        env: Env,
        contract_id: u32,
        arbiter: Address,
        resolution: DisputeResolution,
    ) -> bool {
        /// Gate: contract must have been initialized so pause and emergency rails
        /// are always in scope before any state mutation can occur.
        Self::require_initialized(&env);
        Self::require_not_paused(&env);
        arbiter.require_auth();

        let mut contract: Contract = env
            .storage()
            .persistent()
            .get(&DataKey::Contract(contract_id))
            .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

        ttl::extend_contract_ttl(&env, contract_id);
        Self::require_not_finalized(&env, contract_id);

        // Verify contract is in Disputed state
        if contract.status != ContractStatus::Disputed {
            env.panic_with_error(Error::InvalidStatusTransition);
        }

        // Verify caller is the assigned arbiter
        match &contract.arbiter {
            Some(contract_arbiter) if *contract_arbiter == arbiter => {}
            _ => env.panic_with_error(Error::UnauthorizedRole),
        }

        // Compute payouts based on resolution
        let (client_payout, freelancer_payout) =
            dispute::resolution_payouts(&contract, &resolution)
                .unwrap_or_else(|e| env.panic_with_error(e));

        // Update contract accounting
        contract.refunded_amount += client_payout;
        contract.released_amount += freelancer_payout;

        // Set final status
        let final_status = dispute::final_status_after_resolution(&contract);
        let old_status = contract.status;
        contract.status = final_status;
        if contract.status == ContractStatus::Completed {
            Self::grant_pending_reputation_credit(&env, &contract.freelancer);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Contract(contract_id), &contract);

        ttl::extend_contract_ttl(&env, contract_id);

        env.events().publish(
            (symbol_short!("dispute"), symbol_short!("resolved")),
            (contract_id, resolution.code()),
        );

        env.events().publish(
            (symbol_short!("ctrct_st"), contract_id),
            (
                old_status as u32,
                contract.status as u32,
                contract.funded_amount,
                contract.released_amount,
                contract.refunded_amount,
                env.ledger().timestamp(),
            ),
        );

        true
    }
}

/// Test fixtures and suites are compiled only for native test builds, never wasm.
#[cfg(test)]
mod test;