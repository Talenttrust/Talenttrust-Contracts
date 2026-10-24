# Disputes Threat Model

This document covers trust assumptions, attacker capabilities, and mitigations
for the dispute subsystem in `contracts/escrow/src/dispute.rs` and its
integration points in `lib.rs`, `finalize.rs`, and `types.rs`.

## Scope

- `DisputeResolution` enum and `resolution_payouts()` in `dispute.rs`
- `final_status_after_resolution()` in `dispute.rs`
- `ContractStatus::Disputed` transitions in `lib.rs` and `finalize.rs`
- `finalize_contract` allowing `Disputed` as a terminal entry in `finalize.rs`
- Accounting invariant checks across all dispute paths

## Trust Assumptions

| Assumption | Rationale |
|---|---|
| The **arbiter** is a semi-trusted third party agreed upon at contract creation. | The arbiter alone can resolve disputes and choose the fund split. No on-chain mechanism enforces arbiter fairness; the contract relies on the parties' off-chain selection. |
| **Client** and **freelancer** are adversarial peers. | Each party may act in self-interest; the contract never assumes cooperation between them. |
| The **admin** (protocol operator) is trusted for pause/emergency only. | Admin cannot resolve disputes, release funds, or override accounting. Admin can only freeze operations. |
| Token custody and token transfers are handled **outside** this contract. | The escrow records accounting state only; actual SPL/Stellar token movements must be integrated and audited separately. |
| The arbiter address is set once at contract creation and **cannot be changed**. | No entrypoint exists to reassign the arbiter after `create_contract`. |

## Attacker Capabilities and Mitigations

### A1: Unauthorized outsider raises or resolves a dispute

**Capability:** An address with no relationship to the contract attempts `raise_dispute` or `resolve_dispute`.

**Mitigations:**
- `raise_dispute` requires the caller to be the stored client or freelancer (`UnauthorizedRole` error). Cross-ref: `lib.rs` contract party checks.
- `raise_dispute` requires an assigned arbiter (`ArbiterRequired` error). Cross-ref: `dispute.rs:97` test.
- `resolve_dispute` requires the caller to be the assigned arbiter (`UnauthorizedRole` error). Cross-ref: `dispute.rs:208` test.
- All calls require `caller.require_auth()` enforced by Soroban's auth engine.

**Residual risk:** Low. Access control is role-based and enforced before any state mutation.

### A2: Compromised arbiter chooses an unfair resolution

**Capability:** An arbiter whose key is compromised (or acts maliciously) selects `FullPayout` or a skewed `Split` favoring one party.

**Mitigations:**
- The arbiter is chosen by both parties at contract creation. Off-chain vetting is the primary defense.
- `Split` amounts must exactly equal the available balance (`InvalidDisputeSplit` error). The arbiter cannot extract more than the escrow holds.
- `resolution_payouts()` computes payouts from the accounting invariant: `available = funded_amount - released_amount - refunded_amount`. No new funds are created.
- After resolution, `finalize_contract` writes an immutable `FinalizationRecord` with the arbiter's address, timestamp, and full accounting snapshot, creating a permanent audit trail.

**Residual risk:** Medium. On-chain enforcement guarantees accounting correctness but cannot guarantee fairness of the arbiter's subjective decision. Off-chain reputation and legal agreements are the complementary mitigation.

### A3: Compromised client or freelancer raises a frivolous dispute

**Capability:** A party whose key is compromised raises a dispute on a healthy contract to freeze operations.

**Mitigations:**
- `raise_dispute` transitions the contract to `Disputed`, which **blocks** `release_milestone` (cross-ref: `test/dispute.rs:246` `release_is_blocked_while_disputed`).
- `cancel_contract` is also blocked in `Disputed` state (`InvalidStatusTransition`). Cross-ref: `test/cancel_contract.rs:451-515`.
- The arbiter can resolve the dispute through `resolve_dispute`, restoring funds to either party.
- If the arbiter is unresponsive, finalization via `finalize_contract` from `Disputed` state writes an immutable record. The contract remains in `Disputed` until resolved or finalized.

**Residual risk:** Medium. A compromised party can temporarily freeze operations. The arbiter and finalization provide recovery paths but introduce delay.

### A4: Admin freezes disputes via pause

**Capability:** The admin calls `pause()` to block all mutating operations including `raise_dispute` and `resolve_dispute`.

**Mitigations:**
- Pause and unpause require `admin.require_auth()`.
- Emergency pause additionally sets `Emergency` flag, which blocks `unpause()` until `resolve_emergency()` is called by the admin.
- Paused state is a circuit breaker, not a resolution mechanism. It does not change fund accounting.
- Tests confirm: `pause_blocks_raise_and_resolve_dispute` (cross-ref: `test/dispute.rs:265`).

