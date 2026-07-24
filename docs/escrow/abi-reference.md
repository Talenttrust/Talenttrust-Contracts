# Escrow Contract ABI Reference

This page is the single reference for the escrow contract's currently implemented public entrypoints. It is derived from the live Rust contract surface in [contracts/escrow/src/lib.rs](../../contracts/escrow/src/lib.rs), [contracts/escrow/src/governance.rs](../../contracts/escrow/src/governance.rs), [contracts/escrow/src/finalize.rs](../../contracts/escrow/src/finalize.rs), and [contracts/escrow/src/migration.rs](../../contracts/escrow/src/migration.rs).

The list intentionally omits planned or reserved entrypoints that are not implemented in the current code.

## Legend

- Mutating: changes persistent state or emits events.
- Read-only: does not mutate contract state.
- Auth: indicates whether the caller must authorize the call.
- Errors: the primary contract errors that can be raised by the current implementation.

## Public entrypoints

### initialize

- Signature: `initialize(env: Env, admin: Address) -> bool`
- Kind: Mutating
- Auth: `admin.require_auth()`
- Semantics: One-time initialization of the contract admin and readiness checklist. Stores next contract ID and the operational admin.
- Events: `("init", "admin_set")`
- Errors: `AlreadyInitialized`, `NotInitialized` (for helpers that depend on initialization)

### get_admin

- Signature: `get_admin(env: Env) -> Option<Address>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the stored admin address, if any.
- Events: None
- Errors: None

### get_bounds

- Signature: `get_bounds(env: Env) -> ContractBounds`
- Kind: Read-only
- Auth: None
- Semantics: Returns the protocol-wide hard-coded bounds (max milestones, max single milestone amount, max total escrow amount, max fee bps) enforced by `create_contract` and other validation paths. These are compile-time constants — the return value never changes between calls on the same contract binary. Off-chain indexers and client applications should query this endpoint to discover the current limits without relying on hard-coded constants. The returned `ContractBounds` struct contains only limit fields — no per-contract participant or accounting data — so its schema version tracks the limits API only and is decoupled from the `ContractSummary` schema.
- Events: None
- Errors: None

### get_mainnet_readiness_info

- Signature: `get_mainnet_readiness_info(env: Env) -> ReadinessChecklist`
- Kind: Read-only
- Auth: None
- Semantics: Returns the persisted readiness checklist, defaulting to all flags false.
- Events: None
- Errors: None

### create_contract

- Signature: `create_contract(env: Env, client: Address, freelancer: Address, arbiter: Option<Address>, milestones: Vec<i128>, release_authorization: ReleaseAuthorization) -> u32`
- Kind: Mutating
- Auth: `client.require_auth()`
- Semantics: Allocates a new escrow contract, validates participants and milestone inputs, and stores the initial contract record.
- Events: `("created", contract_id)`
- Errors: `InvalidParticipant` (or the canonical `InvalidParticipants` path in the underlying create logic), `EmptyMilestones`, `InvalidMilestoneAmount`, `InvalidDepositAmount` (in the current lifecycle flow), `InvalidState`, `InvalidArbiter`, `MissingArbiter`, `ContractIdCollision`, `ContractIdOverflow`, `NotInitialized` (when initialization is required by the surrounding security gate)

### deposit_funds

- Signature: `deposit_funds(env: Env, contract_id: u32, caller: Address, amount: i128) -> bool`
- Kind: Mutating
- Auth: `caller.require_auth()`
- Semantics: Funds an existing escrow contract. The current implementation enforces positive amounts, caller identity, and the contract state machine before updating accounting.
- Events: `("deposit", contract_id)` or equivalent lifecycle event emitted by the deposit module
- Errors: `ContractNotFound`, `AmountMustBePositive`, `InvalidState`, `UnauthorizedRole`, `NotInitialized`, `InvalidDepositAmount`, `InsufficientFunds`

### finalize_contract

- Signature: `finalize_contract(env: Env, contract_id: u32, finalizer: Address) -> bool`
- Kind: Mutating
- Auth: `finalizer.require_auth()`
- Semantics: Writes an immutable finalization record for a contract already in `Completed` or `Disputed` state. Prevents later contract-specific mutations.
- Events: `("finalized", contract_id)`
- Errors: `ContractPaused`, `EmergencyActive`, `ContractNotFound`, `AlreadyFinalized`, `UnauthorizedRole`, `InvalidStatusTransition`

### get_finalization_record

- Signature: `get_finalization_record(env: Env, contract_id: u32) -> Option<FinalizationRecord>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the finalization record for a contract, if one exists.
- Events: None
- Errors: None

