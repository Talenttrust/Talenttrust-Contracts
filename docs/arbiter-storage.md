# Arbiter Storage Layout & TTL Policy

This document catalogues every storage key that carries arbiter‑related state
in the escrow contract, describes the value shapes, and defines the TTL
(time‑to‑live) / bump strategy that governs each key. It cross‑references the
current source code and is kept accurate as the implementation evolves.

> **Source references:** All constants live in
> [`contracts/escrow/src/ttl.rs`](../contracts/escrow/src/ttl.rs). The canonical
> `DataKey` enum is defined in
> [`contracts/escrow/src/types.rs`](../contracts/escrow/src/types.rs). Arbiter‑aware
> entrypoints are implemented in
> [`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs),
> [`contracts/escrow/src/approvals.rs`](../contracts/escrow/src/approvals.rs),
> [`contracts/escrow/src/finalize.rs`](../contracts/escrow/src/finalize.rs),
> and [`contracts/escrow/src/dispute.rs`](../contracts/escrow/src/dispute.rs).

---

## 1. Overview

The arbiter is an optional third‑party address assigned at contract creation.
It participates in three distinct storage domains:

| Domain | Storage tier | Arbiter role |
|---|---|---|
| Contract assignment | Persistent | `arbiter: Option<Address>` inside `Contract` |
| Milestone approvals | Temporary | `arbiter_approved: bool` inside `MilestoneApprovals` |
| Finalization | Persistent | Arbiter may be the `finalizer` in `FinalizationRecord` |

Dispute resolution itself does **not** create separate storage keys — it
mutates the existing [`DataKey::Contract(id)`](../contracts/escrow/src/types.rs)
entry (status, accounting totals) and performs token transfers.

Each domain follows a different TTL / bump policy depending on the storage
tier and the expected active lifetime.

---

## 2. Storage Keys

### 2.1 `DataKey::Contract(u32)`

| Attribute | Value |
|---|---|
| **Storage tier** | `env.storage().persistent()` |
| **Value type** | [`Contract`](../contracts/escrow/src/types.rs) |
| **Arbiter field** | `arbiter: Option<Address>` |
| **Written at** | `create_contract` (in [`create_contract.rs`](../contracts/escrow/src/create_contract.rs)) |
| **Mutated alongside** | `release_milestone`, `refund_unreleased_milestones`, `raise_dispute`, `resolve_dispute`, `cancel_contract` (all in [`lib.rs`](../contracts/escrow/src/lib.rs)), `accept_client_migration` (in [`migration.rs`](../contracts/escrow/src/migration.rs)) |
| **Read by** | `get_contract`, `get_contract_summary`, `is_milestone_overdue`, `approve_milestone_release`, `release_milestone`, `refund_unreleased_milestones`, `raise_dispute`, `resolve_dispute`, `cancel_contract`, `finalize_contract` |

**Shape of `Contract` (arbiter‑relevant excerpt):**

```rust
pub struct Contract {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,  // ← arbiter identity
    pub status: ContractStatus,
    pub release_authorization: ReleaseAuthorization,
    // … accounting fields …
}
```

The `arbiter` field is `None` when no arbiter is assigned. An arbiter is
**required** when `release_authorization` is `ArbiterOnly` or
`ClientAndArbiter` — `create_contract` rejects those modes with `MissingArbiter`
if no arbiter is supplied. It also rejects an arbiter identical to the client
or freelancer with `InvalidArbiter`.

### 2.2 `DataKey::MilestoneApprovals(u32, u32)`

| Attribute | Value |
|---|---|
| **Storage tier** | `env.storage().temporary()` |
| **Value type** | [`MilestoneApprovals`](../contracts/escrow/src/types.rs) |
| **Arbiter field** | `arbiter_approved: bool` |
| **Written at** | `approve_milestone` (in [`approvals.rs`](../contracts/escrow/src/approvals.rs)) |
| **Removed at** | `clear_approvals` (after successful milestone release) |
| **Read by** | `get_milestone_approvals`, `get_approval_deadline`, `check_approvals`, `clear_approvals` |

**Shape of `MilestoneApprovals`:**

```rust
pub struct MilestoneApprovals {
    pub client_approved: bool,
    pub freelancer_approved: bool,
    pub arbiter_approved: bool,  // ← arbiter's approval flag
}
```

The `arbiter_approved` flag is written when the arbiter calls
`approve_milestone_release` on a contract whose `release_authorization`
mode permits arbiter approval (`ArbiterOnly`, `ClientAndArbiter`). In
`ArbiterOnly` mode this is the **only** valid approver; in `ClientAndArbiter`
mode either the client or the arbiter may approve.

Duplicate approvals from the same party are rejected (`AlreadyApproved` error).

The `get_approval_deadline` entrypoint also reads this key (via
`env.storage().temporary().has()`) to compute the expiry ledger for extant
approvals.

### 2.3 `DataKey::Finalization(u32)`

| Attribute | Value |
|---|---|
| **Storage tier** | `env.storage().persistent()` |
| **Value type** | [`FinalizationRecord`](../contracts/escrow/src/finalize.rs) |
| **Arbiter field(s)** | `finalizer: Address` (may be the arbiter), `summary.arbiter: Option<Address>` |
| **Written at** | `finalize_contract_impl` (in [`finalize.rs`](../contracts/escrow/src/finalize.rs)) |
| **Read by** | `get_finalization_record` |
| **Mutability** | Write‑once, immutable after creation |

**Shape of `FinalizationRecord`:**

```rust
pub struct FinalizationRecord {
    pub finalizer: Address,     // client, freelancer, or arbiter
    pub timestamp: u64,
    pub summary: ContractSummary, // includes arbiter: Option<Address>
}
```

The arbiter is one of three allowed finalizers (alongside client and
freelancer). The `ContractSummary` snapshot inside the record preserves the
arbiter address at close time.

### 2.4 Dispute Resolution (no separate key)

Dispute lifecycle (`raise_dispute`, `resolve_dispute`) does **not** introduce a
dedicated storage key. Instead both entrypoints operate on the existing
`DataKey::Contract(id)`:

- **`raise_dispute`** in `Escrow::raise_dispute`): Requires `contract.arbiter` to
  be `Some` (panics with `ArbiterRequired` otherwise). Sets
  `contract.status = Disputed`, extends TTL, and persists the updated contract.

- **`resolve_dispute`** in `Escrow::resolve_dispute`): Verifies the caller matches
  `contract.arbiter`, computes payouts via `resolution_payouts` (pure
  arithmetic in [`dispute.rs`](../contracts/escrow/src/dispute.rs)), performs
  SAC token transfers, updates accounting fields, and sets the final status
  via `final_status_after_resolution`.

Payout types available:
- `FullRefund` — client receives all available funds
- `PartialRefund` — freelancer gets 30 % floor, client gets remainder
- `FullPayout` — freelancer receives all available funds
- `Split(DisputeSplit)` — caller‑supplied explicit `(client_amount, freelancer_amount)` split subject to conservation checks

---

## 3. TTL / Bump Policy

### 3.1 Persistent entries (Contract, Finalization)

| Constant | Ledgers | Approximate time | Purpose |
|---|---|---|---|
| `PERSISTENT_TTL_LEDGERS` | 518 400 | ~30 days | Initial TTL on write |
| `PERSISTENT_BUMP_THRESHOLD` | 120 960 | ~7 days | Bump‑on‑read threshold |

**Contract entry bump strategy:**

Every read path that returns or operates on a `Contract` calls
`ttl::extend_contract_ttl(env, contract_id)` which invokes
`env.storage().persistent().extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS)`.

This means:
- If the remaining TTL is **below** 7 days (~120 960 ledgers), the TTL is
  extended to the full 30 days (~518 400 ledgers).
- If the remaining TTL is at or above the threshold, the extend call is a
  no‑op.
- The bump happens on every read path: `get_contract`, `get_contract_summary`,
  `release_milestone`, `refund_unreleased_milestones`, `raise_dispute`,
  `resolve_dispute`, `cancel_contract`, and `finalize_contract`.

**FinalizationRecord TTL:**

Finalization records live in the same persistent storage tier as
`DataKey::Contract(id)` and are written once via
`env.storage().persistent().set()`. Unlike the contract entry, they receive
**no active bump‑on‑read** — there is no `extend_ttl` call for
`DataKey::Finalization(id)` because the record is immutable metadata.
Once the contract is finalized, all mutating entrypoints for that contract
reject with `AlreadyFinalized`, so the record never needs renewal.
The Soroban host manages the persistent entry lifetime via its own archival
policy (typically ~120 days minimum for persistent entries).

**Existence probes versus reads:**

- `contract_exists` uses `env.storage().persistent().has()` which does **not**
  extend TTL. This is an intentional security invariant — probing for contract
  existence cannot be abused to keep entries alive.
- `get_contract` and `get_contract_summary` **do** extend TTL.

### 3.2 Temporary entries (MilestoneApprovals)

| Constant | Ledgers | Approximate time | Purpose |
|---|---|---|---|
| `PENDING_APPROVAL_TTL_LEDGERS` | 120 960 | ~7 days | Initial TTL on write |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 17 280 | ~1 day | Bump‑on‑read threshold |

**Approval bump strategy:**

1. **Write path** (`approve_milestone` in `approvals.rs`): The `MilestoneApprovals`
   struct is written via `env.storage().temporary().set()` and immediately
   extended with `extend_ttl(key, PENDING_APPROVAL_BUMP_THRESHOLD, PENDING_APPROVAL_TTL_LEDGERS)`.

2. **Read path** (`get_milestone_approvals` in `lib.rs`): If the approval
   entry is live, it conditionally extends TTL:
   ```rust
   env.storage().temporary().extend_ttl(
       &approval_key,
       ttl::PENDING_APPROVAL_BUMP_THRESHOLD,
       ttl::PENDING_APPROVAL_TTL_LEDGERS,
   );
   ```

3. **Check path** (`check_approvals` in `approvals.rs`): Uses
   `env.storage().temporary().get()` to read the entry. Uses the
   `extend_if_below_threshold` helper to conditionally bump TTL with the
   approval bump threshold.

4. **Expiry semantics**: When the TTL elapses, Soroban auto‑evicts the
   temporary entry. Both `get_milestone_approvals` and `check_approvals`
   treat `None` as "no approval exists" (fail‑closed). This means an
   arbiter‑only approval that expires prevents release — the arbiter must
   re‑approve.

5. **Cleanup**: After a successful milestone release, `clear_approvals`
   calls `env.storage().temporary().remove()` to explicitly remove the entry.

### 3.3 Summary table

| Key | Tier | Initial TTL | Bump threshold | Extension point(s) |
|---|---|---|---|---|
| `Contract(id)` | Persistent | 30 d (518 400 ledgers) | 7 d (120 960) | Every read/write path that touches the contract |
| `(Contract(id), Symbol("milestones"))` | Persistent | 30 d (518 400 ledgers) | 7 d (120 960) | `load_milestones`, `store_milestones`, `extend_milestone_ttl` |
| `MilestoneApprovals(id, idx)` | Temporary | 7 d (120 960 ledgers) | 1 d (17 280) | `approve_milestone`, `get_milestone_approvals`, `check_approvals` |
| `Finalization(id)` | Persistent | Same as Contract (30 d on write, host‑managed) | N/A | Write‑once; no active bump‑on‑read |

---

## 4. Authorization Flows Involving Arbiter

### 4.1 Release authorization modes

The arbiter's authority during milestone release is governed by
`ReleaseAuthorization`:

| Mode | Who can approve | Who can release |
|---|---|---|
| `ClientOnly` (0) | Client | Client |
| `ClientAndArbiter` (1) | Client **or** arbiter | Client or arbiter |
| `ArbiterOnly` (2) | Arbiter | Arbiter |
| `MultiSig` (3) | Client **and** freelancer | Client or freelancer |

Arbiter authorization checks are performed in `Escrow::release_milestone`
and `approvals::approve_milestone`, both comparing the caller against
`contract.arbiter`.

### 4.2 Dispute authorization

- **`raise_dispute`**: Caller must be the stored `client` or `freelancer`.
  Contract **must** have an arbiter assigned (`ArbiterRequired` otherwise).
- **`resolve_dispute`**: Caller must be the stored `contract.arbiter`
  (`UnauthorizedRole` otherwise).

### 4.3 Finalization

The arbiter is an authorized finalizer alongside client and freelancer.
The check (`require_finalizer_role` in `finalize.rs`) compares
`contract.arbiter` against the caller: `contract.arbiter.clone().is_some_and(|a| a == *finalizer)`.

---

## 5. Events Involving Arbiter

No events carry the arbiter address explicitly as a standalone field. However:
- `("created", contract_id)` is emitted at contract creation (arbiter is
  embedded in the stored `Contract`).
- `("finalized", contract_id)` carries the `finalizer` address and timestamp
  — this may be the arbiter.
- `("mlstn_rls", contract_id)` emits the `caller` which may be the arbiter
  in `ArbiterOnly` or `ClientAndArbiter` modes.

---

## 6. Cross‑References

| Document | Relevance |
|---|---|
| [`docs/escrow/storage-ttl.md`](escrow/storage-ttl.md) | Transient storage TTL policy (approvals, migrations) |
| [`docs/escrow/state-persistence.md`](escrow/state-persistence.md) | Persistent storage model |
| [`docs/escrow/authorization.md`](escrow/authorization.md) | Release authorization flows |
| [`docs/escrow/dispute-resolution.md`](escrow/dispute-resolution.md) | Dispute resolution architecture |
| [`docs/escrow/contract.md`](escrow/contract.md) | Full contract entrypoint reference |
| [`docs/escrow/architecture.md`](escrow/architecture.md) | High‑level architecture |

---

## 7. Reviewer Checklist

1. Every arbiter‑related field is documented with its storage key, tier, and
   value shape.
2. TTL constants and bump thresholds are sourced from
   [`ttl.rs`](../contracts/escrow/src/ttl.rs) and are accurate at time of
   writing.
3. All read paths that extend TTL are listed with their module and function.
4. Authorization rules for arbiter in release, dispute, and finalization are
   described.
5. New arbiter‑related keys added in future PRs should be documented here.
