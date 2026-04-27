# Escrow Dispute Workflow

## Overview

The TalentTrust escrow contract supports a formal on-chain dispute mechanism. Either the **client** or the **freelancer** may raise a dispute against a funded escrow. Once raised, dispute metadata is stored in persistent storage and the escrow status transitions to `Disputed`. Only the **arbiter** may resolve the dispute.

## State Machine

```
┌─────────┐   deposit    ┌────────┐   raise_dispute   ┌──────────┐
│ Created │ ──────────► │ Funded │ ────────────────► │ Disputed │
└─────────┘             └────────┘                   └──────────┘
                                                           │
                                              resolve_dispute(decision)
                                                           │
                                    ┌──────────────────────┼──────────────────────┐
                                    ▼                      ▼                      ▼
                               ┌───────────┐         ┌──────────┐          ┌───────────┐
                               │ Completed │         │ Refunded │          │ Cancelled │
                               └───────────┘         └──────────┘          └───────────┘
                               (Release)             (Refund)               (Cancel)
```

Valid transitions:
- `Funded` → `Disputed` via `raise_dispute` (client or freelancer only)
- `Disputed` → `Completed` via `resolve_dispute(Release)` (arbiter only)
- `Disputed` → `Refunded` via `resolve_dispute(Refund)` (arbiter only)
- `Disputed` → `Cancelled` via `resolve_dispute(Cancel)` (arbiter only)

Invalid (rejected):
- `Created` → `Disputed` — `InvalidStatusTransition` (contract not yet funded)
- `Disputed` → `Disputed` — `InvalidStatusTransition` (already disputed)
- Any state → `Disputed` by arbiter — `UnauthorizedRole`
- `Disputed` → any by non-arbiter — `UnauthorizedRole`

## Data Types

### `DisputeMetadata`

Written to persistent storage when a dispute is raised.

```rust
pub struct DisputeMetadata {
    /// SHA-256 hash of the off-chain dispute reason document.
    pub reason_hash: BytesN<32>,
    /// Ledger timestamp (seconds since Unix epoch) when the dispute was raised.
    pub raised_at: u64,
    /// Address (client or freelancer) that raised the dispute.
    pub raised_by: Address,
}
```

### `DisputeResolution`

Arbiter decision passed to `resolve_dispute`.

```rust
pub enum DisputeResolution {
    /// Release all remaining funded milestones to the freelancer → Completed.
    Release = 0,
    /// Refund all remaining funded milestones to the client → Refunded.
    Refund  = 1,
    /// Cancel the contract with no further payments → Cancelled.
    Cancel  = 2,
}
```

## Functions

### `raise_dispute`

```rust
pub fn raise_dispute(
    env: Env,
    contract_id: u32,
    caller: Address,
    reason_hash: BytesN<32>,
) -> bool
```

Raises a dispute on a funded escrow.

**Execution flow:**

1. `caller.require_auth()` — Soroban-level authorization enforced first.
2. Load `EscrowContractData`; panic with `ContractNotFound` if absent.
3. Validate `caller == contract.client || caller == contract.freelancer`; panic with `UnauthorizedRole` otherwise.
4. Validate `contract.arbiter.is_some()`; panic with `NoArbiter` if no arbiter is assigned.
5. Validate `contract.status == Funded`; panic with `InvalidStatusTransition` otherwise.
6. Transition `contract.status = Disputed` and persist.
7. Write `DisputeMetadata { reason_hash, raised_at, raised_by }` to persistent storage.
8. Emit `dispute_raised` event: topics `(dispute_raised, contract_id)`, data `(caller, reason_hash, timestamp)`.

### `resolve_dispute`

```rust
pub fn resolve_dispute(
    env: Env,
    contract_id: u32,
    arbiter: Address,
    resolution: DisputeResolution,
) -> bool
```

Resolves a disputed escrow. Only the arbiter may call this.

**Execution flow:**

1. `arbiter.require_auth()` — Soroban-level authorization enforced first.
2. Load `EscrowContractData`; panic with `ContractNotFound` if absent.
3. Validate `contract.arbiter == Some(arbiter)`; panic with `UnauthorizedRole` otherwise.
4. Validate `contract.status == Disputed`; panic with `InvalidStatusTransition` otherwise.
5. Transition status based on `resolution`:
   - `Release` → `Completed`
   - `Refund` → `Refunded`
   - `Cancel` → `Cancelled`
6. Persist updated contract.
7. Emit `dispute_resolved` event: topics `(dispute_resolved, contract_id)`, data `(arbiter, resolution, timestamp)`.

### `get_dispute`

```rust
pub fn get_dispute(env: Env, contract_id: u32) -> DisputeMetadata
```

Returns the dispute metadata for a contract. Panics with `DisputeNotFound` if no dispute has been raised.

## Error Codes

| Variant                  | Code | Meaning                                                    |
|--------------------------|------|------------------------------------------------------------|
| `UnauthorizedRole`       | 6    | Caller is not a party or arbiter of this contract          |
| `InvalidStatusTransition`| 7    | Contract is not in the required state for this operation   |
| `NoArbiter`              | 13   | `raise_dispute` called on a contract with no arbiter       |
| `DisputeNotFound`        | 14   | `get_dispute` called before any dispute was raised         |

## Events

| Event name         | Topics                          | Data                                    |
|--------------------|---------------------------------|-----------------------------------------|
| `dispute_raised`   | `(dispute_raised, contract_id)` | `(caller, reason_hash, timestamp)`      |
| `dispute_resolved` | `(dispute_resolved, contract_id)` | `(arbiter, resolution, timestamp)`    |

Events are emitted after all state changes are persisted, making them safe for off-chain indexers.

## Security Properties

| Property | Mechanism |
|----------|-----------|
| Only parties may raise | `caller == client \|\| caller == freelancer` checked against on-chain state |
| Only arbiter may resolve | `arbiter == contract.arbiter` checked against on-chain state |
| Requires auth before any state read | `require_auth()` is the first call in both functions |
| No dispute without arbiter | `NoArbiter` error prevents disputes on arbiter-less contracts |
| Release blocked during dispute | `release_milestone` panics with `InvalidStatusTransition` in `Disputed` state |
| No double-dispute | Second `raise_dispute` fails because status is no longer `Funded` |

## Testing

Tests are in `contracts/escrow/src/test/dispute.rs`:

| Test | Scenario |
|------|----------|
| `client_can_raise_dispute_on_funded_contract` | Happy path — client raises |
| `freelancer_can_raise_dispute_on_funded_contract` | Happy path — freelancer raises |
| `raise_dispute_stores_metadata` | Metadata round-trip |
| `arbiter_can_resolve_with_release` | Resolution → Completed |
| `arbiter_can_resolve_with_refund` | Resolution → Refunded |
| `arbiter_can_resolve_with_cancel` | Resolution → Cancelled |
| `arbiter_cannot_raise_dispute` | Unauthorized raise |
| `third_party_cannot_raise_dispute` | Unauthorized raise |
| `cannot_raise_dispute_without_arbiter` | NoArbiter guard |
| `cannot_raise_dispute_on_created_contract` | Invalid state transition |
| `cannot_raise_dispute_twice` | Double-dispute guard |
| `client_cannot_resolve_dispute` | Unauthorized resolve |
| `freelancer_cannot_resolve_dispute` | Unauthorized resolve |
| `third_party_cannot_resolve_dispute` | Unauthorized resolve |
| `cannot_resolve_non_disputed_contract` | Invalid state transition |
| `release_milestone_blocked_in_disputed_state` | State blocking |
| `get_dispute_fails_when_no_dispute_exists` | DisputeNotFound guard |
