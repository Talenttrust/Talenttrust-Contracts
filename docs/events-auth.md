# Events authorization and access rules

This document describes **who may publish each event**, **in which contract state**, and **which entrypoints trigger them**. It is derived from the auth checks and event emission points across the escrow contract.

All event topics use `symbol_short!` for the first element (4-character max) and indexable keys for the second element where applicable, enabling efficient off-chain filtering by contract ID, milestone index, or event type.

---

## Roles

| Role | Identity source | Can emit events via |
|------|----------------|---------------------|
| **Admin** | `DataKey::Admin` (set by `initialize`) | Governance, pause/emergency, admin rotation, protocol fees, rollback, contract finalization rollback, milestone rollback, storage migration, settlement limit, contract limits |
| **Client** | `Contract.client` (set at `create_contract`) | Contract creation, deposit, approve milestone, release milestone, refund, cancel, raise dispute, issue reputation |
| **Freelancer** | `Contract.freelancer` (set at `create_contract`) | Approve milestone (MultiSig), release milestone (MultiSig), raise dispute, submit work evidence |
| **Arbiter** | `Contract.arbiter` (optional, set at `create_contract`) | Approve milestone (ArbiterOnly/ClientAndArbiter), release milestone (ArbiterOnly/ClientAndArbiter), resolve dispute |
| **Any participant** | Client, freelancer, or arbiter | Finalize contract, client migration (propose/accept/cancel) |

---

## Shared gates

Every mutating entrypoint that emits an event runs these checks first:

| Order | Check | Rejection |
|-------|-------|-----------|
| 1 | `require_initialized` — `DataKey::Initialized` is true | `NotInitialized` |
| 2 | `require_not_paused` — neither pause nor emergency is active | `ContractPaused` or `EmergencyActive` |
| 3 | Caller `require_auth()` | Soroban auth failure (no contract error code) |

Then per-contract entrypoints additionally load `DataKey::Contract(contract_id)` and run:

| Check | Rejection |
|-------|-----------|
| Contract storage present | `ContractNotFound` |
| `require_not_finalized(contract_id)` — no finalization record | `AlreadyFinalized` |

---

## Event inventory

### Lifecycle events

#### `("created", contract_id)`

| Entrypoint | Auth | Required status | Transition |
|------------|------|----------------|------------|
| `create_contract` | `client.require_auth()` | (none — new contract) | → `Created` |

**Payload:** `(client: Address, freelancer: Address, timestamp: u64)`

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Pause/emergency active | `ContractPaused` / `EmergencyActive` |
| Client == freelancer | `InvalidParticipant` |
| Arbiter required by mode but missing | `MissingArbiter` |
| Arbiter == client or freelancer | `InvalidArbiter` |
| Milestones empty | `EmptyMilestones` |
| Milestone amounts invalid | `InvalidMilestoneAmount` |
| Total cap exceeded | `TotalCapExceeded` |
| Too many milestones | `TooManyMilestones` |

---

#### `("contract", contract_id)` — indexed contract snapshot

Emitted by `emit_contract_indexed_event` after every state-changing lifecycle operation.

**Payload:** `(status: u32, funded_amount: i128, released_amount: i128, refunded_amount: i128, total_deposited: i128)`

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `create_contract` | `client.require_auth()` | (new) |
| `deposit_funds` | `contract.client.require_auth()` | `Created` / `PartiallyFunded` |
| `release_milestone` | Per `ReleaseAuthorization` mode | `Funded` |
| `refund_unreleased_milestones` | `contract.client.require_auth()` | `Created` / `Funded` / `Disputed` |
| `cancel_contract` | `contract.client.require_auth()` | `Created` / `Funded` |
| `raise_dispute` | Client or freelancer | `Funded` / `PartiallyFunded` |
| `resolve_dispute` | `contract.arbiter.require_auth()` | `Disputed` |
| `finalize_contract` | Any participant | `Completed` / `Disputed` |

---

#### `("deposit", contract_id)`

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `deposit_funds` | `caller.require_auth()` where caller == client | `Created` / `PartiallyFunded` |

**Payload:** `(deposit_amount: i128, caller: Address, timestamp: u64)`

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Amount ≤ 0 | `AmountMustBePositive` |
| Status not `Created` or `PartiallyFunded` | `InvalidState` |
| Caller not client | `UnauthorizedRole` |
| Deposit would exceed total milestone amount | `InvalidDepositAmount` |
| Settlement token not bound | `SettlementTokenNotConfigured` |

