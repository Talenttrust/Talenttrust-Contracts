# Settlement Authorization Rules

This document defines who may call what, in which contract state, for every
settlement-relevant entrypoint in the TalentTrust Escrow contract.

## Roles

| Role | Identity source | Governs |
|------|----------------|---------|
| **Admin** | `DataKey::Admin` (set by `initialize`) | Pause/emergency, protocol fees, governance admin rotation |
| **Client** | `Contract.client` (set at `create_contract`) | Deposits, cancellations, refunds, approval/release in `ClientOnly`/`ClientAndArbiter`/`MultiSig` modes |
| **Freelancer** | `Contract.freelancer` (set at `create_contract`) | Receives payouts; approval/release in `MultiSig` mode only |
| **Arbiter** | `Contract.arbiter` (optional, set at `create_contract`) | Approval/release in `ArbiterOnly`/`ClientAndArbiter` modes; dispute resolution |

## Contract Lifecycle States

```
Created ──deposit──▶ PartiallyFunded ──deposit──▶ Funded
  │                     │                       │
  │ cancel              │ cancel                │ release_all ──▶ Completed
  │                     │                       │ refund_all  ──▶ Refunded
  │                     │                       │ raise_dispute ──▶ Disputed
  │                     │                       │
  └─────────────────────┴───────────────────────┘
                                              │
                                        resolve_dispute
                                              │
                                    ┌─────────┴──────────┐
                                    ▼                     ▼
                                Completed              Refunded
```

Terminal states (`Completed`, `Refunded`, `Cancelled`) and `Finalized` contracts
reject all settlement operations with `AlreadyFinalized` or `InvalidState`.

## Settlement Entrypoints

### `release_milestone(env, contract_id, caller, milestone_index) → bool`

Transfers the net milestone amount (gross minus protocol fee) to the freelancer
via the bound settlement token.

| Guard | Condition | Error |
|-------|-----------|-------|
| Pause/emergency | `require_not_paused` | `ContractPaused` / `EmergencyActive` |
| Finalization | `require_not_finalized` | `AlreadyFinalized` |
| Contract exists | `DataKey::Contract(id)` present | `ContractNotFound` |
| State | `contract.status == Funded` | `InvalidState` |
| Caller auth | `caller.require_auth()` | Soroban auth failure |
| Role | Per `ReleaseAuthorization` mode (see matrix below) | `UnauthorizedRole` |
| Milestone bounds | `milestone_index < milestones.len()` | `IndexOutOfBounds` |
| Milestone state | `!milestone.released && !milestone.refunded` | `MilestoneAlreadyReleased` / `AlreadyRefunded` |
| Approvals | `approvals::check_approvals` passes | `InsufficientApprovals` |
| Balance | `available_balance >= gross_amount` | `InsufficientFunds` |

**Approval clearing**: approvals are cleared from temporary storage after a
successful release.

### `approve_milestone_release(env, contract_id, caller, milestone_index) → bool`

Records the caller's approval for a milestone in temporary storage (TTL 7 days,
bump threshold 1 day).

| Guard | Condition | Error |
|-------|-----------|-------|
| Pause/emergency | `require_not_paused` | `ContractPaused` / `EmergencyActive` |
| Finalization | `require_not_finalized` | `AlreadyFinalized` |
| Contract exists | `DataKey::Contract(id)` present | `ContractNotFound` |
| State | `Funded` or `PartiallyFunded` | `InvalidState` |
| Caller auth | `caller.require_auth()` | Soroban auth failure |
| Role | Per `ReleaseAuthorization` mode | `UnauthorizedRole` |
| Milestone bounds | `milestone_index < milestones.len()` | `IndexOutOfBounds` |
| Milestone state | `!milestone.released` | `MilestoneAlreadyReleased` |
| Duplicate | Caller has not already approved | `AlreadyApproved` |

### `refund_unreleased_milestones(env, contract_id, milestone_indices) → i128`

Refunds specified unreleased milestones back to the client.

