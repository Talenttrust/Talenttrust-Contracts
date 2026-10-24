# Milestone Authorization and Access Rules

This document describes who may call each milestone-related entrypoint, which
contract states are required, and what errors are returned when the rules are
violated. It is the authoritative reference for roles, state transitions, and
rejection conditions across the escrow lifecycle.

For release authorization mode specifics (approve-then-release flow, TTL
details, per-mode approval matrices) see
[`docs/escrow/authorization.md`](escrow/authorization.md). For the full ABI
surface see [`docs/escrow/abi-reference.md`](escrow/abi-reference.md).

---

## Roles

| Role | How it is identified |
|---|---|
| **client** | `contract.client` — the address that funded the escrow |
| **freelancer** | `contract.freelancer` — the address that performs the work |
| **arbiter** | `contract.arbiter` (optional) — assigned at contract creation; required for `ArbiterOnly` and `ClientAndArbiter` release modes, and for any dispute |
| **admin** | The address stored under `DataKey::Admin` after `initialize` — controls pause, emergency, and governance; never participates in individual escrow contracts |

Addresses must be distinct: client ≠ freelancer, arbiter ≠ client, arbiter ≠
freelancer. `create_contract` enforces these invariants and panics with
`InvalidParticipant` or `InvalidArbiter` on violation.

---

## Contract States

The `ContractStatus` state machine determines which operations are legal at any
point. A contract begins in `Created` and may only move forward; transitions
are irreversible unless noted.

```
Created
  │  deposit_funds (partial)
  ▼
PartiallyFunded
  │  deposit_funds (completes total)
  ▼
Funded ──────────────────────────────┐
  │  release_milestone(s)             │ raise_dispute
  │  (all released/refunded → Complete) │
  ▼                                   ▼
Completed                          Disputed
  │  finalize_contract               │  resolve_dispute
  ▼                                  │
Finalized (immutable record)        ▼
                                  Completed or Refunded
                                     │  finalize_contract
                                     ▼
                                  Finalized

Created / Funded → Cancelled   (cancel_contract, no milestones released)
Funded / Disputed → Refunded   (refund_unreleased_milestones, all refunded)
```

---

## Global Guards — Pause and Emergency

Every state-changing entrypoint runs `require_not_paused` before any auth
check or business logic. The guard panics with:

- `ContractPaused` (`Error::37`) when `DataKey::Paused` is `true`
- `EmergencyActive` (`Error::38`) when `DataKey::Emergency` is `true`

Read-only queries (`get_contract`, `get_milestones`, `get_milestone_approvals`,
etc.) are never blocked. The admin controls these flags via `pause`,
`unpause`, `activate_emergency_pause`, and `resolve_emergency`.

> All auth and state checks described below assume the pause guard has already
> passed. An active pause stops execution before any per-role check is reached.

---

## Entrypoint Authorization Table

| Entrypoint | Authorized callers | Required contract state | Finalized? | Key error codes |
|---|---|---|---|---|
| `create_contract` | client | — (creates new contract) | — | `ContractPaused`, `InvalidParticipant`, `MissingArbiter`, `InvalidArbiter`, `EmptyMilestones`, `InvalidMilestoneAmount`, `TooManyMilestones`, `TotalCapExceeded` |
| `deposit_funds` | client | `Created` or `PartiallyFunded` | blocked | `ContractPaused`, `UnauthorizedRole`, `InvalidDepositAmount`, `InvalidState` |
| `approve_milestone_release` | mode-dependent (see below) | `Funded` or `PartiallyFunded` | blocked | `ContractPaused`, `AlreadyFinalized`, `UnauthorizedRole`, `AlreadyApproved`, `InvalidState`, `MilestoneAlreadyReleased` |
| `release_milestone` | mode-dependent (see below) | `Funded` | blocked | `ContractPaused`, `UnauthorizedRole`, `InvalidState`, `InsufficientApprovals`, `MilestoneAlreadyReleased`, `AlreadyRefunded`, `InsufficientFunds` |
| `submit_work_evidence` | freelancer | `Funded` | blocked | `ContractPaused`, `UnauthorizedRole`, `InvalidState`, `MilestoneAlreadyReleased`, `AlreadyRefunded`, `EvidenceTooLong` |
| `refund_unreleased_milestones` | client | `Created`, `Funded`, or `Disputed` | blocked | `ContractPaused`, `UnauthorizedRole`, `InvalidState`, `AlreadyReleased`, `AlreadyRefunded`, `MilestoneNotOverdue` |
| `raise_dispute` | client or freelancer | `Funded` or `PartiallyFunded` | blocked | `ContractPaused`, `UnauthorizedRole`, `ArbiterRequired`, `InvalidState` |
| `resolve_dispute` | arbiter | `Disputed` | blocked | `ContractPaused`, `UnauthorizedRole`, `InvalidStatusTransition`, `InvalidDisputeSplit`, `AccountingInvariantViolated` |
| `cancel_contract` | client | `Created` or `Funded` (no released milestones) | blocked | `ContractPaused`, `UnauthorizedRole`, `AlreadyCancelled`, `InvalidStatusTransition` |
| `issue_reputation` | client | `Completed` | unblocked (read state only) | `ContractPaused`, `UnauthorizedRole`, `NotCompleted`, `ReputationAlreadyIssued`, `InvalidRating`, `EmptyComment`, `CommentTooLong`, `SelfRating` |
| `finalize_contract` | client, freelancer, or arbiter | `Completed` or `Disputed` | panics `AlreadyFinalized` | `ContractPaused`, `UnauthorizedRole`, `InvalidStatusTransition`, `AlreadyFinalized` |