---

### Milestone events

#### `("mlstn_idx", contract_id, milestone_index)` — per-milestone indexed event

Emitted by both `release_milestone` and `refund_unreleased_milestones` for each affected milestone.

**Payload:** `(amount: i128, released: bool, refunded: bool, timestamp: u64)`

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `release_milestone` | Per `ReleaseAuthorization` mode | `Funded` |
| `refund_unreleased_milestones` | `contract.client.require_auth()` | `Created` / `Funded` / `Disputed` |

---

#### `("mlstn_rls", contract_id)` — milestone release

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `release_milestone` | Per `ReleaseAuthorization` mode | `Funded` |

**Payload:** `(milestone_index: u32, gross_amount: i128, protocol_fee: i128, new_released_amount: i128, caller: Address, timestamp: u64)`

**Rejection matrix (release_milestone):**

| Condition | Error |
|-----------|-------|
| Status not `Funded` | `InvalidState` |
| Caller not authorized by release mode | `UnauthorizedRole` |
| Milestone already released | `AlreadyReleased` |
| Milestone already refunded | `AlreadyRefunded` |
| Insufficient approvals (mode-specific) | `InsufficientApprovals` |
| Insufficient balance | `InsufficientFunds` |
| Milestone index out of bounds | `IndexOutOfBounds` |

---

#### `("ctrct_cmp", contract_id)` — contract completed

Emitted conditionally by `release_milestone` when all milestones are released.

**Payload:** `(caller: Address, timestamp: u64)`

Same auth and state requirements as `release_milestone`.

---

#### `("approve", contract_id)` — milestone approval

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `approve_milestone_release_batch` | Per `ReleaseAuthorization` mode | `Funded` / `PartiallyFunded` |

**Payload:** `(caller: Address, milestone_index: u32, timestamp: u64)`

Emitted per approved milestone in the batch.

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Status not `Funded` or `PartiallyFunded` | `InvalidState` |
| Caller not authorized by release mode | `UnauthorizedRole` |
| Milestone already released | `AlreadyReleased` |
| Caller already approved this milestone | `AlreadyApproved` |

---

#### `("refunded", contract_id)` — contract refunded

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `refund_unreleased_milestones` | `contract.client.require_auth()` | `Created` / `Funded` / `Disputed` |

**Payload:** `(total_refund_amount: i128, new_status: ContractStatus, timestamp: u64)`

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Status not `Created`, `Funded`, or `Disputed` | `InvalidState` |
| Caller not client | `UnauthorizedRole` |
| Empty refund request | `EmptyRefundRequest` |
| Duplicate milestone indices | `DuplicateMilestoneInRefund` |
| Milestone already released | `AlreadyReleased` |
| Milestone already refunded | `AlreadyRefunded` |
| Insufficient balance | `InsufficientFunds` |

---

#### `("cancelled", contract_id)` — contract cancelled

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `cancel_contract` | `contract.client.require_auth()` | `Created` / `Funded` |

**Payload:** `(client: Address, refund_amount: i128, timestamp: u64)`

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Status not `Created` or `Funded` | `InvalidStatusTransition` |
| Caller not client | `UnauthorizedRole` |
| Released amount > 0 | `InvalidStatusTransition` |
| Already cancelled | `ContractCancelled` |

---

### Dispute events

#### `("dispute", "opened")` — dispute opened

**Payload:** `(contract_id: u32, caller: Address)`

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `raise_dispute` | Client or freelancer | `Funded` / `PartiallyFunded` |

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Status not `Funded` or `PartiallyFunded` | `InvalidState` |
| Caller not client or freelancer | `UnauthorizedRole` |
| Arbiter not assigned (`contract.arbiter` is `None`) | `ArbiterRequired` |

---

#### `("dispute", "resolved")` — dispute resolved

**Payload:** `(contract_id: u32, resolution_code: u32)`

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `resolve_dispute` | `contract.arbiter.require_auth()` | `Disputed` |

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Status not `Disputed` | `InvalidStatusTransition` |
| Caller not assigned arbiter | `UnauthorizedRole` |
| Invalid split amounts | `InvalidDisputeSplit` |
| Accounting invariant violated | `AccountingInvariantViolated` |

---

### Finalization events