### propose_client_migration

- Signature: `propose_client_migration(env: Env, contract_id: u32, current_client: Address, new_client: Address) -> bool`
- Kind: Mutating
- Auth: `current_client.require_auth()`
- Semantics: Creates a temporary pending migration proposal for the contract client. The proposal is stored with a TTL and may be accepted later.
- Events: `("client_migration_proposed", contract_id)`
- Errors: `ContractPaused`, `EmergencyActive`, `ContractNotFound`, `AlreadyFinalized`, `UnauthorizedRole`, `InvalidParticipant`, `InvalidState`, `InvalidStatusTransition`

### accept_client_migration

- Signature: `accept_client_migration(env: Env, contract_id: u32, new_client: Address) -> bool`
- Kind: Mutating
- Auth: `new_client.require_auth()`
- Semantics: Accepts a pending client migration, replacing the stored client with the proposed client.
- Events: `("client_migration_accepted", contract_id)`
- Errors: `ContractPaused`, `EmergencyActive`, `ContractNotFound`, `AlreadyFinalized`, `UnauthorizedRole`, `InvalidState`, `InvalidStatusTransition`

### has_pending_client_migration

- Signature: `has_pending_client_migration(env: Env, contract_id: u32) -> bool`
- Kind: Read-only
- Auth: None
- Semantics: Returns whether a live pending client migration exists for the contract.
- Events: None
- Errors: None

### get_pending_client_migration

- Signature: `get_pending_client_migration(env: Env, contract_id: u32) -> PendingClientMigration`
- Kind: Read-only
- Auth: None
- Semantics: Returns the pending migration payload for a contract, if present.
- Events: None
- Errors: `InvalidState` when no live pending proposal exists

### approve_milestone_release

- Signature: `approve_milestone_release(env: Env, contract_id: u32, caller: Address, milestone_index: u32) -> bool`
- Kind: Mutating
- Auth: `caller.require_auth()`
- Semantics: Stores a milestone approval record for the caller. The approval is temporary and expires according to the configured TTL.
- Events: `("ms_appr", contract_id)` with payload `(milestone_index, milestone_amount)`
- Errors: `ContractNotFound`, `AlreadyFinalized`, `InvalidState`, `IndexOutOfBounds`, `AlreadyApproved`, `InsufficientApprovals`, `ApprovalExpired` (if the implementation surface uses it), `UnauthorizedRole`

### release_milestone

- Signature: `release_milestone(env: Env, contract_id: u32, caller: Address, milestone_index: u32) -> bool`
- Kind: Mutating
- Auth: `caller.require_auth()`
- Semantics: Releases one milestone and transfers the escrowed funds to the freelancer (net of any configured protocol fee). The function clears approvals after a successful release and may complete the contract.
- Events: `("mlstn_rls", contract_id)`, and `("ctrct_cmp", contract_id)` when the contract reaches `Completed`
- Errors: `ContractNotFound`, `InvalidState`, `IndexOutOfBounds`, `MilestoneAlreadyReleased`, `AlreadyRefunded`, `InsufficientFunds`, `UnauthorizedRole`, `NotInitialized`, `ContractPaused`, `EmergencyActive`, `AlreadyFinalized`

### refund_unreleased_milestones

