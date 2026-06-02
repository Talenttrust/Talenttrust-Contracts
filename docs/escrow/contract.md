# Escrow Contract Documentation

**Mainnet readiness (limits, events, risks):** [mainnet-readiness.md](mainnet-readiness.md)

This document summarizes the reviewer-facing architecture for
`contracts/escrow` as implemented in `contracts/escrow/src/lib.rs`.

## Scope

The live contract persists:

- escrow lifecycle state for each contract
- client and freelancer addresses
- milestone amounts and per-milestone release flags
- deposited, released, and refunded accounting totals
- reputation aggregates and pending reputation credits
- immutable finalization records for closed contracts
- one operational admin address
- pause and emergency flags
- a readiness checklist

The live contract does not implement token transfers, protocol fee deduction,
two-step admin transfer, refund flows, approval expiry, or schema migration
entrypoints. Planned items are listed below with tracking issues.

## Public Entrypoints

Core escrow endpoints:

- `create_contract(client, freelancer, milestone_amounts, deposit_mode) -> u32`
- `deposit_funds(contract_id, amount) -> bool`
- `release_milestone(contract_id, milestone_index) -> bool`
- `issue_reputation(contract_id, caller, freelancer, rating) -> bool`
- `cancel_contract(contract_id, caller) -> bool`
- `finalize_contract(contract_id, finalizer) -> bool`
- `get_contract(contract_id) -> EscrowContractData`
- `get_finalization_record(contract_id) -> Option<FinalizationRecord>`
- `get_reputation(freelancer) -> Option<ReputationRecord>`
- `get_pending_reputation_credits(freelancer) -> u32`

Operational controls:

- `initialize(admin) -> bool`
- `get_admin() -> Option<Address>`
- `pause() -> bool`
- `unpause() -> bool`
- `is_paused() -> bool`
- `activate_emergency_pause() -> bool`
- `resolve_emergency() -> bool`
- `is_emergency() -> bool`
- `get_mainnet_readiness_info() -> MainnetReadinessInfo`

## Function Semantics

### `initialize(admin) -> bool`

One-time setup for the operational admin. Requires `admin.require_auth()` and
fails with `AlreadyInitialized` if called twice.

### `create_contract(client, freelancer, milestone_amounts, deposit_mode) -> u32`

Creates a contract in `Created` status. Requires `client.require_auth()`.
Rejects identical client/freelancer addresses, empty milestones, too many
milestones, non-positive milestone amounts, checked-arithmetic overflow, and
totals above `MAX_TOTAL_ESCROW_STROOPS`.

### `deposit_funds(contract_id, amount) -> bool`

Adds escrow accounting balance. Rejects non-positive amounts, unknown contract
ids, exact-total deposits that are not exactly the milestone sum, repeat exact
deposits, and incremental deposits that would exceed the milestone sum.

### `release_milestone(contract_id, milestone_index) -> bool`

Marks one milestone released and increments `released_amount`. Rejects paused
state, unknown contract ids, invalid milestone indexes, double release, and
insufficient funded balance.

Current implementation note: this entrypoint does not authenticate the client or
arbiter. Do not document or rely on client-required release authorization until
the authorization fix lands.

### `issue_reputation(contract_id, caller, freelancer, rating) -> bool`

Requires `caller.require_auth()`. The caller must be the stored client, the
freelancer argument must match the stored freelancer, the contract must be
`Completed`, rating must be `1..=5`, and reputation can be issued only once per
contract.

### `cancel_contract(contract_id, caller) -> bool`

Requires `caller.require_auth()`. The caller must be the stored client or
freelancer. Cancellation fails for unknown contracts, already-cancelled
contracts, completed contracts, and unauthorized callers.

### `finalize_contract(contract_id, finalizer) -> bool`

Requires `finalizer.require_auth()`. The finalizer must be the stored client,
freelancer, or assigned arbiter. The contract must be in `Completed` or
`Disputed` status. Finalization writes one immutable `FinalizationRecord` with
the finalizer, ledger timestamp, and `ContractSummary` snapshot.

After finalization, contract-specific mutating entrypoints reject with
`AlreadyFinalized`; read-only queries remain available.

### Read-only Queries

`get_contract` panics with `ContractNotFound` for unknown ids. Reputation,
pending credits, admin, pause, emergency, and mainnet readiness queries return
stored values or documented defaults.

## Lifecycle Model

Implemented status transitions:

- `Created -> PartiallyFunded` through an incremental deposit below the total.
- `Created -> Funded` through an exact deposit or final incremental deposit.
- `PartiallyFunded -> Funded` through the final incremental deposit.
- `Created`, `PartiallyFunded`, or `Funded -> Cancelled` through
  `cancel_contract`.
- `Funded -> Completed` after all milestones are released.
- `Completed` or `Disputed -> finalized metadata written` through
  `finalize_contract`. The status itself is preserved in the immutable summary.

`Accepted`, `Disputed`, and `Refunded` enum variants exist but no public
entrypoint currently transitions a contract into those states.

## Events

Implemented event topics:

- `("init", "admin_set")`
- `("paused", timestamp)`
- `("unpaused", timestamp)`
- `("emergency", "activated")`
- `("emergency", "resolved")`
- `("audit", contract_id)`
- `("created", contract_id)`
- `("released", contract_id, milestone_index)`
- `("rep_issd", contract_id)`
- `("cancelled", contract_id)`
- `("finalized", contract_id)`

The deterministic v1 lifecycle event schema previously described for
`approve`, `refund`, `finalize`, `withdraw`, and protocol-fee events is not
implemented in `lib.rs`.

## Planned Features

- Two-step admin transfer:
  [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318)
- Protocol fee deduction:
  [#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313)
- Protocol fee withdrawal:
  [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314)
- Governed parameter setter/readiness wiring:
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323)
- Structured deposit and fee events:
  [#336](https://github.com/Talenttrust/Talenttrust-Contracts/issues/336)
- Canonical storage-key documentation:
  [#342](https://github.com/Talenttrust/Talenttrust-Contracts/issues/342)
- `migrate_state` / `StateV1` / `StateV2` migration flow:
  [#341](https://github.com/Talenttrust/Talenttrust-Contracts/issues/341)

## Security Notes

- Mutating lifecycle operations fail while paused or in emergency.
- Admin controls require the stored admin's authentication after initialization.
- Amount math uses checked helpers for aggregate totals.
- Release accounting is state-only; actual token custody and transfer logic are
  outside the current contract surface.
- Duplicate release and duplicate reputation issuance are rejected.
- Finalization is authenticated, allowed only from `Completed` or `Disputed`,
  and prevents later contract-specific state mutation.
- Storage TTL constants are exported for planned pending records, but current
  live lifecycle records use persistent storage and no public approval/migration
  TTL flow exists.