#### `("finalized", contract_id)` — contract finalized

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `finalize_contract` | Any participant (`caller.require_auth()` where caller is client, freelancer, or arbiter) | `Completed` / `Disputed` |

**Payload:** `(finalizer: Address, timestamp: u64)`

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Status not `Completed` or `Disputed` | `InvalidStatusTransition` |
| Caller not client, freelancer, or arbiter | `UnauthorizedRole` |
| Already finalized | `AlreadyFinalized` |

---

#### `("rollback", contract_id)` — rollback

Emitted by three different rollback operations with different auth rules.

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `rollback_dispute` | Admin only (`admin.require_auth()`) | `Disputed` (rollback record exists) |
| `rollback_contract` | Admin only | Finalized, status `Completed` or `Disputed` |
| `rollback_milestone` | Admin only | `Funded` or `PartiallyFunded` |

**Payload (varies by caller):**
- `rollback_dispute`: `(admin: Address, from_status: Disputed, to_status: ContractStatus, timestamp: u64)`
- `rollback_contract`: `(admin: Address, status: ContractStatus, timestamp: u64)`
- `rollback_milestone`: `(milestone_index: u32, admin: Address, timestamp: u64)`

---

### Evidence and reputation events

#### `("evidence", contract_id)` — work evidence submitted

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `submit_work_evidence` | `contract.freelancer.require_auth()` | `Funded` |

**Payload:** `(milestone_index: u32, freelancer: Address, timestamp: u64)`

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Status not `Funded` | `InvalidState` |
| Caller not freelancer | `UnauthorizedRole` |
| Milestone already released or refunded | `AlreadyReleased` / `AlreadyRefunded` |

---

#### `("repr_put", contract_id)` — reputation issued

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `issue_reputation` | `contract.client.require_auth()` | `Completed` |

**Payload:** `(freelancer: Address, rating: u32, timestamp: u64)`

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Status not `Completed` | `NotCompleted` |
| Caller not client | `UnauthorizedRole` |
| Rating out of range (1–5) | `InvalidRating` |
| Self-rating (client == freelancer) | `SelfRating` |
| Reputation already issued | `ReputationAlreadyIssued` |
| Comment empty | `EmptyComment` |
| Comment too long (>200 bytes) | `CommentTooLong` |

---

### Governance events

#### `("init", Symbol("admin_set"))` — initialization

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `initialize` | `admin.require_auth()` | (none — one-time) |

**Payload:** `(admin: Address, timestamp: u64)`

Rejected with `AlreadyInitialized` if called again.

---

#### `("sttl_bind",)` — settlement token bound

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `bind_settlement_token` | `admin.require_auth()` | Initialized |

**Payload:** `(admin: Address, token: Address, timestamp: u64)`

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Not initialized | `NotInitialized` |
| Pause/emergency active | `ContractPaused` / `EmergencyActive` |
| Caller not admin | `UnauthorizedRole` |
| Token already bound | `SettlementTokenAlreadyBound` |
| Token is escrow contract address | `SettlementTokenIsSelf` |
| Token is admin address | `SettlementTokenIsAdmin` |
| Token not a valid SAC | `InvalidSettlementToken` |

---

#### `("protocol_fee_bps",)` — protocol fee changed

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `set_protocol_fee_bps` | Admin only | Initialized |

**Payload:** `(old_bps: u32, new_bps: u32, admin: Address, timestamp: u64)`

---

#### `("events_limit",)` — events storage limit changed

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `set_events_limit` | Admin only | Initialized |

**Payload:** `(old_limit: u32, new_limit: u32, admin: Address, timestamp: u64)`

---

#### `("settlement_limit",)` — settlement limit changed

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `set_settlement_limit` | Admin only | Initialized |

**Payload:** `(old_limit: i128, new_limit: i128, admin: Address, timestamp: u64)`

---

#### Admin rotation events

| Event topic | Entrypoint | Auth | Payload |
|-------------|------------|------|---------|
| `("admin", Symbol("proposed"))` | `propose_governance_admin` | Admin | `(admin: Address, proposed: Address, timestamp: u64)` |
| `("admin", Symbol("accepted"))` | `accept_governance_admin` | Proposed admin | `(old_admin: Address, new_admin: Address, timestamp: u64)` |
| `("admin", Symbol("cancelled"))` | `cancel_governance_admin_proposal` | Admin | `(admin: Address, cancelled_proposal: Address, timestamp: u64)` |