- Signature: `refund_unreleased_milestones(env: Env, contract_id: u32, milestone_indices: Vec<u32>) -> i128`
- Kind: Mutating
- Auth: `contract.client.require_auth()`
- Semantics: Refunds the requested unreleased milestones back to the client and updates the contract status when all remaining milestones are refunded or released.
- Events: None in the current implementation
- Errors: `EmptyRefundRequest`, `DuplicateMilestoneInRefund`, `ContractNotFound`, `InvalidState`, `IndexOutOfBounds`, `AlreadyReleased`, `AlreadyRefunded`, `InsufficientFunds`, `AlreadyFinalized`

### contract_exists

- Signature: `contract_exists(env: Env, contract_id: u32) -> bool`
- Kind: Read-only
- Auth: None
- Semantics: Checks whether a contract with the given ID exists in storage. Returns `true` if the contract record is present, `false` otherwise. This is a cheap, non-panicking existence probe suitable for indexers iterating over ID ranges. Unlike `get_contract`, this function does not panic with `ContractNotFound` for missing IDs.
- Events: None
- Errors: None
- Security: This is a read-only operation that does **not** extend the contract's TTL. Probing for contract existence cannot be abused to keep entries alive.

### get_next_contract_id

- Signature: `get_next_contract_id(env: Env) -> u32`
- Kind: Read-only
- Auth: None
- Semantics: Returns the next contract ID to be allocated (the allocation high-water mark). Indexers can use this to determine the allocated ID range `[1, get_next_contract_id() - 1]` and safely probe for existing contracts using `contract_exists`.
- Events: None
- Errors: None
- Security: This is a read-only operation that does not mutate contract state or extend TTL.

### get_contract

- Signature: `get_contract(env: Env, contract_id: u32) -> Contract`
- Kind: Read-only
- Auth: None
- Semantics: Returns the persisted contract record for the given ID.
- Events: None
- Errors: `ContractNotFound`

### get_milestones

