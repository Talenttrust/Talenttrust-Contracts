# Milestones Data Model and Invariants

**Scope:** `contracts/escrow/src/`  
**Cross-check source:** `types.rs`, `create_contract.rs`, `release.rs`, `refund_impl.rs`, `approvals.rs`, `amount_validation.rs`, `deposit.rs`, `lib.rs`, `finalize.rs`

This document describes the **milestones data model** in the TalentTrust escrow contract:
how milestones are represented in storage, the invariants that govern them, the
entrypoints that create, read, mutate, or finalize milestone state, and a worked
example tracing a milestone through its lifecycle.

---

## Table of Contents

1. [Data Structures](#1-data-structures)
2. [Storage Layout](#2-storage-layout)
3. [Constants](#3-constants)
4. [Invariants](#4-invariants)
5. [Lifecycle and Status Transitions](#5-lifecycle-and-status-transitions)
6. [Entrypoints That Touch Milestones](#6-entrypoints-that-touch-milestones)
   - [Creation](#61-creation-create_contract)
   - [Funding / Deposit](#62-funding--deposit-deposit_funds)
   - [Approval](#63-approval-approve_milestone_release)
   - [Release](#64-release-release_milestone)
   - [Refund](#65-refund-refund_unreleased_milestones)
   - [Read Queries](#66-read-queries)
   - [Finalization](#67-finalization-finalize_contract)
7. [Worked Example](#7-worked-example)
8. [Security Considerations](#8-security-considerations)

---

## 1. Data Structures

### 1.1 `Milestone` (per-milestone record)

Defined in [`types.rs`](../../contracts/escrow/src/types.rs):

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    /// The agreed-upon payout amount for this milestone, in stroops.
    pub amount: i128,
    /// The amount of funds actually allocated/transferred to this milestone
    /// via per-milestone deposit or aggregate deposit accounting.
    pub funded_amount: i128,
    /// Whether this milestone has been released (paid out to the freelancer).
    pub released: bool,
    /// Whether this milestone has been refunded back to the client.
    pub refunded: bool,
    /// Optional work-evidence string (e.g. a URL to deliverables).
    pub work_evidence: Option<String>,
    /// The actual amount refunded (set to `amount` on refund for full-milestone refunds).
    pub refunded_amount: i128,
    /// Optional Unix timestamp (seconds) after which the client may claim
    /// a timeout refund for this milestone without arbiter involvement.
    /// `None` means the milestone never expires.
    pub deadline: Option<u64>,
}
```

**Field semantics:**

| Field | Type | Default (at creation) | Description |
|-------|------|-----------------------|-------------|
| `amount` | `i128` | User-supplied | Contracted payout value; **immutable** after creation. |
| `funded_amount` | `i128` | `0` | Tracked per-milestone funded amount. Updated by deposit and release flows. |
| `released` | `bool` | `false` | Becomes `true` exactly once, when the milestone is paid out. |
| `refunded` | `bool` | `false` | Becomes `true` exactly once, when the milestone is refunded. |
| `work_evidence` | `Option<String>` | `None` | Optional evidence string set by the freelancer. |
| `refunded_amount` | `i128` | `0` | Set to the milestone's `amount` upon refund. |
| `deadline` | `Option<u64>` | `None` | Optional timeout deadline. When set and past, the client can claim a timeout refund. |

**Key constraint:** `released` and `refunded` are mutually exclusive — a milestone
cannot be both released and refunded. Both being `false` means the milestone is
**pending** (unreleased and unrefunded).

### 1.2 `MilestoneSummary` (indexer-friendly view)

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneSummary {
    pub index: u32,
    pub amount: i128,
    pub released: bool,
    pub refunded: bool,
}
```

A lightweight subset of `Milestone` used in `ContractSummary` for off-chain
indexers. Omits `funded_amount`, `work_evidence`, `refunded_amount`, and
`deadline`.

### 1.3 `ContractSummary` (contract-level summary including milestones)

```rust
pub struct ContractSummary {
    pub schema_version: u32,
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub status: ContractStatus,
    pub reputation_issued: bool,
    pub total_amount: i128,          // sum of milestone amounts
    pub funded_amount: i128,         // total deposited
    pub released_amount: i128,       // net amount paid out (after fees)
    pub refundable_balance: i128,    // funded - released - refunded
    pub released_milestone_count: u32,
    pub milestones: Vec<MilestoneSummary>,
}
```

**Computed fields:**

- `total_amount`: Sum of all `milestone.amount` values, computed on read via
  `accumulate_amounts`.
- `released_milestone_count`: Count of milestones where `released == true`.
- `refundable_balance`: `funded_amount - released_amount - refunded_amount`.

---

## 2. Storage Layout

### 2.1 Milestone vector key

```
(DataKey::Contract(contract_id), Symbol("milestones"))  →  Vec<Milestone>
```

- **Storage class:** Persistent (Soroban `Env::storage().persistent()`).
- **TTL:** Renewed on every read or write via `ttl::extend_milestone_ttl` or
  `ttl::extend_contract_and_milestones_ttl`.
- **Eviction risk:** If a milestone vector is never accessed for more than the
  Soroban persistent-entries TTL, it is evicted and the contract becomes
  unrecoverable. All milestone-touching operations extend TTL.

### 2.2 Approval records (temporary)

```
DataKey::MilestoneApprovals(contract_id, milestone_index)  →  MilestoneApprovals
```

- **Storage class:** Temporary (Soroban `Env::storage().temporary()`).
- **TTL:** `PENDING_APPROVAL_TTL_LEDGERS` (~7 days), renewed on approval and read.
- **Contents:** Tracks which parties have approved a given milestone.
  ```rust
  pub struct MilestoneApprovals {
      pub client_approved: bool,
      pub freelancer_approved: bool,
      pub arbiter_approved: bool,
  }
  ```

### 2.3 Contract-level accounting fields

Stored under `DataKey::Contract(contract_id)` → `Contract`:

```rust
pub struct Contract {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub status: ContractStatus,
    pub total_deposited: i128,
    pub funded_amount: i128,     // total funds deposited
    pub released_amount: i128,   // net paid out (after protocol fee deduction)
    pub refunded_amount: i128,   // total refunded to client
    pub release_authorization: ReleaseAuthorization,
    pub reputation_issued: bool,
}
```

The contract-level `funded_amount`, `released_amount`, and `refunded_amount` are
**aggregate running totals** that must be consistent with the per-milestone flags
(see [Invariants](#4-invariants)).

---

## 3. Constants

Defined in `lib.rs` and `amount_validation.rs`:

| Constant | Value | Description |
|----------|-------|-------------|
| `MAX_MILESTONES` | `10` | Hard cap on milestone count per contract. |
| `MAX_SINGLE_AMOUNT_STROOPS` | `1_000_000_0000000` | Max stroops per milestone amount (1M tokens). |
| `MAX_TOTAL_ESCROW_STROOPS` | `MAX_SINGLE_AMOUNT_STROOPS` | Max total stroops across all milestones. |
| `MIN_POSITIVE_AMOUNT` | `1` | Minimum positive amount in stroops. |

---

## 4. Invariants

### 4.1 Per-Milestone Invariants

| # | Invariant | Enforcement |
|---|-----------|-------------|
| **M1** | `amount > 0` for every milestone. | `create_contract` validates via `validate_milestone_amounts`. |
| **M2** | `released` and `refunded` are never both `true` on the same milestone. | `release_milestone` panics with `AlreadyRefunded` if `refunded == true`; `refund_unreleased_milestones` panics with `AlreadyReleased` if `released == true`. |
| **M3** | A milestone can transition from `released=false, refunded=false` to either `released=true` or `refunded=true`, but never back. | Immutable once set; no code path clears either flag. |
| **M4** | `funded_amount <= amount` for each milestone. | Per-milestone `funded_amount` tracks what has been allocated; release enforces sufficient aggregate balance; deposit caps at milestone total. |

### 4.2 Contract-Level Accounting Invariant

| # | Invariant | Enforcement |
|---|-----------|-------------|
| **C1** | `sum(milestone.amount)` = `total_amount` (computed on read). | Sum computed via `accumulate_amounts` with overflow check. |
| **C2** | `funded_amount - released_amount - refunded_amount >= 0` (available balance). | Checked before release (`InsufficientFunds`) and before refund (`InsufficientFunds`). |
| **C3** | `released_amount + refunded_amount + accumulated_fees <= funded_amount`. | Checked after release via `AccountingInvariantViolated`. |
| **C4** | `released_milestone_count` equals the count of milestones where `released == true`. | Computed on read in `get_contract_summary`. |

### 4.3 Lifecycle Invariants

| # | Invariant | Enforcement |
|---|-----------|-------------|
| **L1** | Milestones can only be released while `status == Funded`. | `release_milestone` panics with `InvalidState` otherwise. |
| **L2** | Milestones can only be refunded while `status ∈ {Created, Funded, Disputed}`. | `refund_unreleased_milestones` panics with `InvalidState` otherwise. |
| **L3** | Milestones can only be approved while `status ∈ {Funded, PartiallyFunded}`. | `approve_milestone` returns `InvalidState` otherwise. |
| **L4** | `status == Completed` iff all milestones are either `released` or `refunded`. | Set atomically in `release_milestone` and `refund_unreleased_milestones`. |
| **L5** | `status == Refunded` iff all milestones are `refunded` (none released). | Set atomically in `refund_unreleased_milestones`. |
| **L6** | When `status == Completed` and at least one milestone was released (not all refunded), a pending reputation credit is granted. | Done in both `release_milestone` and `refund_unreleased_milestones`. |

### 4.4 Milestone Count Invariants

| # | Invariant | Enforcement |
|---|-----------|-------------|
| **N1** | Milestone count ≥ 1. | `create_contract` panics with `EmptyMilestones`. |
| **N2** | Milestone count ≤ `MAX_MILESTONES` (10). | `create_contract` panics with `TooManyMilestones`. |

### 4.5 Approval Invariants

| # | Invariant | Enforcement |
|---|-----------|-------------|
| **A1** | A milestone must have sufficient approvals before it can be released. | `check_approvals` returns `InsufficientApprovals` otherwise. |
| **A2** | Each party can approve a given milestone at most once. | `approve_milestone` returns `AlreadyApproved` on duplicate. |
| **A3** | Approvals auto-expire after `PENDING_APPROVAL_TTL_LEDGERS`. | Soroban temporary storage TTL; expired approvals read as `None`. |
| **A4** | Approvals are cleared after a successful release. | `clear_approvals` called at the end of release. |

---

## 5. Lifecycle and Status Transitions

### 5.1 Per-Milestone State Machine

```
                    ┌──────────┐
                    │ Pending  │
                    │(pending) │
                    └────┬─────┘
                        │
              ┌─────────┼─────────┐
              │                   │
         [release]           [refund]
              │                   │
              ▼                   ▼
        ┌──────────┐       ┌──────────┐
        │ Released │       │ Refunded │
        │ (payable)│       │  (done)  │
        └──────────┘       └──────────┘
```

- **Pending:** `released = false, refunded = false`
- **Released:** `released = true, refunded = false`
- **Refunded:** `released = false, refunded = true`

No other states exist. The `released = true, refunded = true` combination is
forbidden.

### 5.2 Contract-Level Status Transitions (driven by milestone states)

```
Created
   │ (deposit_funds)
   ▼
Funded
   ├── (all milestones released or refunded) ──► Completed
   │     ├── (all refunded, none released) ──► Refunded
   │     └── (mixed: some released, some refunded) ──► Completed
   │
   ├── (partial refund, still some pending) ──► Funded  (no status change)
   │
   └── (all milestones released) ──► Completed
```

---

## 6. Entrypoints That Touch Milestones

### 6.1 Creation: `create_contract`

**File:** [`create_contract.rs`](../../contracts/escrow/src/create_contract.rs)

**Signature:** `create_contract(env, client, freelancer, arbiter, milestones: Vec<i128>, release_authorization) -> u32`

**What it does to milestones:**

1. Validates `milestones.len() >= 1` (panic: `EmptyMilestones`).
2. Validates `milestones.len() <= MAX_MILESTONES` (panic: `TooManyMilestones`).
3. Copies amounts into a native array and calls `validate_milestone_amounts` to
   check each amount is positive and the sum does not exceed the governed cap.
4. Builds a `Vec<Milestone>` from the amounts:
   ```rust
   Milestone {
       amount,
       funded_amount: 0,
       released: false,
       refunded: false,
       work_evidence: None,
       refunded_amount: 0,
       deadline: None,
   }
   ```
5. Persists the vector under `(DataKey::Contract(id), Symbol("milestones"))`.

**Storage writes:** `DataKey::Contract(id)`, `(DataKey::Contract(id), "milestones")`, `DataKey::NextContractId`.

### 6.2 Funding / Deposit: `deposit_funds`

**File:** [`lib.rs`](../../contracts/escrow/src/lib.rs) (inline), [`deposit.rs`](../../contracts/escrow/src/deposit.rs)

**Signature:** `deposit_funds(env, contract_id, caller, amount) -> bool`

**What it does to milestones:**

1. Validates `amount > 0`, contract exists, caller is the client, contract is in `Created` status.
2. Reads the milestone vector to compute total milestone sum and remaining capacity.
3. Transfers tokens from client to escrow via SAC `transfer`.
4. Updates `contract.funded_amount += amount`.
5. Transitions status to `Funded` once `funded_amount >= total_milestone_sum`.

**Per-milestone effect:** No per-milestone flags are mutated; only the aggregate
`funded_amount` is updated.

### 6.3 Approval: `approve_milestone_release`

**File:** [`lib.rs`](../../contracts/escrow/src/lib.rs) → [`approvals.rs`](../../contracts/escrow/src/approvals.rs)

**Signature:** `approve_milestone_release(env, contract_id, caller, milestone_index) -> bool`

**What it does to milestones:**

1. Requires contract status ∈ `{Funded, PartiallyFunded}`.
2. Validates `milestone_index < milestones.len()`.
3. Validates the milestone is not already released.
4. Checks caller is authorized per `ReleaseAuthorization` mode.
5. Records the caller's approval in temporary storage under
   `DataKey::MilestoneApprovals(contract_id, milestone_index)`.
6. Rejects duplicate approvals from the same party (`AlreadyApproved`).

**Per-milestone effect:** No direct mutation of the milestone record. An approval
record is stored separately in temporary storage.

### 6.4 Release: `release_milestone`

**File:** [`lib.rs`](../../contracts/escrow/src/lib.rs)

**Signature:** `release_milestone(env, contract_id, caller, milestone_index) -> bool`

**What it does to milestones:**

1. Requires contract status == `Funded`.
2. Requires caller is authorized per `ReleaseAuthorization` mode.
3. Validates `milestone_index < milestones.len()` (panic: `IndexOutOfBounds`).
4. Validates `milestone.released == false` (panic: `MilestoneAlreadyReleased`).
5. Validates `milestone.refunded == false` (panic: `AlreadyRefunded`).
6. Calls `check_approvals` to verify sufficient approvals exist and are unexpired.
7. Checks `available_balance >= milestone.amount` (panic: `InsufficientFunds`).
8. Computes protocol fee, transfers net amount to freelancer via SAC.
9. Sets `milestone.released = true` and `milestone.funded_amount = gross_amount`.
10. Updates `contract.released_amount += net_amount`.
11. Clears approval records via `clear_approvals`.
12. If all milestones are released or refunded, transitions status to `Completed`
    and grants a pending reputation credit.

**Per-milestone effect:** `released → true`, `funded_amount → amount`.

### 6.5 Refund: `refund_unreleased_milestones`

**File:** [`lib.rs`](../../contracts/escrow/src/lib.rs) (inline)

**Signature:** `refund_unreleased_milestones(env, contract_id, milestone_indices: Vec<u32>) -> i128`

**What it does to milestones:**

1. Validates `!milestone_indices.is_empty()` (panic: `EmptyRefundRequest`).
2. Checks no duplicate indices (panic: `DuplicateMilestoneInRefund`).
3. Requires contract status ∈ `{Created, Funded, Disputed}` (panic: `InvalidState`).
4. Requires caller is the client (`client.require_auth()`).
5. Loads milestone vector and validates each requested index:
   - Index in bounds (panic: `IndexOutOfBounds`).
   - `milestone.released == false` (panic: `AlreadyReleased`).
   - `milestone.refunded == false` (panic: `AlreadyRefunded`).
   - If `milestone.deadline` is `Some`, checks the milestone is overdue
     (`is_milestone_overdue`); panics with `MilestoneNotOverdue` otherwise.
6. Checks aggregate available balance.
7. Transfers total refund amount to client via SAC.
8. For each refunded index: sets `milestone.refunded = true`,
   `milestone.refunded_amount = milestone.amount`.
9. Updates `contract.refunded_amount += total_refund_amount`.
10. Updates contract status:
    - All milestones refunded → `Refunded`
    - All milestones released or refunded (mixed) → `Completed` (+ reputation credit)
    - Otherwise → stays `Funded`.

**Per-milestone effect:** `refunded → true`, `refunded_amount → amount`.

### 6.6 Read Queries

| Entrypoint | File | Returns | Milestone Effect |
|-----------|------|---------|-----------------|
| `get_milestones` | `lib.rs` | `Vec<Milestone>` | Returns all milestone records; extends TTL. |
| `get_milestone` | `lib.rs` | `Option<Milestone>` | Returns single milestone by index; extends TTL. |
| `get_contract_summary` | `lib.rs` | `ContractSummary` | Computes `total_amount`, `released_milestone_count`, builds `Vec<MilestoneSummary>`; extends TTL. |
| `get_milestone_approvals` | `lib.rs` | `Option<MilestoneApprovals>` | Returns approval status for a milestone; extends TTL. |
| `get_approval_deadline` | `lib.rs` | `Option<u32>` | Returns ledgers remaining until approval expiry. |
| `is_milestone_overdue` | `lib.rs` | `bool` | Checks if milestone deadline (if any) has passed and milestone is unreleased. |
| `contract_exists` | `lib.rs` | `bool` | Existence probe; does not extend TTL. |
| `get_refundable_balance` | `lib.rs` | `i128` | Returns `funded - released - refunded`; extends TTL. |

### 6.7 Finalization: `finalize_contract`

**File:** [`finalize.rs`](../../contracts/escrow/src/finalize.rs)

**Signature:** `finalize_contract(env, contract_id, finalizer) -> bool`

**What it does to milestones:**

1. Requires status ∈ `{Completed, Disputed}` (panic: `InvalidStatusTransition`).
2. Reads the milestone vector and builds `Vec<MilestoneSummary>` with
   `released_milestone_count`.
3. Writes an immutable `FinalizationRecord` containing the `ContractSummary`
   (including milestone summaries).

**Per-milestone effect:** None. Milestone state is snapshot into the finalization
record. After finalization, all mutating milestone operations are blocked
(`AlreadyFinalized`).

---

## 7. Worked Example

Tracing a 3-milestone contract (200 / 300 / 500 stroops) through a full lifecycle.

### 7.1 Creation

```rust
let milestones = vec![&env, 200_i128, 300_i128, 500_i128];
let id = client.create_contract(&client_addr, &freelancer_addr, &milestones);
```

**Storage after:**
```
DataKey::Contract(1) → Contract {
    status: Created,
    funded_amount: 0,
    released_amount: 0,
    refunded_amount: 0,
    ...
}

(DataKey::Contract(1), "milestones") → Vec<Milestone>[
    { amount: 200, funded_amount: 0, released: false, refunded: false, ... },
    { amount: 300, funded_amount: 0, released: false, refunded: false, ... },
    { amount: 500, funded_amount: 0, released: false, refunded: false, ... },
]
```

### 7.2 Full Deposit

```rust
client.deposit_funds(&id, &1000); // total milestone sum
```

**Storage after:**
```
Contract.status = Funded
Contract.funded_amount = 1000
```

### 7.3 Approve and Release Milestone 0

```rust
client.approve_milestone_release(&id, &0);
client.release_milestone(&id, &0);
```

**Validation flow:**
1. `status == Funded` ✓
2. `milestone_index (0) < 3` ✓
3. `milestone[0].released == false` ✓
4. `milestone[0].refunded == false` ✓
5. Approvals checked ✓
6. `available_balance (1000 - 0 - 0) >= 200` ✓
7. Protocol fee: `200 * fee_bps / 10000`, net transferred to freelancer.

**Storage after:**
```
milestones[0] = { amount: 200, funded_amount: 200, released: true, refunded: false }
Contract.released_amount += net_amount (e.g. 200 - fee)
```

### 7.4 Refund Milestone 1 (partial refund)

```rust
client.refund_unreleased_milestones(&id, &vec![&env, 1_u32]);
```

**Validation flow:**
1. `milestone_indices = [1]` non-empty ✓
2. No duplicates ✓
3. `status == Funded` ✓
4. Caller is client ✓
5. `milestones[1].released == false` ✓
6. `milestones[1].refunded == false` ✓
7. `available_balance (1000 - released - 0) >= 300` ✓
8. Transfer 300 to client.

**Storage after:**
```
milestones[1] = { amount: 300, funded_amount: 0, released: false, refunded: true, refunded_amount: 300 }
Contract.refunded_amount += 300
Contract.status = Funded  // still has milestone[2] pending
```

### 7.5 Approve and Release Milestone 2

```rust
client.approve_milestone_release(&id, &2);
client.release_milestone(&id, &2);
```

**After release:**
```
milestones[2] = { amount: 500, released: true, refunded: false }
// All milestones are either released or refunded:
// [0]=released, [1]=refunded, [2]=released
Contract.status = Completed
PendingReputationCredit granted for freelancer
```

### 7.6 Finalization

```rust
client.finalize_contract(&id, &client_addr);
```

**Finalization record contains:**
```
ContractSummary {
    total_amount: 1000,
    funded_amount: 1000,
    released_amount: <net amount>,
    refundable_balance: 0,
    status: Completed,
    released_milestone_count: 2,
    milestones: [
        MilestoneSummary { index: 0, amount: 200, released: true, refunded: false },
        MilestoneSummary { index: 1, amount: 300, released: false, refunded: true },
        MilestoneSummary { index: 2, amount: 500, released: true, refunded: false },
    ],
    ...
}
```

---

## 8. Security Considerations

### 8.1 Bug Bar / Threat Model

- **Double-spend:** A milestone cannot be released twice (`MilestoneAlreadyReleased`)
  or released then refunded (mutual exclusion enforced by pre-flight checks).
- **Double-refund:** A refunded milestone cannot be refunded again
  (`AlreadyRefunded`). Duplicate indices in a single request are rejected
  (`DuplicateMilestoneInRefund`).
- **Insufficient balance:** Both `release_milestone` and
  `refund_unreleased_milestones` check `available_balance` before any token
  transfer.
- **Authorization:** Only the client can initiate refunds. Release authorization
  depends on the contract's `ReleaseAuthorization` mode.
- **Approval expiry:** Missing or expired approvals fail closed
  (`InsufficientApprovals`).
- **Deadline bypass:** If a milestone has a `deadline`, a refund requires the
  deadline to have passed. Non-deadline milestones can be refunded at any time.
- **Finalization freeze:** After finalization, all milestone state is immutable.
  Attempted mutations panic with `AlreadyFinalized`.
- **TTL exhaustion:** All milestone-touching operations extend storage TTL.
  A contract whose milestones are never accessed eventually becomes
  unrecoverable.

### 8.2 Overflow Protection

- Milestone amounts use `i128` with `checked_add`/`checked_sub` via
  `safe_add_amounts` / `safe_subtract_amounts`.
- The sum of milestone amounts at creation is validated via
  `validate_milestone_amounts` which uses checked arithmetic.
- Deposit amount validation uses `checked_add` to prevent overflow.

### 8.3 Accounting Invariant Enforcement

After every state-mutating milestone operation (release, refund), the contract
enforces:

```
released_amount + refunded_amount + accumulated_fees <= funded_amount
```

Violations cause a panic with `AccountingInvariantViolated`.

---

## References

- [`types.rs`](../../contracts/escrow/src/types.rs) — `Milestone`, `MilestoneSummary`, `Contract`, `ContractStatus`, `MilestoneApprovals`
- [`create_contract.rs`](../../contracts/escrow/src/create_contract.rs) — Milestone creation validation
- [`release.rs`](../../contracts/escrow/src/release.rs) — Milestone release logic
- [`refund_impl.rs`](../../contracts/escrow/src/refund_impl.rs) — Milestone refund logic
- [`approvals.rs`](../../contracts/escrow/src/approvals.rs) — Milestone approval storage and checks
- [`amount_validation.rs`](../../contracts/escrow/src/amount_validation.rs) — Amount validation helpers
- [`deposit.rs`](../../contracts/escrow/src/deposit.rs) — Deposit validation
- [`finalize.rs`](../../contracts/escrow/src/finalize.rs) — Finalization and milestone snapshot
- [`lib.rs`](../../contracts/escrow/src/lib.rs) — Root entrypoints
- [`ttl.rs`](../../contracts/escrow/src/ttl.rs) — TTL management helpers
- [`docs/escrow/REFUND_IMPLEMENTATION.md`](./REFUND_IMPLEMENTATION.md) — Refund design doc
- [`docs/escrow/milestone-validation.md`](./milestone-validation.md) — Milestone approval flow
- [`docs/escrow/milestone_schedule.md`](./milestone_schedule.md) — Milestone schedule feature
- [`docs/escrow/FUNDING_ACCOUNTING.md`](./FUNDING_ACCOUNTING.md) — Funding invariants
- [`docs/escrow/sac-custody.md`](./sac-custody.md) — Custody model and accounting invariant