---

#### Contract limits events (admin only)

| Event topic | Entrypoint | Payload |
|-------------|------------|---------|
| `("limits", Symbol("max_milestones"))` | `set_max_milestones` | `(max_milestones: u32, timestamp: u64)` |
| `("limits", Symbol("max_escrow"))` | `set_max_escrow_stroops` | `(max_escrow_stroops: i128, timestamp: u64)` |

---

#### `("arbiter", contract_id)` — arbiter changed

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `set_arbiter` | Admin only | Initialized |

**Payload:** `(old_arbiter: Option<Address>, new_arbiter: Option<Address>, timestamp: u64)`

---

### Pause and emergency events

| Event topic | Entrypoint | Auth |
|-------------|------------|------|
| `("pause", timestamp: u64)` | `pause` | Admin |
| `("unpaused", timestamp: u64)` | `unpause` | Admin |
| `("emergency", Symbol("activated"))` | `activate_emergency_pause` | Admin |
| `("emergency", Symbol("resolved"))` | `resolve_emergency` | Admin |

All pause/emergency events carry `(admin: Address, timestamp: u64)` payload.

---

### Storage migration event

#### `(Symbol("state_migrated"), version: u32)` — storage version migrated

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `migrate_state` | Admin only | Initialized |

**Payload:** `(admin: Address, timestamp: u64)`

---

### Fee events

#### `("fee", Symbol("withdraw"))` — protocol fee withdrawal

| Entrypoint | Auth | Required status |
|------------|------|----------------|
| `withdraw_protocol_fees` | Admin only | Initialized |

**Payload:** `(admin: Address, to: Address, amount: i128, timestamp: u64)`

**Rejection matrix:**

| Condition | Error |
|-----------|-------|
| Amount ≤ 0 | `AmountMustBePositive` |
| Amount > accumulated fees | `InsufficientAccumulatedFees` |

---

### Client migration events

| Event topic | Entrypoint | Auth | Required status | Payload |
|-------------|------------|------|-----------------|---------|
| `(Symbol("client_migration_proposed"), contract_id)` | `propose_client_migration` | Current client | Not completed, cancelled, refunded, or disputed | `(current_client: Address, new_client: Address, requested_at: u32)` |
| `(Symbol("client_migration_accepted"), contract_id)` | `accept_client_migration` | Proposed new client | Pending migration exists | `(old_client: Address, new_client: Address, timestamp: u64)` |
| `(Symbol("client_migration_cancelled"), contract_id)` | `cancel_client_migration` | Current client | Pending migration exists | `(current_client: Address, timestamp: u64)` |

---

## ReleaseAuthorization mode matrix

The `ReleaseAuthorization` enum controls who may approve and release milestones.
This directly governs which events can be emitted by `approve_milestone_release_batch`
and `release_milestone`.

| Mode | Who may approve | Who may release | Approval threshold |
|------|----------------|-----------------|-------------------|
| `ClientOnly` | Client | Client | Client alone |
| `ArbiterOnly` | Arbiter | Arbiter | Arbiter alone (arbiter required at creation) |
| `ClientAndArbiter` | Client or arbiter | Client or arbiter | Either client or arbiter (arbiter required at creation) |
| `MultiSig` | Client and freelancer | Client or freelancer | Both client and freelancer must approve; either may execute release |

---

## Event dependency graph

```
create_contract
  ├── ("created", id)
  └── ("contract", id)

deposit_funds
  ├── ("deposit", id)
  └── ("contract", id)

approve_milestone_release_batch
  └── ("approve", id)  [per milestone]

release_milestone  (status: Funded → Completed when last milestone)
  ├── ("mlstn_idx", id, idx)
  ├── ("mlstn_rls", id)
  ├── ("ctrct_cmp", id)  [conditional]
  └── ("contract", id)

refund_unreleased_milestones
  ├── ("mlstn_idx", id, idx)  [per milestone]
  ├── ("refunded", id)
  └── ("contract", id)

cancel_contract
  ├── ("cancelled", id)
  └── ("contract", id)

raise_dispute
  ├── ("dispute", "opened")
  └── ("contract", id)

resolve_dispute
  ├── ("dispute", "resolved")
  └── ("contract", id)

finalize_contract
  ├── ("finalized", id)
  └── ("contract", id)

submit_work_evidence
  └── ("evidence", id)

issue_reputation
  └── ("repr_put", id)
```