| Guard | Condition | Error |
|-------|-----------|-------|
| Pause/emergency | `require_not_paused` | `ContractPaused` / `EmergencyActive` |
| Finalization | `require_not_finalized` | `AlreadyFinalized` |
| Contract exists | `DataKey::Contract(id)` present | `ContractNotFound` |
| State | `Created`, `Funded`, or `Disputed` | `InvalidState` |
| Caller auth | `contract.client.require_auth()` | Soroban auth failure |
| Non-empty | `milestone_indices.len() > 0` | `EmptyRefundRequest` |
| No duplicates | All indices unique | `DuplicateMilestoneInRefund` |
| Milestone bounds | Each index valid | `IndexOutOfBounds` |
| Milestone state | Not released and not already refunded | `AlreadyReleased` / `AlreadyRefunded` |
| Deadline | If set, milestone must be overdue | `MilestoneNotOverdue` |
| Balance | `available_balance >= total_refund_amount` | `InsufficientFunds` |

**Only the client** may call this entrypoint. No other role is permitted.

### `raise_dispute(env, contract_id, caller) → bool`

Transitions a funded contract to `Disputed`, blocking further releases until
resolution.

| Guard | Condition | Error |
|-------|-----------|-------|
| Initialized | `require_initialized` | `NotInitialized` |
| Pause/emergency | `require_not_paused` | `ContractPaused` / `EmergencyActive` |
| Caller auth | `caller.require_auth()` | Soroban auth failure |
| Contract exists | `DataKey::Contract(id)` present | `ContractNotFound` |
| Finalization | `require_not_finalized` | `AlreadyFinalized` |
| Role | `caller == client || caller == freelancer` | `UnauthorizedRole` |
| Arbiter assigned | `contract.arbiter.is_some()` | `ArbiterRequired` |
| State | `Funded` or `PartiallyFunded` | `InvalidState` |

### `resolve_dispute(env, contract_id, arbiter, resolution) → bool`

Applies an arbiter-selected dispute resolution and transfers funds accordingly.

| Guard | Condition | Error |
|-------|-----------|-------|
| Initialized | `require_initialized` | `NotInitialized` |
| Pause/emergency | `require_not_paused` | `ContractPaused` / `EmergencyActive` |
| Caller auth | `arbiter.require_auth()` | Soroban auth failure |
| Contract exists | `DataKey::Contract(id)` present | `ContractNotFound` |
| Finalization | `require_not_finalized` | `AlreadyFinalized` |
| State | `contract.status == Disputed` | `InvalidStatusTransition` |
| Role | `caller == contract.arbiter` | `UnauthorizedRole` |
| Split validity | Split amounts conserve available balance | `InvalidDisputeSplit` |

### `cancel_contract(env, contract_id, client) → bool`

Cancels a contract and refunds the full balance to the client.

| Guard | Condition | Error |
|-------|-----------|-------|
| Pause/emergency | `require_not_paused` | `ContractPaused` / `EmergencyActive` |
| Contract exists | `DataKey::Contract(id)` present | `ContractNotFound` |
| Finalization | `require_not_finalized` | `AlreadyFinalized` |
| State | `Created` or `Funded` | `InvalidStatusTransition` |
| No releases | `contract.released_amount == 0` | `InvalidStatusTransition` |
| Caller auth | `caller.require_auth()` | Soroban auth failure |
| Role | `caller == contract.client` | `UnauthorizedRole` |
| Not already cancelled | `contract.status != Cancelled` | `AlreadyCancelled` |

### `finalize_contract(env, contract_id, finalizer) → bool`

Writes an immutable finalization record. Settlement operations are then blocked.

| Guard | Condition | Error |
|-------|-----------|-------|
| Pause/emergency | `require_not_paused` | `ContractPaused` / `EmergencyActive` |
| Caller auth | `finalizer.require_auth()` | Soroban auth failure |
| Contract exists | `DataKey::Contract(id)` present | `ContractNotFound` |
| Finalization | `require_not_finalized` | `AlreadyFinalized` |
| Role | `finalizer == client \|\| finalizer == freelancer \|\| finalizer == arbiter` | `UnauthorizedRole` |
| State | `Completed` or `Disputed` | `InvalidStatusTransition` |

## ReleaseAuthorization Matrix

The `ReleaseAuthorization` enum (defined in `types.rs`) controls who may approve
and who may release each milestone. The four variants are:

### ClientOnly (`ReleaseAuthorization::ClientOnly`)

| Operation | Who may act | Logic |
|-----------|-------------|-------|
| Approve | Client only | `client_approved` |
| Release | Client only | — |

### ArbiterOnly (`ReleaseAuthorization::ArbiterOnly`)

| Operation | Who may act | Logic |
|-----------|-------------|-------|
| Approve | Arbiter only | `arbiter_approved` |
| Release | Arbiter only | — |

