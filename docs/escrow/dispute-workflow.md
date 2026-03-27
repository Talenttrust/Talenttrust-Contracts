# Escrow Dispute Workflow

## Overview

The TalentTrust escrow contract supports a formal on-chain dispute mechanism. Either the **client** or the **freelancer** may raise a dispute against a funded or completed escrow. Once raised, the dispute is recorded immutably in persistent storage and the escrow status transitions to `Disputed`.

## State Machine

```
┌─────────┐   deposit    ┌────────┐   dispute   ┌──────────┐
│ Created │ ──────────► │ Funded │ ──────────► │ Disputed │
└─────────┘             └────────┘             └──────────┘
                             │
                          complete
                             │
                             ▼
                        ┌───────────┐   dispute   ┌──────────┐
                        │ Completed │ ──────────► │ Disputed │
                        └───────────┘             └──────────┘
```

Valid transitions to `Disputed`:
- `Funded` → `Disputed`
- `Completed` → `Disputed`

Invalid (rejected with error):
- `Created` → `Disputed` — returns `DisputeError::InvalidStatus`
- `Disputed` → `Disputed` — returns `DisputeError::AlreadyDisputed`

## Data Types

### `DisputeRecord`

Immutable record written to persistent storage when a dispute is initiated.

```rust
pub struct DisputeRecord {
    /// The address (client or freelancer) that initiated the dispute.
    pub initiator: Address,
    /// A short human-readable reason for the dispute.
    pub reason: String,
    /// Ledger timestamp (seconds since Unix epoch) at the moment the dispute was recorded.
    pub timestamp: u64,
}
```

### `DisputeError`

Typed error enum returned by dispute functions.

| Variant           | Value | Meaning                                                  |
|-------------------|-------|----------------------------------------------------------|
| `NotFound`        | 1     | No escrow with the given `contract_id` exists            |
| `Unauthorized`    | 2     | Caller is not the client or freelancer of this escrow    |
| `InvalidStatus`   | 3     | Escrow is in `Created` status (not yet funded)           |
| `AlreadyDisputed` | 4     | A dispute record already exists for this escrow          |

## Functions

### `initiate_dispute`

```rust
pub fn initiate_dispute(
    env: Env,
    contract_id: u32,
    initiator: Address,
    reason: String,
) -> Result<(), DisputeError>
```

Raises a dispute on an existing escrow.

**Execution flow:**

1. `initiator.require_auth()` — Soroban-level authorization enforced before any state access.
2. Load `EscrowState` from persistent storage; return `NotFound` if absent.
3. Validate `initiator == state.client || initiator == state.freelancer`; return `Unauthorized` otherwise.
4. Check `state.status`:
   - `Created` → return `InvalidStatus`
   - `Disputed` → return `AlreadyDisputed`
   - `Funded` or `Completed` → continue
5. Check for existing `DisputeRecord`; return `AlreadyDisputed` if present (defense-in-depth).
6. Set `state.status = Disputed` and persist updated `EscrowState`.
7. Write `DisputeRecord { initiator, reason, timestamp: env.ledger().timestamp() }` to persistent storage.

### `get_dispute`

```rust
pub fn get_dispute(env: Env, contract_id: u32) -> Option<DisputeRecord>
```

Returns the dispute record for an escrow, or `None` if no dispute has been initiated.

## Security Assumptions and Threat Scenarios

### Authorization

- `require_auth()` is the **first** operation — no storage reads or writes occur before the caller is authenticated.
- Soroban's auth framework ensures that if `require_auth()` panics, the entire transaction is reverted atomically.

### Immutability of DisputeRecord

- The record is guarded by two independent checks before writing:
  1. `state.status == Disputed` check (status-level guard).
  2. `env.storage().persistent().has(DataKey::Dispute(id))` check (storage-level guard).
- This defense-in-depth ensures the record cannot be overwritten even if the status check were somehow bypassed.

### Access Control

- The `initiator` address is validated against the **on-chain** `client` and `freelancer` addresses stored at escrow creation time — not against any caller-supplied claim.
- A third-party address (even one that passes `require_auth()`) will be rejected with `Unauthorized`.

### Threat Scenarios

| Threat | Mitigation |
|--------|-----------|
| Attacker calls `initiate_dispute` without authorization | `require_auth()` panics; transaction reverts |
| Attacker supplies a different address as `initiator` | On-chain address comparison rejects non-parties |
| Client/freelancer tries to dispute a `Created` escrow | `InvalidStatus` returned; no state change |
| Party tries to overwrite an existing dispute record | `AlreadyDisputed` returned; record unchanged |
| Reentrancy via cross-contract call | Soroban's single-threaded execution model prevents reentrancy |

## Testing

All acceptance criteria are covered by unit tests in `contracts/escrow/src/test.rs`:

| Test | Covers |
|------|--------|
| `test_initiate_dispute_from_client` | Req 1.1 — Funded → Disputed (client) |
| `test_initiate_dispute_from_freelancer` | Req 1.1 — Funded → Disputed (freelancer) |
| `test_dispute_on_completed_escrow` | Req 1.2 — Completed → Disputed |
| `test_dispute_on_created_escrow_fails` | Req 1.3 — Created rejected |
| `test_dispute_already_disputed_fails` | Req 1.4 — duplicate rejected |
| `test_dispute_unauthorized_caller` | Req 1.5, 4.3 — third party rejected |
| `test_get_dispute_no_record` | Req 2.4 — None before dispute |
| `test_get_dispute_returns_record` | Req 2.1, 2.3 — record round-trip |