---

## Worked example: full lifecycle event sequence

Scenario: client `C` creates contract 42 with freelancer `F`, arbiter `A`,
`ReleaseAuthorization::ClientAndArbiter`, two milestones (300 + 200).

### Step 1 — Create

```
Entrypoint:  create_contract
Auth:        C.require_auth()
Events:
  ("created", 42)              → (C, F, ts1)
  ("contract", 42)             → (Created(0), 0, 0, 0, 0)

Rejected alternatives:
  create_contract called by F  → Soroban auth failure
  create_contract with arbiter=None in ClientAndArbiter mode → MissingArbiter
```

### Step 2 — Deposit (full amount: 500)

```
Entrypoint:  deposit_funds(contract_id=42, caller=C, amount=500)
Auth:        C.require_auth()  (must match contract.client)
Events:
  ("deposit", 42)              → (500, C, ts2)
  ("contract", 42)             → (Funded(2), 500, 0, 0, 500)

Rejected alternatives:
  deposit_funds by F           → UnauthorizedRole
  deposit while paused         → ContractPaused
  deposit on finalized         → AlreadyFinalized
```

### Step 3 — Approve milestone 0 (arbiter approves)

```
Entrypoint:  approve_milestone_release_batch(contract_id=42, caller=A, milestone_indices=[0])
Auth:        A.require_auth(), mode ClientAndArbiter → arbiter allowed
Events:
  ("approve", 42)              → (A, 0, ts3)

Rejected alternatives:
  approve by F (not allowed in ClientAndArbiter) → UnauthorizedRole
  approve already-released milestone  → AlreadyReleased
  approve already-approved milestone by same caller → AlreadyApproved
```

### Step 4 — Release milestone 0 (client releases)

```
Entrypoint:  release_milestone(contract_id=42, caller=C, milestone_index=0)
Auth:        C.require_auth(), mode ClientAndArbiter → client allowed
Checks:      milestone not released, check_approvals → arbiter_approved=true, status=Funded
Events:
  ("mlstn_idx", 42, 0)         → (300, true, false, ts4)
  ("mlstn_rls", 42)            → (0, 300, fee, 300, C, ts4)
  ("contract", 42)             → (Funded(2), 500, 300, 0, 500)

Rejected alternatives:
  release by non-participant   → UnauthorizedRole
  release without approval (ClientAndArbiter requires client or arbiter approval) → InsufficientApprovals
  release with insufficient balance → InsufficientFunds
```

### Step 5 — Approve and release milestone 1

```
Entrypoint:  approve_milestone_release_batch(contract_id=42, caller=C, milestone_indices=[1])
Events:      ("approve", 42) → (C, 1, ts5)

Entrypoint:  release_milestone(contract_id=42, caller=C, milestone_index=1)
Events:
  ("mlstn_idx", 42, 1)         → (200, true, false, ts6)
  ("mlstn_rls", 42)            → (1, 200, fee, 500, C, ts6)
  ("ctrct_cmp", 42)            → (C, ts6)   [all milestones released]
  ("contract", 42)             → (Completed(3), 500, 500, 0, 500)
```

### Step 6 — Issue reputation

```
Entrypoint:  issue_reputation(contract_id=42, caller=C, freelancer=F, rating=5, comment="Great work")
Auth:        C.require_auth()
Events:
  ("repr_put", 42)             → (F, 5, ts7)

Rejected alternatives:
  issue_reputation before Completed → NotCompleted
  issue_reputation by freelancer    → UnauthorizedRole
  double issuance                  → ReputationAlreadyIssued
```

### Step 7 — Finalize

```
Entrypoint:  finalize_contract(contract_id=42, finalizer=C)
Auth:        C.require_auth()  (any participant allowed)
Events:
  ("finalized", 42)            → (C, ts8)
  ("contract", 42)             → (Completed(3), 500, 500, 0, 500)

Rejected alternatives:
  finalize by non-participant  → UnauthorizedRole
  finalize when not Completed or Disputed → InvalidStatusTransition
  finalize when already finalized → AlreadyFinalized
```

---

## Dispute lifecycle example

Scenario: same contract, after deposit (status = Funded).

### Raise dispute