**Requires** an arbiter address at contract creation (`MissingArbiter` if absent).

### ClientAndArbiter (`ReleaseAuthorization::ClientAndArbiter`)

| Operation | Who may act | Logic |
|-----------|-------------|-------|
| Approve | Client OR arbiter | `client_approved \|\| arbiter_approved` |
| Release | Client OR arbiter | OR logic for approvals and release |

**Requires** an arbiter address at contract creation.

### MultiSig (`ReleaseAuthorization::MultiSig`)

| Operation | Who may act | Logic |
|-----------|-------------|-------|
| Approve | Client AND freelancer | `client_approved && freelancer_approved` |
| Release | Client OR freelancer | After both have approved |

**Arbiter cannot** approve or release in MultiSig mode.

## Worked Example: ClientOnly Mode

```
Setup:
  - Client: CA... (0x1111)
  - Freelancer: FL... (0x2222)
  - Arbiter: None
  - ReleaseAuthorization: ClientOnly
  - Milestone 0: 5,000,000 stroops
  - Milestone 1: 3,000,000 stroops
  - Total funded: 8,000,000 stroops

Step 1 — Client approves milestone 0
  Caller: CA... (client)
  Entrypoint: approve_milestone_release(contract_id=42, caller=CA..., index=0)
  Check: ClientOnly → caller == client ✓
  Result: client_approved = true for milestone 0

Step 2 — Client releases milestone 0
  Caller: CA... (client)
  Entrypoint: release_milestone(contract_id=42, caller=CA..., index=0)
  Checks:
    ✓ not paused
    ✓ not finalized
    ✓ status == Funded
    ✓ caller == client (ClientOnly)
    ✓ milestone 0 not released, not refunded
    ✓ approvals::check_approvals → client_approved == true ✓
    ✓ available_balance (8,000,000) >= gross_amount (5,000,000) ✓
  Side effects:
    - 5,000,000 stroops transferred to FL... (minus fee)
    - released_amount += net_amount
    - milestone 0.released = true
    - Approval record cleared

Step 3 — Client approves milestone 1
  (Same as Step 1, index=1)

Step 4 — Client releases milestone 1
  (Same as Step 2, index=1)
  After release: all milestones released → contract.status = Completed
```

## Worked Example: MultiSig Mode (Approval + Release Separation)

```
Setup:
  - Client: CA... (0x1111)
  - Freelancer: FL... (0x2222)
  - Arbiter: None
  - ReleaseAuthorization: MultiSig
  - Milestone 0: 5,000,000 stroops

Step 1 — Client approves milestone 0
  Caller: CA...
  Result: client_approved = true
  check_approvals: false (freelancer not yet approved)

Step 2 — Freelancer approves milestone 0
  Caller: FL...
  Result: freelancer_approved = true
  check_approvals: true (both flags set)

Step 3a — Client releases milestone 0 (authorized in MultiSig)
  Caller: CA...
  Auth check: client is allowed ✓
  Result: release succeeds

Step 3b — Freelancer could also have released (either party may release)
  Caller: FL...
  Auth check: freelancer is allowed ✓
  Result: same release outcome

Step 4 — A stranger (0x9999) attempting release
  Auth check: not client, not freelancer → UnauthorizedRole
```

## Worked Example: Refund Flow

```
Setup:
  - Contract in Funded state, 2 milestones (5,000,000 + 3,000,000)
  - Milestone 0 released, milestone 1 not released
  - Available balance: 3,000,000 stroops

Caller: Client (CA...)
Entrypoint: refund_unreleased_milestones(contract_id=42, indices=[1])

Checks:
  ✓ not paused, not finalized
  ✓ status == Funded → refundable
  ✓ caller == client
  ✓ milestone 1 not released, not refunded
  ✓ available_balance (3,000,000) >= refund_amount (3,000,000) ✓

Result:
  - 3,000,000 stroops transferred back to client
  - milestone 1.refunded = true
  - refunded_amount += 3,000,000
  - Status stays Funded (milestone 0 still released, 1 now refunded = Completed)
```

## Worked Example: Dispute Resolution

```
Setup:
  - Contract in Disputed state (after raise_dispute)
  - Contract has an arbiter assigned
  - Available balance: 8,000,000 stroops

Caller: Arbiter (AB...), resolves with FullPayout
Entrypoint: resolve_dispute(contract_id=42, arbiter=AB..., resolution=FullPayout)

Checks:
  ✓ initialized
  ✓ not paused, not finalized
  ✓ caller == contract.arbiter (AB...) ✓
  ✓ status == Disputed ✓
  ✓ resolution_payouts: freelancer gets 8,000,000, client gets 0

Result:
  - released_amount += 8,000,000
  - status → Completed (non-zero freelancer payout)
  - Reputation credit granted to freelancer
```