- Signature: `get_milestones(env: Env, contract_id: u32) -> Vec<Milestone>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the milestone list for the contract.
- Events: None
- Errors: `ContractNotFound`

### get_refundable_balance

- Signature: `get_refundable_balance(env: Env, contract_id: u32) -> i128`
- Kind: Read-only
- Auth: None
- Semantics: Returns `funded_amount - released_amount - refunded_amount` for the contract.
- Events: None
- Errors: `ContractNotFound`

### get_milestone_approvals

- Signature: `get_milestone_approvals(env: Env, contract_id: u32, milestone_index: u32) -> Option<MilestoneApprovals>`
- Kind: Read query (touches temporary storage)
- Auth: None
- Semantics: Returns the approval state for a milestone, if it exists and has not expired. Successful reads renew the live temporary entry with `PENDING_APPROVAL_BUMP_THRESHOLD` / `PENDING_APPROVAL_TTL_LEDGERS`; missing or expired entries return `None` without creating state.
- Events: None
- Errors: None

### pause

- Signature: `pause(env: Env) -> bool`
- Kind: Mutating
- Auth: stored admin
- Semantics: Sets the global pause flag.
- Events: `("paused", timestamp)`
- Errors: `NotInitialized`, `UnauthorizedRole`

### unpause

- Signature: `unpause(env: Env) -> bool`
- Kind: Mutating
- Auth: stored admin
- Semantics: Clears the pause flag unless emergency mode is active.
- Events: `("unpaused", timestamp)`
- Errors: `NotInitialized`, `EmergencyActive`, `UnauthorizedRole`

### is_paused

- Signature: `is_paused(env: Env) -> bool`
- Kind: Read-only
- Auth: None
- Semantics: Returns the current pause flag.
- Events: None
- Errors: None

### activate_emergency_pause

- Signature: `activate_emergency_pause(env: Env) -> bool`
- Kind: Mutating
- Auth: stored admin
- Semantics: Enables emergency pause mode and sets the paused flag.
- Events: `("emergency", "activated")`
- Errors: `NotInitialized`, `UnauthorizedRole`

### resolve_emergency

- Signature: `resolve_emergency(env: Env) -> bool`
- Kind: Mutating
- Auth: stored admin
- Semantics: Clears emergency and paused flags.
- Events: `("emergency", "resolved")`
- Errors: `NotInitialized`, `UnauthorizedRole`

### is_emergency

- Signature: `is_emergency(env: Env) -> bool`
- Kind: Read-only
- Auth: None
- Semantics: Returns the current emergency flag.
- Events: None
- Errors: None

### cancel_contract

- Signature: `cancel_contract(env: Env, contract_id: u32, caller: Address) -> bool`
- Kind: Mutating
- Auth: `caller.require_auth()`
- Semantics: Cancels an active contract when the caller is the stored client or freelancer and the contract is still cancellable.
- Events: `("cancelled", contract_id)` via the shared status-change helper
- Errors: `ContractPaused`, `EmergencyActive`, `ContractNotFound`, `UnauthorizedRole`, `InvalidState`, `AlreadyFinalized`

### raise_dispute

- Signature: `raise_dispute(env: Env, contract_id: u32, caller: Address) -> bool`
- Kind: Mutating
- Auth: `caller.require_auth()`
- Semantics: Opens a dispute for a contract that has an assigned arbiter and is currently disputable.
- Events: `("dispute", "opened")`
- Errors: `ContractPaused`, `EmergencyActive`, `ContractNotFound`, `UnauthorizedRole`, `ArbiterRequired`, `InvalidState`, `AlreadyFinalized`

### resolve_dispute

- Signature: `resolve_dispute(env: Env, contract_id: u32, arbiter: Address, resolution: DisputeResolution) -> bool`
- Kind: Mutating
- Auth: `arbiter.require_auth()`
- Semantics: Resolves an open dispute by applying the arbiter-selected settlement and updating the contract accounting.
- Events: `("dispute", "resolved")`
- Errors: `ContractPaused`, `EmergencyActive`, `ContractNotFound`, `UnauthorizedRole`, `InvalidStatusTransition`, `InvalidDisputeSplit`, `AccountingInvariantViolated`, `PotentialOverflow`, `AlreadyFinalized`

### issue_reputation

- Signature: `issue_reputation(env: Env, contract_id: u32, caller: Address, rating: u32, comment: String) -> bool`
- Kind: Mutating
- Auth: `caller.require_auth()`
- Semantics: Issues reputation for a completed contract once. Updates the freelancer's aggregate reputation record and stores the provided comment.
- Events: None in the current implementation
- Errors: `ContractNotFound`, `UnauthorizedRole`, `InvalidRating`, `EmptyComment`, `CommentTooLong`, `NotCompleted`, `ReputationAlreadyIssued`, `SelfRating`, `InvalidState`

### get_reputation_comment

- Signature: `get_reputation_comment(env: Env, contract_id: u32) -> Option<String>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the client-supplied comment for a contract, if reputation was issued.
- Events: None
- Errors: None

### get_reputation

- Signature: `get_reputation(env: Env, address: Address) -> Option<types::Reputation>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the persisted reputation aggregate for the address, if one exists.
- Events: None
- Errors: None

### get_average_rating

- Signature: `get_average_rating(env: Env, address: Address) -> Option<i128>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the average rating in basis points, or `None` if there is no completed-contract reputation record.
- Events: None
- Errors: None

### get_pending_reputation_credits

- Signature: `get_pending_reputation_credits(env: Env, address: Address) -> i128`
- Kind: Read-only
- Auth: None
- Semantics: Returns the pending reputation credits for the freelancer.
- Events: None
- Errors: None

### submit_work_evidence

- Signature: `submit_work_evidence(env: Env, contract_id: u32, caller: Address, milestone_index: u32, evidence: String) -> bool`
- Kind: Mutating
- Auth: `caller.require_auth()`
- Semantics: Stores evidence for an unreleased milestone. Evidence is capped to 256 bytes.
- Events: `("evidence", contract_id)`
- Errors: `ContractPaused`, `EmergencyActive`, `ContractNotFound`, `AlreadyFinalized`, `UnauthorizedRole`, `InvalidState`, `IndexOutOfBounds`, `MilestoneAlreadyReleased`, `AlreadyRefunded`, `EvidenceTooLong`