```
Entrypoint:  raise_dispute(contract_id=42, caller=F)
Auth:        F.require_auth()  (client or freelancer)
Events:
  ("dispute", "opened")        → (42, F)
  ("contract", 42)             → (Disputed(4), 500, 0, 0, 500)

Rejected alternatives:
  raise_dispute by arbiter     → UnauthorizedRole
  raise_dispute with no arbiter assigned → ArbiterRequired
```

### Resolve dispute

```
Entrypoint:  resolve_dispute(contract_id=42, arbiter=A, resolution=FullPayout)
Auth:        A.require_auth()  (must match contract.arbiter)
Events:
  ("dispute", "resolved")      → (42, resolution_code)
  ("contract", 42)             → (Completed(3), 500, 500, 0, 500)

Rejected alternatives:
  resolve_dispute by client    → UnauthorizedRole
  resolve_dispute on non-disputed → InvalidStatusTransition
```

---

## Admin-only event summary

These events are emitted by entrypoints that require `admin.require_auth()`:

| Event | Entrypoint |
|-------|------------|
| `("init", Symbol("admin_set"))` | `initialize` |
| `("sttl_bind",)` | `bind_settlement_token` |
| `("protocol_fee_bps",)` | `set_protocol_fee_bps` |
| `("events_limit",)` | `set_events_limit` |
| `("settlement_limit",)` | `set_settlement_limit` |
| `("admin", Symbol("proposed"))` | `propose_governance_admin` |
| `("admin", Symbol("cancelled"))` | `cancel_governance_admin_proposal` |
| `("limits", Symbol("max_milestones"))` | `set_max_milestones` |
| `("limits", Symbol("max_escrow"))` | `set_max_escrow_stroops` |
| `("arbiter", contract_id)` | `set_arbiter` |
| `("pause", timestamp)` | `pause` |
| `("unpaused", timestamp)` | `unpause` |
| `("emergency", Symbol("activated"))` | `activate_emergency_pause` |
| `("emergency", Symbol("resolved"))` | `resolve_emergency` |
| `(Symbol("state_migrated"), version)` | `migrate_state` |
| `("fee", Symbol("withdraw"))` | `withdraw_protocol_fees` |
| `("rollback", contract_id)` | `rollback_dispute`, `rollback_contract`, `rollback_milestone` |

---

## Cross-reference: entrypoint → source location

| Entrypoint | Source location | Event emission |
|------------|----------------|----------------|
| `initialize` | `lib.rs:554` | `lib.rs:582` |
| `bind_settlement_token` | `lib.rs:388` | `lib.rs:439` |
| `create_contract` | `create_contract.rs:56` | `create_contract.rs:154`; `create_contract.rs:160` |
| `deposit_funds` | `lib.rs:732` | `deposit.rs:140`; `deposit.rs:136` |
| `approve_milestone_release_batch` | `lib.rs:1340` | `lib.rs:1359` |
| `release_milestone` | `lib.rs:1600` | `milestones.rs:243,256`; `lib.rs:1616,1652,1669,1686` |
| `refund_unreleased_milestones` | `lib.rs:2015` | `refund_impl.rs:125`; `refund_impl.rs:147`; `lib.rs:2041,2070,2079` |
| `cancel_contract` | `lib.rs:2870` | `refund.rs:257`; `lib.rs:2903,2906` |
| `raise_dispute` | `lib.rs:3945` | `dispute.rs:351`; `lib.rs:3963,3967` |
| `resolve_dispute` | `lib.rs:4045` | `dispute.rs:415`; `lib.rs:4064,4068` |
| `finalize_contract` | `lib.rs:841` | `finalize.rs:168,173` |
| `rollback_dispute` | `lib.rs:1012` | `rollback.rs:91` |
| `rollback_contract` | `lib.rs:1054` | `finalize.rs:225` |
| `rollback_milestone` | `lib.rs:1812` | `lib.rs:1834` |
| `submit_work_evidence` | `lib.rs:3575` | `lib.rs:3598` |
| `issue_reputation` | `lib.rs:3100` | `lib.rs:3136` |
| `set_protocol_fee_bps` | `governance.rs:50` | `governance.rs:75` |
| `set_events_limit` | `governance.rs:125` | `governance.rs:147` |
| `propose_governance_admin` | `governance.rs:165` | `governance.rs:186` |
| `accept_governance_admin` | `governance.rs:205` | `governance.rs:228` |
| `cancel_governance_admin_proposal` | `governance.rs:245` | `governance.rs:267` |
| `set_settlement_limit` | `governance.rs:410` | `governance.rs:433` |
| `set_max_milestones` | `contracts.rs:495` | `contracts.rs:511` |
| `set_max_escrow_stroops` | `contracts.rs:525` | `contracts.rs:541` |
| `set_arbiter` | `contracts.rs:460` | `contracts.rs:484` |
| `pause` | `lib.rs:2590` | `lib.rs:2608` |
| `unpause` | `lib.rs:2635` | `lib.rs:2652` |
| `activate_emergency_pause` | `lib.rs:2710` | `lib.rs:2737` |
| `resolve_emergency` | `lib.rs:2775` | `lib.rs:2798` |
| `migrate_state` | `lib.rs:1228` | `lib.rs:1255` |
| `withdraw_protocol_fees` | `lib.rs:3760` | `lib.rs:3787` |
| `propose_client_migration` | `lib.rs:1085` | `migration.rs:71` |
| `accept_client_migration` | `lib.rs:1119` | `migration.rs:104` |
| `cancel_client_migration` | `lib.rs:1150` | `migration.rs:125` |