**Residual risk:** Low. Admin abuse is an operational risk mitigated by off-chain governance and the two-step admin transfer (planned: #318).

### A5: Double-spend or accounting manipulation during dispute resolution

**Capability:** An attacker attempts to extract more funds than the escrow holds, or manipulate accounting during resolution.

**Mitigations:**
- `resolution_payouts()` computes `available = funded_amount - released_amount - refunded_amount` using checked subtraction. Returns `AccountingInvariantViolated` if the invariant breaks.
- `Split(client_amount, freelancer_amount)` validates `client_amount + freelancer_amount == available` via `safe_add_amounts()`. Returns `InvalidDisputeSplit` if the total doesn't match.
- Negative split amounts are rejected (`InvalidDisputeSplit`).
- `final_status_after_resolution()` sets `Refunded` only if `refunded_amount == funded_amount`, otherwise `Completed`. This prevents inconsistent terminal states.
- All arithmetic uses checked helpers (`checked_sub`, `checked_mul`, `checked_div`, `safe_add_amounts`) returning `Option<T>` with `PotentialOverflow` errors.

**Residual risk:** Low. The accounting invariant is enforced at the math level with no bypass paths.

### A6: State transition attacks

**Capability:** An attacker attempts to resolve a non-disputed contract, raise a dispute on a completed contract, or perform other invalid transitions.

**Mitigations:**
- `resolve_dispute` requires `ContractStatus::Disputed` (`InvalidStatusTransition` error). Cross-ref: `test/dispute.rs:228`.
- `raise_dispute` requires `Funded` or `PartiallyFunded` status.
- `finalize_contract` from `Disputed` status is allowed but produces an immutable record. After finalization, all contract-specific mutations fail with `AlreadyFinalized`.
- `release_milestone` is blocked while in `Disputed` status (`InvalidState` error). Cross-ref: `test/dispute.rs:246`.
- `cancel_contract` is blocked in `Disputed` status (`InvalidStatusTransition`). Cross-ref: `test/cancel_contract.rs:451`.

**Residual risk:** Low. All transitions are explicitly guarded with status checks before mutations.

### A7: Replay or re-resolution after dispute resolution

**Capability:** An attacker attempts to resolve an already-resolved dispute or re-raise a dispute on a resolved contract.

**Mitigations:**
- After resolution, the contract transitions to `Completed` or `Refunded` (terminal states for dispute purposes).
- `finalize_contract` writes an immutable `FinalizationRecord`. After finalization, all mutations are blocked with `AlreadyFinalized`.
- `resolve_dispute` only accepts contracts in `Disputed` status.
- `raise_dispute` only accepts contracts in `Funded` or `PartiallyFunded` status.

**Residual risk:** Low. Terminal state transitions and finalization provide idempotent guards.

## Auth Check Cross-Reference

| Operation | Caller Requirement | Auth Mechanism | Status Guard | Error Codes |
|---|---|---|---|---|
| `raise_dispute` | Client or freelancer | `require_auth()` | `Funded` or `PartiallyFunded` | `UnauthorizedRole`, `ArbiterRequired`, `InvalidStatusTransition`, `ContractPaused`, `EmergencyActive`, `AlreadyFinalized` |
| `resolve_dispute` | Assigned arbiter | `require_auth()` | `Disputed` | `UnauthorizedRole`, `InvalidStatusTransition`, `ContractPaused`, `EmergencyActive`, `AlreadyFinalized` |
| `finalize_contract` | Client, freelancer, or arbiter | `require_auth()` | `Completed` or `Disputed` | `UnauthorizedRole`, `InvalidStatusTransition`, `ContractPaused`, `EmergencyActive`, `AlreadyFinalized` |
| `pause` | Admin | `require_auth()` | Any (global) | `NotInitialized` |
| `cancel_contract` | Client or freelancer | `require_auth()` | `Created`, `PartiallyFunded`, or `Funded` | `UnauthorizedRole`, `InvalidState`, `AlreadyFinalized` |

## Accounting Invariant

The core invariant enforced across all dispute paths:

```
available_balance = funded_amount - released_amount - refunded_amount
available_balance >= 0
client_payout + freelancer_payout == available_balance  (for Split resolution)
```

Violation of this invariant returns `AccountingInvariantViolated` or `InvalidDisputeSplit`.
All arithmetic uses checked operations (`checked_sub`, `checked_mul`, `checked_div`,
`safe_add_amounts`) to prevent overflow.

## Dispute Lifecycle State Machine

```
Created ──(deposit)──> PartiallyFunded ──(deposit)──> Funded
                         │                              │
                         │         raise_dispute         │
                         └──────────> Disputed <──────────┘
                                        │
                         resolve_dispute │ finalize_contract
                         ┌───────────────┴───────────────┐
                         ▼                               ▼
                    Completed                       Finalized
                    or Refunded                  (immutable record)
```

- `Disputed` blocks: `release_milestone`, `cancel_contract`, `refund_unreleased_milestones`
- `Completed`/`Refunded` are terminal; `finalize_contract` writes an immutable record
- After finalization: all mutations fail with `AlreadyFinalized`

## Open Issues

- `raise_dispute` and `resolve_dispute` are not yet public entrypoints in `lib.rs`. The internal logic in `dispute.rs` is implemented and tested, but the Soroban `#[contractimpl]` entrypoints are pending.
- Arbiter reassignment is not supported. If the arbiter key is lost, dispute resolution is blocked until finalization.
- No on-chain mechanism enforces arbiter fairness beyond accounting correctness.
- Token custody and transfers are outside this contract's scope and must be audited separately.