### get_work_evidence

- Signature: `get_work_evidence(env: Env, contract_id: u32, milestone_index: u32) -> Option<String>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the stored evidence for a milestone, if any.
- Events: None
- Errors: `ContractNotFound`

### set_protocol_fee_bps

- Signature: `set_protocol_fee_bps(env: Env, new_bps: u32) -> bool`
- Kind: Mutating
- Auth: stored admin
- Semantics: Updates the configured protocol fee in basis points, capped at `10_000` (100%).
- Events: `("protocol_fee_bps",)`
- Errors: `NotInitialized`, `UnauthorizedRole`, `InvalidProtocolParameters`

### get_protocol_fee_bps_view

- Signature: `get_protocol_fee_bps_view(env: Env) -> u32`
- Kind: Read-only
- Auth: None
- Semantics: Returns the configured protocol fee in basis points.
- Events: None
- Errors: None

### get_accumulated_protocol_fees

- Signature: `get_accumulated_protocol_fees(env: Env) -> i128`
- Kind: Read-only
- Auth: None
- Semantics: Returns the cumulative retained protocol fees.
- Events: None
- Errors: None

### propose_governance_admin

- Signature: `propose_governance_admin(env: Env, proposed: Address) -> bool`
- Kind: Mutating
- Auth: stored admin
- Semantics: Starts a two-step governance-admin transfer proposal with a timelock.
- Events: `("admin", "proposed")`
- Errors: `NotInitialized`, `UnauthorizedRole`, `InvalidState` (for missing proposal state in helper paths)

### accept_governance_admin

- Signature: `accept_governance_admin(env: Env) -> bool`
- Kind: Mutating
- Auth: proposed admin
- Semantics: Completes the timelocked governance-admin transfer if the timelock has elapsed.
- Events: `("admin", "accepted")`
- Errors: `NotInitialized`, `InvalidState`, `TimelockNotElapsed`, `UnauthorizedRole`

### get_pending_governance_admin

- Signature: `get_pending_governance_admin(env: Env) -> Option<Address>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the pending governance-admin proposal, if any.
- Events: None
- Errors: None

### get_governance_admin

- Signature: `get_governance_admin(env: Env) -> Option<Address>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the current governance admin address, if any.
- Events: None
- Errors: None

### set_governed_params

- Signature: `set_governed_params(env: Env, admin: Address, protocol_fee_bps: u32, max_escrow_total_stroops: i128) -> bool`
- Kind: Mutating
- Auth: stored admin
- Semantics: Stores the protocol fee (capped at `10_000`, or 100%) and maximum escrow total and updates the readiness checklist.
- Events: None
- Errors: `NotInitialized`, `UnauthorizedRole`, `InvalidProtocolParameters`

### get_governed_parameters

- Signature: `get_governed_parameters(env: Env) -> Option<GovernedParameters>`
- Kind: Read-only
- Auth: None
- Semantics: Returns the stored governance parameters, if present.
- Events: None
- Errors: None

## Error-code cross-reference

The authoritative error enums are in [contracts/escrow/src/lib.rs](../../contracts/escrow/src/lib.rs) and [contracts/escrow/src/types.rs](../../contracts/escrow/src/types.rs). The ABI summary above uses the current live error names and maps them to the same contract-facing error values used by the runtime.

## Security notes

- Mutating entrypoints are guarded by the initialization, pause, and emergency flags where applicable.
- Admin and participant authorization is enforced via `require_auth()` on the relevant caller addresses.
- Finalization and migration flows are documented as live only where the current implementation actually exposes them.
- Planned fee-withdrawal, migration, and admin-transfer entrypoints are intentionally not described as live API in this document.