---

## Quick reference: event → who may trigger

| Event topic | Client | Freelancer | Arbiter | Admin | Any participant |
|-------------|--------|------------|---------|-------|-----------------|
| `("init", ...)` | | | | ✓ | |
| `("sttl_bind",)` | | | | ✓ | |
| `("created", id)` | ✓ | | | | |
| `("contract", id)` | ✓ | ✓ | ✓ | | ✓ (finalize) |
| `("deposit", id)` | ✓ | | | | |
| `("approve", id)` | ✓ | ✓ (MultiSig) | ✓ (ArbiterOnly/ClientAndArbiter) | | |
| `("mlstn_idx", id, idx)` | ✓ | ✓ (MultiSig) | ✓ (ArbiterOnly/ClientAndArbiter) | | |
| `("mlstn_rls", id)` | ✓ | ✓ (MultiSig) | ✓ (ArbiterOnly/ClientAndArbiter) | | |
| `("ctrct_cmp", id)` | ✓ | ✓ (MultiSig) | ✓ (ArbiterOnly/ClientAndArbiter) | | |
| `("refunded", id)` | ✓ | | | | |
| `("cancelled", id)` | ✓ | | | | |
| `("dispute", "opened")` | ✓ | ✓ | | | |
| `("dispute", "resolved")` | | | ✓ | | |
| `("finalized", id)` | | | | | ✓ |
| `("rollback", id)` | | | | ✓ | |
| `("evidence", id)` | | ✓ | | | |
| `("repr_put", id)` | ✓ | | | | |
| `("arbiter", id)` | | | | ✓ | |
| `("limits", ...)` | | | | ✓ | |
| `("protocol_fee_bps",)` | | | | ✓ | |
| `("events_limit",)` | | | | ✓ | |
| `("settlement_limit",)` | | | | ✓ | |
| `("admin", ...)` | | | | ✓ | |
| `("pause", ...)` | | | | ✓ | |
| `("unpaused", ...)` | | | | ✓ | |
| `("emergency", ...)` | | | | ✓ | |
| `(Symbol("state_migrated"), ...)` | | | | ✓ | |
| `("fee", ...)` | | | | ✓ | |
| `(Symbol("client_migration_*"), id)` | ✓ | | | ✓ (proposed) | |

Note: Client and freelancer column for milestone events depends on
`ReleaseAuthorization` mode. See the mode matrix above for details.

---

## Auth check order (reference)

### Lifecycle entrypoints