---

## Release Authorization Modes

`ReleaseAuthorization` is set at `create_contract` and never changes. It
controls who may call `approve_milestone_release` and `release_milestone`.

### Summary matrix

| Mode | Enum | Who may approve | Who may release | Arbiter required at creation? |
|---|---|---|---|---|
| `ClientOnly` | 0 | client | client | no |
| `ClientAndArbiter` | 1 | client **or** arbiter (one is sufficient) | client or arbiter | **yes** |
| `ArbiterOnly` | 2 | arbiter | arbiter | **yes** |
| `MultiSig` | 3 | client **and** freelancer (both required) | client or freelancer | no |

### Approval check logic (from `approvals.rs`)

```rust
match contract.release_authorization {
    ClientOnly       => approvals.client_approved,
    ArbiterOnly      => approvals.arbiter_approved,
    ClientAndArbiter => approvals.client_approved || approvals.arbiter_approved,
    MultiSig         => approvals.client_approved && approvals.freelancer_approved,
}
```

### Release caller check logic (from `lib.rs::release_milestone`)

```rust
match contract.release_authorization {
    ClientOnly       => if !is_client    { panic UnauthorizedRole }
    ArbiterOnly      => if !is_arbiter   { panic UnauthorizedRole }
    ClientAndArbiter => if !is_client && !is_arbiter   { panic UnauthorizedRole }
    MultiSig         => if !is_client && !is_freelancer { panic UnauthorizedRole }
}
```

In `MultiSig` mode, both parties must approve, but either party may trigger
the release transaction. This separates intent (approval) from execution
(release).

---

## Approval Lifecycle

Milestone releases are a two-step operation:

### Step 1 — `approve_milestone_release(contract_id, caller, milestone_index)`

Records the caller's approval in Soroban **temporary** storage under
`DataKey::MilestoneApprovals(contract_id, milestone_index)`.

- Contract must be `Funded` or `PartiallyFunded`.
- Milestone must not already be released.
- Caller must be authorized by the release mode (see matrix above).
- Duplicate calls from the same address return `AlreadyApproved`.
- Approvals expire after **120 960 ledgers (~7 days)** and are treated as
  absent thereafter (fail-closed).

### Step 2 — `release_milestone(contract_id, caller, milestone_index)`

Executes the SAC transfer and advances milestone state.

- Contract must be `Funded`.
- Caller must be authorized to release by the release mode.
- Sufficient approvals must exist and not have expired (`InsufficientApprovals`
  on failure).
- The milestone must not be released or refunded.
- Available balance (`funded_amount − released_amount − refunded_amount`) must
  cover the milestone amount.
- Approvals are cleared after a successful release (no reuse).
- If all milestones are released or refunded, contract transitions to
  `Completed` and a pending reputation credit is granted to the freelancer.

### Approval TTL

| Constant | Ledgers | Days (~5 s/ledger) |
|---|---|---|
| `PENDING_APPROVAL_TTL_LEDGERS` | 120 960 | 7 |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 17 280 | 1 |

The TTL is reset to the full 7 days on every write. When accessed and the
remaining TTL is below the bump threshold, it is extended back to the full
value. Expired approvals cannot be used; all parties must re-approve.