## Cross-Reference: Entrypoint → Source Locations

| Entrypoint | Source location | Auth module |
|------------|----------------|-------------|
| `release_milestone` | `lib.rs:690` | Inline `match contract.release_authorization` in lib.rs |
| `approve_milestone_release` | `lib.rs:606` | Delegates to `approvals::approve_milestone` |
| `refund_unreleased_milestones` | `lib.rs:1018` | `contract.client.require_auth()` only |
| `raise_dispute` | `lib.rs:2184` | `caller == client \|\| caller == freelancer` |
| `resolve_dispute` | `lib.rs:2263` | `caller == contract.arbiter` |
| `cancel_contract` | `lib.rs:1593` | `caller == contract.client` |
| `finalize_contract` | `lib.rs:531` (entrypoint), `finalize.rs:140` (impl) | `require_finalizer_role` helper |
| `approve_milestone` (internal) | `approvals.rs:26` | `match contract.release_authorization` |
| `check_approvals` (internal) | `approvals.rs:115` | Per-mode boolean logic |

## Error Code Reference

| Code | Name | Raised by settlement entrypoints |
|------|------|----------------------------------|
| 11 | `UnauthorizedRole` | All entrypoints when caller lacks the required role |
| 16 | `InvalidState` | `release_milestone`, `refund_unreleased_milestones`, `resolve_dispute`, `finalize_contract`, `cancel_contract` when contract is not in a compatible state |
| 46 | `AlreadyFinalized` | All settlement entrypoints after finalization |
| 41 | `InvalidStatusTransition` | `resolve_dispute`, `finalize_contract`, `cancel_contract` for disallowed transitions |
| 20 | `InsufficientApprovals` | `release_milestone` when approvals are missing or expired |
| 17 | `MilestoneAlreadyReleased` | `release_milestone` on an already-released milestone |
| 8 | `AlreadyRefunded` | `release_milestone` on a refunded milestone |
| 4 | `AlreadyReleased` | `refund_unreleased_milestones` on an already-released milestone |
| 9 | `InsufficientFunds` | `release_milestone` or `refund_unreleased_milestones` when balance is inadequate |
| 53 | `MilestoneNotOverdue` | `refund_unreleased_milestones` when a deadline-set milestone is not yet overdue |
| 42 | `ArbiterRequired` | `raise_dispute` when no arbiter is assigned |
| 43 | `InvalidDisputeSplit` | `resolve_dispute` when split amounts do not conserve |
| 44 | `AccountingInvariantViolated` | `resolve_dispute` when accounting state is inconsistent |
| 3 | `IndexOutOfBounds` | Milestone index exceeds milestones vector length |
| 10 | `ContractNotFound` | Contract ID not found in storage |
| 6 | `EmptyRefundRequest` | `refund_unreleased_milestones` with empty indices |
| 7 | `DuplicateMilestoneInRefund` | `refund_unreleased_milestones` with duplicate indices |
| 37 | `ContractPaused` | Any settlement entrypoint when pause flag is set |
| 38 | `EmergencyActive` | Any settlement entrypoint when emergency flag is set |
| 36 | `NotInitialized` | `raise_dispute`, `resolve_dispute` before `initialize` |
| 50 | `AlreadyCancelled` | `cancel_contract` on an already-cancelled contract |

## Pause and Emergency Overrides

All settlement entrypoints (except `get_contract`, `get_milestones`, and other
read-only operations) are gated by `require_not_paused`. When the pause flag or
emergency flag is set, every settlement write operation panics with
`ContractPaused` or `EmergencyActive` respectively, regardless of the caller's
role or any approvals on record.

Only the Admin role (via `load_and_auth_admin`) can clear these flags through
`unpause()` and `resolve_emergency()`.

## Finalization Blocks All Settlement

Once a contract is finalized (via `finalize_contract`), all settlement entrypoints
that mutate state (`release_milestone`, `approve_milestone_release`,
`refund_unreleased_milestones`, `cancel_contract`, `resolve_dispute`,
`raise_dispute`) reject with `AlreadyFinalized`. Read-only queries remain
available.