```
deposit_funds:
  1. require_initialized
  2. require_not_paused
  3. require_not_finalized
  4. validate_deposit (caller == client, status Created|PartiallyFunded)
  5. token.transfer
  6. apply_validated_deposit → emit ("deposit", id) + ("contract", id)

release_milestone:
  1. require_initialized
  2. require_not_paused
  3. require_not_finalized
  4. Load contract → ContractNotFound
  5. Status == Funded → else InvalidState
  6. caller.require_auth()
  7. require_release_authorization → else UnauthorizedRole
  8. Milestone bounds + not released/refunded
  9. check_approvals (mode-specific) → else InsufficientApprovals
  10. Balance check → InsufficientFunds
  11. token.transfer
  12. Update state + emit events

refund_unreleased_milestones:
  1. require_initialized
  2. require_not_paused
  3. require_not_finalized
  4. Load contract → ContractNotFound
  5. contract.client.require_auth()
  6. Status ∈ {Created, Funded, Disputed} → else InvalidState
  7. Validate indices + milestone states
  8. token.transfer
  9. Update state + emit events

cancel_contract:
  1. require_initialized
  2. require_not_paused
  3. require_not_finalized
  4. Load contract → ContractNotFound
  5. Status ∈ {Created, Funded} → else InvalidStatusTransition
  6. released_amount == 0 → else InvalidStatusTransition
  7. client.require_auth()
  8. token.transfer
  9. Update state + emit events
```

### Dispute entrypoints (derived from `disputes-auth.md`)

```
raise_dispute:
  1. require_initialized
  2. require_not_paused
  3. caller.require_auth()
  4. Load contract → ContractNotFound
  5. TTL bump + require_not_finalized
  6. Role: client OR freelancer → else UnauthorizedRole
  7. Arbiter present → else ArbiterRequired
  8. Status ∈ {Funded, PartiallyFunded} → else InvalidState
  9. Write Disputed + emit opened event

resolve_dispute:
  1. require_initialized
  2. require_not_paused
  3. arbiter.require_auth()
  4. Load contract → ContractNotFound
  5. TTL bump + require_not_finalized
  6. Status == Disputed → else InvalidStatusTransition
  7. caller == contract.arbiter → else UnauthorizedRole
  8. resolution_payouts → typed math errors
  9. Update accounting, final status, emit resolved event
```

---

## Error code reference

| Code | Name | Relevant entrypoints |
|------|------|---------------------|
| 11 | `UnauthorizedRole` | All entrypoints when caller lacks required role |
| 14 | `NotInitialized` | `raise_dispute`, `resolve_dispute`, `bind_settlement_token`, all governance |
| 16 | `InvalidState` | `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `raise_dispute`, `resolve_dispute` |
| 24 | `InvalidStatusTransition` | `resolve_dispute`, `finalize_contract`, `cancel_contract` |
| 29 | `AlreadyFinalized` | All mutating entrypoints after finalization |
| 37 | `ContractPaused` | All mutating entrypoints when paused |
| 38 | `EmergencyActive` | All mutating entrypoints in emergency |
| 10 | `ContractNotFound` | Any per-contract entrypoint with unknown contract ID |
| 9 | `InsufficientFunds` | `release_milestone`, `refund_unreleased_milestones` |
| 4 | `AlreadyReleased` | `release_milestone` on released milestone; `refund_unreleased_milestones` on released |
| 8 | `AlreadyRefunded` | `release_milestone` on refunded milestone; `refund_unreleased_milestones` on refunded |
| 20 | `InsufficientApprovals` | `release_milestone` when mode requires approval |
| 25 | `ArbiterRequired` | `raise_dispute` when no arbiter assigned |
| 26 | `InvalidDisputeSplit` | `resolve_dispute` with non-conserving split |
| 27 | `AccountingInvariantViolated` | `resolve_dispute` when math violates invariants |
| 42 | `ArbiterRequired` | `create_contract` for modes requiring arbiter |
| 43 | `InvalidDisputeSplit` | `resolve_dispute` |
| 44 | `AccountingInvariantViolated` | `resolve_dispute` |

---

## Related documentation

- [`docs/disputes-auth.md`](disputes-auth.md) — Detailed dispute authorization rules
- [`docs/settlement-auth.md`](settlement-auth.md) — Settlement and release authorization rules
- [`docs/arbiter-auth.md`](arbiter-auth.md) — Arbiter role authorization rules
- [`docs/milestones-auth.md`](milestones-auth.md) — Milestone-level authorization rules
- [`docs/reputation-auth.md`](reputation-auth.md) — Reputation authorization rules
- [`docs/escrow/abi-reference.md`](escrow/abi-reference.md) — Public ABI signatures
- [`docs/escrow/indexer-schema.md`](escrow/indexer-schema.md) — Indexer event schema
- [`contracts/escrow/src/events.rs`](../contracts/escrow/src/events.rs) — Event helper source
- [`contracts/escrow/src/authorization.rs`](../contracts/escrow/src/authorization.rs) — Shared auth helpers