---

## Per-Entrypoint Detail

### `create_contract`

```
Authorized: client (client.require_auth())
State:      none — creates a new contract in Created
```

- Client and freelancer must be distinct → `InvalidParticipant`
- Modes `ArbiterOnly` and `ClientAndArbiter` require a non-`None` arbiter →
  `MissingArbiter`
- Arbiter must differ from both client and freelancer → `InvalidArbiter`
- Milestones must be non-empty → `EmptyMilestones`
- All milestone amounts must be > 0 → `InvalidMilestoneAmount`
- Milestone count ≤ 10 → `TooManyMilestones`
- Sum of amounts ≤ governed cap (or `i128::MAX` when unset) → `TotalCapExceeded`

---

### `deposit_funds`

```
Authorized: client (caller == contract.client, then caller.require_auth())
State:      Created or PartiallyFunded
```

- Any other caller → `UnauthorizedRole`
- Cancelled contract → `ContractCancelled`
- Refunded contract → `ContractRefunded`
- Other terminal states → `InvalidState`
- Deposit that would exceed total milestone sum → `InvalidDepositAmount`
- Partial deposit → transitions to `PartiallyFunded`; full deposit → `Funded`

---

### `submit_work_evidence`

```
Authorized: freelancer (caller == contract.freelancer, then caller.require_auth())
State:      Funded
```

- Any other caller → `UnauthorizedRole`
- Contract not `Funded` → `InvalidState`
- Milestone already released → `MilestoneAlreadyReleased`
- Milestone already refunded → `AlreadyRefunded`
- Evidence string > 256 bytes → `EvidenceTooLong`
- Evidence may be overwritten before release; no write limit per milestone

---

### `approve_milestone_release`

```
Authorized: mode-dependent (see release matrix)
State:      Funded or PartiallyFunded
```

- Not a contract participant at all → `UnauthorizedRole`
- Participant but not permitted by mode → `UnauthorizedRole`
- Contract not in `Funded`/`PartiallyFunded` → `InvalidState`
- Milestone already released → `MilestoneAlreadyReleased`
- Caller already approved this milestone → `AlreadyApproved`

---

### `release_milestone`

```
Authorized: mode-dependent (see release matrix)
State:      Funded
```

- Not permitted by mode → `UnauthorizedRole`
- Contract not `Funded` → `InvalidState`
- Approvals absent or expired → `InsufficientApprovals`
- Milestone already released → `MilestoneAlreadyReleased`
- Milestone already refunded → `AlreadyRefunded`
- Insufficient contract balance → `InsufficientFunds`

The SAC transfer to the freelancer occurs **before** milestone state is
updated. A failed transfer leaves accounting untouched (fail-safe).

---

### `refund_unreleased_milestones`

```
Authorized: client (contract.client.require_auth())
State:      Created, Funded, or Disputed
```

- Caller is not `contract.client` → `UnauthorizedRole`
- Invalid state → `InvalidState`
- Empty index list → `EmptyRefundRequest`
- Duplicate indices → `DuplicateMilestoneInRefund`
- Out-of-bounds index → `IndexOutOfBounds`
- Milestone already released → `AlreadyReleased`
- Milestone already refunded → `AlreadyRefunded`
- Milestone has a deadline but is not yet overdue → `MilestoneNotOverdue`
  (milestones with no deadline may be refunded at any time)
- Insufficient balance → `InsufficientFunds`
- After all milestones are refunded → status becomes `Refunded` (no
  reputation credit). If some were released first → `Completed` with a
  reputation credit granted.

---

### `raise_dispute`

```
Authorized: client or freelancer (caller == contract.client || caller == contract.freelancer)
State:      Funded or PartiallyFunded
```

- Any other caller → `UnauthorizedRole`
- No arbiter assigned → `ArbiterRequired`
- Contract not in `Funded`/`PartiallyFunded` → `InvalidState`
- Transitions contract to `Disputed`

---

### `resolve_dispute`

```
Authorized: arbiter (arbiter == contract.arbiter, then arbiter.require_auth())
State:      Disputed
```

- Any other caller → `UnauthorizedRole`
- Contract not `Disputed` → `InvalidStatusTransition`
- Split amounts that do not conserve the available balance → `InvalidDisputeSplit`
- Accounting inconsistency → `AccountingInvariantViolated`

Resolution variants and their outcomes:

| Variant | Client payout | Freelancer payout | Final status |
|---|---|---|---|
| `FullRefund` | 100% of available | 0 | `Refunded` |
| `PartialRefund` | ~70% of available | ~30% of available | `Completed` |
| `FullPayout` | 0 | 100% of available | `Completed` |
| `Split(client_amount, freelancer_amount)` | `client_amount` | `freelancer_amount` | `Completed` or `Refunded` |

A `Refunded` final status is set only when `refunded_amount == funded_amount`
after the resolution. Otherwise the status is `Completed` and a pending
reputation credit is granted to the freelancer.

---

### `cancel_contract`

```
Authorized: client (client == contract.client, then client.require_auth())
State:      Created or Funded, with released_amount == 0
```

- Any other caller → `UnauthorizedRole`
- Already cancelled → `AlreadyCancelled`
- In any other state → `InvalidStatusTransition`
- Any milestone already released (`released_amount != 0`) → `InvalidStatusTransition`
- The full refundable balance is transferred back to the client via the SAC
  before the status is set to `Cancelled`. A zero-balance cancellation skips
  the token transfer.

---

### `issue_reputation`

```
Authorized: client (caller == contract.client, then caller.require_auth())
State:      Completed
```

- Any other caller → `UnauthorizedRole`
- Contract not `Completed` → `NotCompleted`
- Reputation already issued for this contract → `ReputationAlreadyIssued`
- Rating outside `[1, 5]` → `InvalidRating`
- Empty comment → `EmptyComment`
- Comment > 200 bytes → `CommentTooLong`
- Client and freelancer are the same address → `SelfRating`
- No pending reputation credit for the freelancer → `InvalidState`

Issuing reputation consumes one pending credit from the freelancer's credit
counter and increments their `completed_contracts` and `total_rating`. It can
be called exactly once per contract.

---

### `finalize_contract`

```
Authorized: client, freelancer, or arbiter
State:      Completed or Disputed
```

- Caller not a contract participant → `UnauthorizedRole`
- Contract not `Completed`/`Disputed` → `InvalidStatusTransition`
- Already finalized → `AlreadyFinalized`

Finalization writes an immutable `FinalizationRecord` containing a full
contract snapshot. After finalization, all contract-specific mutating calls
panic with `AlreadyFinalized`.

---

## Worked Example: Three-Milestone Contract (MultiSig Mode)

This example walks through a complete lifecycle: funding, two milestone
releases, a refund, and closure.

**Setup**

```
client    = Alice
freelancer = Bob
arbiter   = None   (MultiSig does not require an arbiter)
milestones = [100, 200, 150]  stroops
release_authorization = MultiSig
```

**Step 1 — Alice creates the contract**

```
create_contract(client=Alice, freelancer=Bob, arbiter=None,
                milestones=[100, 200, 150], release_authorization=MultiSig)
→ contract_id = 42
  status: Created
```

Alice's `require_auth()` is called. Sum = 450 stroops, within cap.

**Step 2 — Alice funds the contract**

```
deposit_funds(contract_id=42, caller=Alice, amount=450)
```

Alice is `contract.client`. Amount matches total. Status → `Funded`.

**Step 3 — Bob submits evidence for milestone 0**

```
submit_work_evidence(contract_id=42, caller=Bob, milestone_index=0, evidence="ipfs://Qm...")
```

Bob is `contract.freelancer`. Contract is `Funded`. Evidence recorded.

**Step 4 — Approvals for milestone 0 (MultiSig)**

```
approve_milestone_release(contract_id=42, caller=Alice, milestone_index=0)
  → client_approved = true  (TTL: 7 days)

approve_milestone_release(contract_id=42, caller=Bob, milestone_index=0)
  → freelancer_approved = true  (TTL refreshed to 7 days)
```

At this point: `client_approved && freelancer_approved = true` → sufficient.

**Step 5 — Alice releases milestone 0**

```
release_milestone(contract_id=42, caller=Alice, milestone_index=0)
```

- Alice is `is_client` → authorized by MultiSig mode
- Approvals check passes
- SAC transfer: 100 stroops (minus fee) → Bob
- Milestone 0 marked released; approvals cleared

**Step 6 — Bob approves milestone 1; Alice also approves**

```
approve_milestone_release(contract_id=42, caller=Bob,   milestone_index=1)
approve_milestone_release(contract_id=42, caller=Alice, milestone_index=1)
```

Both approved. Bob triggers the release:

```
release_milestone(contract_id=42, caller=Bob, milestone_index=1)
```

- Bob is `is_freelancer` → authorized by MultiSig mode
- 200 stroops (minus fee) → Bob. Milestone 1 marked released.

**Step 7 — Alice refunds milestone 2**

Work on milestone 2 was not delivered; the milestone has no deadline.

```
refund_unreleased_milestones(contract_id=42, milestone_indices=[2])
```

- `contract.client.require_auth()` called for Alice
- Milestone 2 has no deadline → refundable immediately
- 150 stroops → Alice. Milestone 2 marked refunded.
- All milestones are released or refunded → status → `Completed`
- Pending reputation credit granted to Bob

**Step 8 — Alice issues reputation**

```
issue_reputation(contract_id=42, caller=Alice, rating=4, comment="Good work on milestones 0 and 1")
```

- Alice is `contract.client` → authorized
- Status is `Completed`
- Pending credit exists for Bob → consumed; Bob's `completed_contracts` incremented

**Step 9 — Alice finalizes**

```
finalize_contract(contract_id=42, finalizer=Alice)
```

- Alice is `contract.client` → authorized
- Status is `Completed` → allowed
- `FinalizationRecord` written; contract is now immutable

---

## Rejection Reference

| Error | Code | Common trigger |
|---|---|---|
| `UnauthorizedRole` | 11 | Wrong role for the called entrypoint |
| `AlreadyApproved` | 18 | Same party approving the same milestone twice |
| `InsufficientApprovals` | 20 | Approvals absent, insufficient, or expired |
| `MissingArbiter` | 12 | `ArbiterOnly`/`ClientAndArbiter` mode without arbiter at creation |
| `InvalidArbiter` | 13 | Arbiter address equals client or freelancer |
| `InvalidParticipant` | 14 | Client equals freelancer |
| `InvalidState` | 16 | Operation called in wrong contract state |
| `InvalidStatusTransition` | 41 | State transition not permitted |
| `ContractNotFound` | 10 | Unknown contract_id |
| `IndexOutOfBounds` | 3 | Milestone index ≥ milestone count |
| `MilestoneAlreadyReleased` | 17 | Attempting to release/approve a released milestone |
| `AlreadyRefunded` | 8 | Attempting to release/refund an already-refunded milestone |
| `AlreadyFinalized` | 46 | Mutating call after `finalize_contract` |
| `AlreadyCancelled` | 50 | `cancel_contract` on an already-cancelled contract |
| `ArbiterRequired` | 42 | `raise_dispute` with no arbiter assigned |
| `ContractPaused` | 37 | Any mutating call while paused |
| `EmergencyActive` | 38 | Any mutating call during emergency |
| `ReputationAlreadyIssued` | 23 | `issue_reputation` called twice on same contract |
| `NotCompleted` | 40 | `issue_reputation` before `Completed` |
| `SelfRating` | 39 | `issue_reputation` when client == freelancer |
| `MilestoneNotOverdue` | 53 | Refund of a milestone with a future deadline |
| `InsufficientFunds` | 9 | Balance insufficient for the requested operation |
| `EvidenceTooLong` | 47 | `submit_work_evidence` string > 256 bytes |

---

## Implementation References

| Concern | Source |
|---|---|
| Role types and `ReleaseAuthorization` enum | `contracts/escrow/src/types.rs` |
| Approval record and TTL policy | `contracts/escrow/src/approvals.rs` |
| `approve_milestone_release` entrypoint | `contracts/escrow/src/lib.rs` |
| `release_milestone` entrypoint | `contracts/escrow/src/lib.rs` |
| `deposit_funds`, `cancel_contract`, `issue_reputation` | `contracts/escrow/src/lib.rs` |
| `submit_work_evidence` | `contracts/escrow/src/lib.rs` |
| `raise_dispute`, `resolve_dispute` | `contracts/escrow/src/lib.rs` |
| Deposit validation | `contracts/escrow/src/deposit.rs` |
| Finalization logic | `contracts/escrow/src/finalize.rs` |
| Dispute payout arithmetic | `contracts/escrow/src/dispute.rs` |
| TTL constants | `contracts/escrow/src/ttl.rs` |
| Error codes | `contracts/escrow/src/types.rs` |
| Release mode deep-dive | `docs/escrow/authorization.md` |
| ABI reference | `docs/escrow/abi-reference.md` |
| Security analysis | `docs/escrow/SECURITY.md` |
