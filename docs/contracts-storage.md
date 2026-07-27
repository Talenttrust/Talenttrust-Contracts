# Contracts Storage Layout & TTL Policy

This document describes the on-chain storage layout used by the TalentTrust
escrow contract on Soroban: every storage key, its value shape, which Soroban
storage type it lives in (`persistent` vs `temporary`), and the deterministic
TTL / bump strategy that governs its lifetime.

All values are Soroban `#[contracttype]` types or primitives defined in
[`contracts/escrow/src/types.rs`](../contracts/escrow/src/types.rs).  TTL
constants and helpers live in
[`contracts/escrow/src/ttl.rs`](../contracts/escrow/src/ttl.rs).  The
canonical `DataKey` enum is defined in
[`types.rs#L59-L93`](../contracts/escrow/src/types.rs#L59-L93).

---

## 1. Storage Types at a Glance

| Soroban storage kind | Used for | Eviction model |
|---|---|---|
| `env.storage().persistent()` | Contract state, accounting records, governance config, reputation, finalization, settlement-token binding | Manual TTL extension; evicted by the host after `PERSISTENT_TTL_LEDGERS` without a renewing access |
| `env.storage().temporary()` | Pending milestone approvals, pending client migrations | Auto-evicted by the host as soon as their TTL elapses; no on-chain eviction event |
| `env.storage().instance()` | (not used directly by the escrow; reserved for contract-level metadata) | — |

The contract never writes to `instance()` storage for its own records.

---

## 2. Unit Conversions

All TTL constants are denominated in **ledgers** (the native Soroban expiry
unit).  On Stellar mainnet one ledger closes roughly every 5 seconds.  The
conversion factor used everywhere is `LEDGERS_PER_DAY = 17 280`.

| Name | Ledgers | Approximate wall-clock |
|---|---:|---|
| `LEDGERS_PER_DAY` | 17 280 | 1 day |
| `PENDING_APPROVAL_TTL_LEDGERS` | 120 960 | 7 days |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 17 280 | 1 day |
| `PENDING_MIGRATION_TTL_LEDGERS` | 362 880 | 21 days |
| `PENDING_MIGRATION_BUMP_THRESHOLD` | 51 840 | 3 days |
| `PERSISTENT_TTL_LEDGERS` | 518 400 | 30 days |
| `PERSISTENT_BUMP_THRESHOLD` | 120 960 | 7 days |
| `ADMIN_ROTATION_MIN_DELAY_LEDGERS` | 34 560 | 2 days (timelock, **not** a storage TTL) |

Reference:
[`ttl.rs#L45-L61`](../contracts/escrow/src/ttl.rs#L45-L61).

---

## 3. Persistent Storage Keys

Each entry below lists: the key expression, the Rust value type, a short
description, the TTL renew strategy, and a code pointer that performs the
write or the canonical read.

### 3.1 Initialization & Admin

| Key | Value type | Description | TTL bump? | Write site |
|---|---|---|---|---|
| `DataKey::Initialized` | `bool` | Flipped to `true` exactly once by `initialize`.  Absent means the contract is not yet initialized. | Never bumped (effectively immortal because it is only read in guards and never written after init). | [`lib.rs#L367-L378`](../contracts/escrow/src/lib.rs#L367-L378) |
| `DataKey::Admin` | `Address` | Operational admin address.  Authorizes pause/emergency, protocol fees, governed parameters, settlement-token binding, admin rotation, and fee withdrawal.  Set during `initialize` and rotated via the two-step `PendingAdmin` proposal. | Never bumped explicitly; read on every admin-gated call, so in practice it is always hot. | [`lib.rs#L376-L378`](../contracts/escrow/src/lib.rs#L376-L378), [`governance.rs#L124-L133`](../contracts/escrow/src/governance.rs#L124-L133) |
| `DataKey::PendingAdmin` | `PendingAdminProposal { proposed: Address, proposed_at_ledger: u32 }` | Two-step admin-rotation proposal.  Cleared on accept or cancel.  A proposal must age at least `ADMIN_ROTATION_MIN_DELAY_LEDGERS` before it can be accepted (timelock enforced at accept time, not via storage TTL). | Never bumped; acceptance gate reads `proposed_at_ledger` and compares with the current sequence. | [`governance.rs#L85-L91`](../contracts/escrow/src/governance.rs#L85-L91), [`governance.rs#L107-L133`](../contracts/escrow/src/governance.rs#L107-L133) |

### 3.2 Pause & Emergency

| Key | Value type | Description | TTL bump? | Write site |
|---|---|---|---|---|
| `DataKey::Paused` | `bool` | Normal operational pause.  When `true` every *mutating* entrypoint panics with `ContractPaused`; read-only queries still succeed.  `unpause` clears it; `activate_emergency_pause` *also* sets it. | Never bumped. | [`lib.rs#L1428-L1465`](../contracts/escrow/src/lib.rs#L1428-L1465) |
| `DataKey::Emergency` | `bool` | Emergency freeze.  When `true` the same mutation gate fires `EmergencyActive` and `unpause` itself is blocked; only `resolve_emergency` clears both `Emergency` and `Paused`.  Flipping `Emergency` on once also sets `ReadinessChecklist::emergency_controls_enabled = true` permanently so deployers can prove they tested the emergency circuit. | Never bumped. | [`lib.rs#L1486-L1566`](../contracts/escrow/src/lib.rs#L1486-L1566) |

### 3.3 Contracts & Milestones

| Key | Value type | Description | TTL bump? | Write / load site |
|---|---|---|---|---|
| `DataKey::NextContractId` | `u32` | Monotonic allocator.  Starts at 1 after `initialize`; incremented after every successful `create_contract`.  Reads are cheap and do **not** extend TTL on `get_next_contract_id`; only the creation path calls `extend_next_contract_id_ttl` before touching it. | `PERSISTENT_BUMP_THRESHOLD` → `PERSISTENT_TTL_LEDGERS`, only from `create_contract`. | [`ttl.rs#L160-L168`](../contracts/escrow/src/ttl.rs#L160-L168), [`create_contract.rs#L115-L166`](../contracts/escrow/src/create_contract.rs#L115-L166) |
| `DataKey::Contract(contract_id: u32)` | [`Contract`](../contracts/escrow/src/types.rs#L213-L226) struct (`client`, `freelancer`, `arbiter: Option<Address>`, `status: ContractStatus`, `total_deposited`, `funded_amount`, `released_amount`, `refunded_amount`, `release_authorization: ReleaseAuthorization`, `reputation_issued: bool`) | Core accounting + lifecycle record for escrow `contract_id`.  All money-moving entrypoints read-then-write this key. | Bumped to `PERSISTENT_TTL_LEDGERS` (threshold = `PERSISTENT_BUMP_THRESHOLD`) on every read or write via `extend_contract_ttl`.  Exceptions: `contract_exists` is a pure existence probe and deliberately does **not** bump TTL, to prevent keep-alive abuse. | [`create_contract.rs#L136-L138`](../contracts/escrow/src/create_contract.rs#L136-L138), [`lib.rs#L1202-L1212`](../contracts/escrow/src/lib.rs#L1202-L1212), [`ttl.rs#L171-L177`](../contracts/escrow/src/ttl.rs#L171-L177) |
| `(DataKey::Contract(contract_id), Symbol::new(env, "milestones"))` | `Vec<`[`Milestone`](../contracts/escrow/src/types.rs#L228-L241)`>` (each: `amount`, `funded_amount`, `released: bool`, `refunded: bool`, `work_evidence: Option<String>`, `refunded_amount`, `deadline: Option<u64>`) | **Compound tuple key**, *not* a `DataKey` variant.  Stores the ordered milestone vector.  `Milestone.released` / `Milestone.refunded` flags are the single source of truth; the declared `DataKey::MilestoneReleased(u32, u32)` variant is **never written** (see §5). | Bumped whenever the vector is loaded or stored via `load_milestones` / `store_milestones` / `extend_milestone_ttl`.  The same `PERSISTENT_BUMP_THRESHOLD → PERSISTENT_TTL_LEDGERS` policy applies. | [`ttl.rs#L134-L186`](../contracts/escrow/src/ttl.rs#L134-L186), [`create_contract.rs#L140-L156`](../contracts/escrow/src/create_contract.rs#L140-L156) |

#### `ContractStatus` enum (written inside `Contract.status`)

```
Created = 0 → Accepted = 1 → Funded / PartiallyFunded = 2 / 7 → Completed = 3
                                                     ↘ Disputed = 4  ↗
                                            Cancelled = 5 / Refunded = 6 (terminal)
```

Defined at [`types.rs#L199-L210`](../contracts/escrow/src/types.rs#L199-L210).

### 3.4 Governance & Protocol Fees

| Key | Value type | Description | TTL bump? | Write site |
|---|---|---|---|---|
| `DataKey::ProtocolFeeBps` | `u32` | Release fee in basis points.  Defaults to `0` (no fee).  Max `10 000` (= 100 %).  Overridden atomically by `set_governed_params` which writes `GovernedParameters` instead; both keys are consulted. | Never bumped explicitly. | [`governance.rs#L32-L55`](../contracts/escrow/src/governance.rs#L32-L55) |
| `DataKey::GovernedParameters` | [`GovernedParameters { protocol_fee_bps: u32, max_escrow_total_stroops: i128 }`](../contracts/escrow/src/types.rs#L299-L304) | Canonical combined governance record.  Setting it via `set_governed_params` also flips `ReadinessChecklist::governed_params_set = true` to mark the deploy step complete. | Never bumped explicitly. | [`governance.rs#L200-L249`](../contracts/escrow/src/governance.rs#L200-L249) |
| `DataKey::AccumulatedProtocolFees` | `i128` | Running total of protocol fees retained inside the SAC balance, accrued on each `release_milestone`.  Drained by `withdraw_protocol_fees`.  Because fees are commingled with the escrow balance in the SAC token, this counter is the authoritative record of how much is owed to the protocol vs owed to counterparties. | Bumped on write in `withdraw_protocol_fees` using the persistent policy. | [`lib.rs#L849-L854`](../contracts/escrow/src/lib.rs#L849-L854), [`lib.rs#L2036-L2060`](../contracts/escrow/src/lib.rs#L2036-L2060) |

### 3.5 Settlement-Token Custody

| Key | Value type | Description | TTL bump? | Write site |
|---|---|---|---|---|
| `DataKey::SettlementToken` | `Address` | Write-once SAC token address bound by `bind_settlement_token`.  All `deposit_funds`, `release_milestone`, `refund_*`, `cancel_contract`, and `withdraw_protocol_fees` paths perform `token::Client::transfer` against this address; absence of the binding panics with `SettlementTokenNotConfigured`. | Never bumped; read-only getters (`get_settlement_token`, `is_settlement_token_bound`) also do not extend TTL. | [`lib.rs#L182-L187`](../contracts/escrow/src/lib.rs#L182-L187), [`lib.rs#L256-L313`](../contracts/escrow/src/lib.rs#L256-L313) |

### 3.6 Finalization (Immutable Close Records)

| Key | Value type | Description | TTL bump? | Write site |
|---|---|---|---|---|
| `DataKey::Finalization(contract_id: u32)` | [`FinalizationRecord { finalizer: Address, timestamp: u64, summary: ContractSummary }`](../contracts/escrow/src/finalize.rs#L13-L22) | Immutable snapshot written when a participant closes a `Completed` or `Disputed` contract.  Once written, every contract-specific mutating entrypoint fails `require_not_finalized` with `AlreadyFinalized`. | Not bumped explicitly; written once and typically read shortly thereafter. | [`finalize.rs#L140-L168`](../contracts/escrow/src/finalize.rs#L140-L168) |

### 3.7 Readiness Checklist

| Key | Value type | Description | TTL bump? | Write site |
|---|---|---|---|---|
| `DataKey::ReadinessChecklist` | [`ReadinessChecklist { initialized: bool, governed_params_set: bool, emergency_controls_enabled: bool }`](../contracts/escrow/src/types.rs#L277-L297) | Three-bit progress tracker for mainnet-deploy QA.  Each flag is flipped by the entrypoint that performs the corresponding step: `initialize`, `set_governed_params`, and `activate_emergency_pause` (the latter is sticky once flipped). | Never bumped. | [`lib.rs#L383-L391`](../contracts/escrow/src/lib.rs#L383-L391), [`governance.rs#L238-L246`](../contracts/escrow/src/governance.rs#L238-L246), [`lib.rs#L1504-L1512`](../contracts/escrow/src/lib.rs#L1504-L1512) |

### 3.8 Reputation

| Key | Value type | Description | TTL bump? | Write site |
|---|---|---|---|---|
| `DataKey::ReputationIssued(contract_id: u32)` | `bool` | Per-contract "already issued" guard.  Redundantly tracks `Contract.reputation_issued`; both are consulted in the summary path.  Written together with the reputation counters in `issue_reputation`. | Bumped at write-time in `issue_reputation` using the persistent policy. | [`lib.rs#L1724-L1735`](../contracts/escrow/src/lib.rs#L1724-L1735) |
| `DataKey::PendingReputationCredits(freelancer: Address)` | `i128` | Counter of completed contracts awaiting a client rating.  Incremented by `grant_pending_reputation_credit` (on final milestone release or dispute completion); decremented by exactly `1` per `issue_reputation` call.  Refunded contracts never grant a credit. | Not bumped explicitly; read/written without TTL extension. | [`lib.rs#L625-L629`](../contracts/escrow/src/lib.rs#L625-L629), [`lib.rs#L1737-L1742`](../contracts/escrow/src/lib.rs#L1737-L1742) |
| `DataKey::Reputation(freelancer: Address)` | [`Reputation { completed_contracts: i128, total_rating: i128, last_rating: i128 }`](../contracts/escrow/src/types.rs#L318-L324) | Aggregate counters per freelancer.  `get_average_rating` returns `(total_rating * 10_000 / completed_contracts)` when `completed_contracts > 0`; `None` otherwise. | Not bumped explicitly. | [`lib.rs#L1744-L1750`](../contracts/escrow/src/lib.rs#L1744-L1750), [`lib.rs#L1778-L1811`](../contracts/escrow/src/lib.rs#L1778-L1811) |
| `DataKey::ReputationComment(contract_id: u32)` | `String` (max 200 UTF-8 bytes) | Client-supplied free-form feedback written by `issue_reputation`.  Capped at 200 bytes to cap storage growth; validated at write time by `EmptyComment` / `CommentTooLong`. | Bumped at write-time in `issue_reputation` and on read in `get_reputation_comment` using the persistent policy. | [`lib.rs#L1752-L1758`](../contracts/escrow/src/lib.rs#L1752-L1758), [`lib.rs#L1765-L1776`](../contracts/escrow/src/lib.rs#L1765-L1776) |

---

## 4. Temporary Storage Keys (TTL-governed, auto-evicting)

Everything in this section lives in `env.storage().temporary()` and is
subject to Soroban host auto-eviction.  The contract consistently treats a
missing / evicted entry as "not approved / not migrated" (fail-closed).

### 4.1 Pending Milestone Approvals

| Key | Value type | Description | TTL | Bump threshold |
|---|---|---|---|---|
| `DataKey::MilestoneApprovals(contract_id: u32, milestone_index: u32)` | [`MilestoneApprovals { client_approved: bool, freelancer_approved: bool, arbiter_approved: bool }`](../contracts/escrow/src/types.rs#L259-L266) | Bitmask of which parties have pre-approved a given milestone for release.  Required approvers depend on `Contract.release_authorization`: `ClientOnly`, `ClientAndArbiter`, `ArbiterOnly`, or `MultiSig` (client **and** freelancer).  Cleared explicitly by `clear_approvals` after a successful release. | 7 d = `PENDING_APPROVAL_TTL_LEDGERS` | 1 d = `PENDING_APPROVAL_BUMP_THRESHOLD` |

- **Write path:** `approve_milestone` in
  [`approvals.rs#L46-L159`](../contracts/escrow/src/approvals.rs#L46-L159)
  calls `.temporary().set` then `.temporary().extend_ttl(threshold, ttl)`.
  Duplicate approvals from the same role return `AlreadyApproved`.
- **Bump-on-read:** `get_milestone_approvals` renews TTL when the entry is
  live; missing entries return `None` without writing.  See
  [`lib.rs#L1388-L1403`](../contracts/escrow/src/lib.rs#L1388-L1403).
- **Check path:** `check_approvals` in
  [`approvals.rs#L180-L212`](../contracts/escrow/src/approvals.rs#L180-L212)
  performs a plain `.get`; any `None` → `InsufficientApprovals` fail-closed.
- **Explicit removal:** `clear_approvals` after successful release
  ([`approvals.rs#L222-L225`](../contracts/escrow/src/approvals.rs#L222-L225)).

### 4.2 Pending Client Migrations

| Key | Value type | Description | TTL | Bump threshold |
|---|---|---|---|---|
| `DataKey::PendingClientMigration(contract_id: u32)` | [`PendingClientMigration { current_client: Address, proposed_client: Address, requested_at_ledger: u32, expires_at_ledger: u32 }`](../contracts/escrow/src/migration.rs#L5-L12) | Single-slot proposal to transfer the `client` role on a contract to a new address.  At most one proposal may be pending per contract; re-proposing panics with `InvalidState`.  Migrations are disallowed on `Completed`, `Cancelled`, `Refunded`, or `Disputed` contracts. | 21 d = `PENDING_MIGRATION_TTL_LEDGERS` | 3 d = `PENDING_MIGRATION_BUMP_THRESHOLD` |

- **Write path:** `propose_client_migration_impl` in
  [`migration.rs#L48-L90`](../contracts/escrow/src/migration.rs#L48-L90)
  writes via `ttl::store_with_ttl`.  `expires_at_ledger` in the struct is
  informational (for indexers); the authoritative TTL is the host-level one
  set by `store_with_ttl`.
- **Read path:** `read_if_live` wraps `.temporary().get`; `None` is treated
  as "no pending migration" whether due to eviction or to never being set.
  See [`migration.rs#L105-L125`](../contracts/escrow/src/migration.rs#L105-L125)
  and
  [`migration.rs#L156-L168`](../contracts/escrow/src/migration.rs#L156-L168).
- **Explicit removal:** `cancel_client_migration` via
  `ttl::remove_transient`
  ([`migration.rs#L131-L155`](../contracts/escrow/src/migration.rs#L131-L155)).

---

## 5. DataKey Variants Declared but **Not** Written

The `DataKey` enum declares the following variants that, as of this writing,
have no storage write site in the contract.  They are listed here so an
indexer does not expect them on-chain.

| Variant | Declared at | Status | Single source of truth instead |
|---|---|---|---|
| `DataKey::MilestoneReleased(u32, u32)` | [`types.rs#L70`](../contracts/escrow/src/types.rs#L70) | Never persisted.  Verified by the storage test comment in [`test/storage.rs#L272-L273`](../contracts/escrow/src/test/storage.rs#L272-L273) and again in [`test/summary.rs#L179`](../contracts/escrow/src/test/summary.rs#L179). | Each `Milestone.released` / `refunded` boolean inside the milestone vector compound key (§3.3). |
| `DataKey::GovernanceAdmin` | [`types.rs#L80`](../contracts/escrow/src/types.rs#L80) | Never used; superseded by `DataKey::Admin` during the initial implementation. | `DataKey::Admin`. |
| `DataKey::PendingGovernanceAdmin` | [`types.rs#L81`](../contracts/escrow/src/types.rs#L81) | Never used; superseded by `DataKey::PendingAdmin`. | `DataKey::PendingAdmin`. |
| `DataKey::ProtocolParameters` | [`types.rs#L82`](../contracts/escrow/src/types.rs#L82) | Never used; the combined-parameters struct lives under `GovernedParameters` and the legacy BPS value under `ProtocolFeeBps`. | `DataKey::GovernedParameters` + `DataKey::ProtocolFeeBps`. |

---

## 6. TTL / Bump Strategy Summary

### 6.1 Persistent entries: 30-day renew on access

Every frequently-accessed persistent key is extended using the same two
constants via `extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS)`:

- If remaining TTL < 7 days (120 960 ledgers): extend to 30 days.
- Otherwise: no-op (Soroban `extend_ttl` never shortens).

Keys that receive this treatment from the dedicated helpers in
[`ttl.rs#L171-L199`](../contracts/escrow/src/ttl.rs#L171-L199):

| Helper | Target key |
|---|---|
| `extend_contract_ttl(contract_id)` | `DataKey::Contract(contract_id)` |
| `extend_milestone_ttl(contract_id)` | `(DataKey::Contract(contract_id), "milestones")` — via `milestone_storage_key` |
| `extend_contract_and_milestones_ttl(contract_id)` | Both above in one call |
| `extend_next_contract_id_ttl()` | `DataKey::NextContractId` |
| `extend_participant_contract_index_ttl(&key)` | Any participant contract-index `DataKey` (currently wired through the helper but the concrete index keys are reserved for a future list API) |

Call-site TTL extensions:

- `ReputationIssued(contract_id)` — bumped inline in `issue_reputation`.
- `ReputationComment(contract_id)` — bumped inline in `issue_reputation` and `get_reputation_comment`.
- `AccumulatedProtocolFees` — bumped inline in `withdraw_protocol_fees`.

**Eviction risk:** Any single persistent entry that goes untouched for more
than `PERSISTENT_TTL_LEDGERS` (≈ 30 days) will be evicted by the Soroban
host.  Because the contract reads `Contract(id)` / milestones together,
active contracts stay hot; the deliberate design choice is that *inactive*
contracts and their associated records are archived automatically by the
network rather than persisting forever.  If the milestone vector is evicted
but the `Contract(id)` record is not, `load_milestones` still panics with
`ContractNotFound`, so callers observe a consistent "contract gone" state.

### 6.2 Temporary entries: bump on access within threshold

| Entry family | Full TTL | Bump threshold | Behavior below threshold |
|---|---:|---:|---|
| Milestone approvals (`MilestoneApprovals`) | 7 d | 1 d | On `approve_milestone` write, `get_milestone_approvals` read, and — via the host `extend_ttl(threshold, ttl)` semantics — whenever a read/write occurs inside the last day.  Outside the threshold, reads still succeed but do not extend. |
| Client migrations (`PendingClientMigration`) | 21 d | 3 d | Same semantics via `store_with_ttl` and `extend_if_below_threshold`.  Reads use `read_if_live`, which itself does **not** bump; explicit bump calls are placed in the acceptance / cancellation paths where needed. |

### 6.3 Helper API (from `ttl.rs`)

| Helper | Storage kind | Description |
|---|---|---|
| `compute_expiry(env, ttl_ledgers)` | pure | `sequence.saturating_add(ttl_ledgers)` — used by off-chain-facing deadline getters. |
| `store_with_ttl(env, key, value, ttl)` | temporary | `.set` + `.extend_ttl(ttl, ttl)` in one call. |
| `read_if_live::<V>(env, key) -> Option<V>` | temporary | Thin wrapper around `.get`.  `None` covers both "absent" and "evicted". |
| `extend_if_below_threshold(env, key, threshold, extend_to) -> bool` | temporary | Returns `false` when the key is absent / evicted; otherwise performs the thresholded extend.  The boolean reports **liveness**, not whether the host actually performed an extension. |
| `remove_transient(env, key)` | temporary | Idempotent `.remove`. |
| `has_transient(env, key) -> bool` | temporary | `.has` proxy; returns `false` after eviction just as it does for a never-set key. |
| `load_milestones(env, id) -> Vec<Milestone>` | persistent | `.get` (panics with `ContractNotFound` on absent) then `extend_milestone_ttl`. |
| `store_milestones(env, id, milestones)` | persistent | `.set` then `extend_milestone_ttl`. |
| `milestone_storage_key(env, id)` | pure | Returns the compound `(DataKey::Contract(id), Symbol("milestones"))` tuple. |
| `extend_*_ttl(...)` helpers listed in §6.1 | persistent | Consistent persistent-policy wrappers. |

Reference:
[`ttl.rs#L64-L199`](../contracts/escrow/src/ttl.rs#L64-L199).

---

## 7. Fail-Closed Semantics

The following security-relevant guarantees arise directly from the storage
layout:

1. **Missing or evicted approval ≠ not approved.** `release_milestone`
   calls `approvals::check_approvals`, which `.get`s the temporary record;
   `None` maps to `InsufficientApprovals` (see
   [`approvals.rs#L186-L211`](../contracts/escrow/src/approvals.rs#L186-L211)).
   An approval whose TTL expires between the `approve_*` and
   `release_milestone` calls therefore cannot be reused — the caller must
   re-approve.

2. **Missing or evicted migration ≠ no migration.**
   `accept_client_migration_impl` and `get_pending_client_migration_impl`
   use `read_if_live`; `None` panics with `InvalidState`, preventing a
   stale (evicted) proposal from being accepted and preventing a caller
   from reading a phantom record.

3. **Contract absence ≠ present data.** Every mutating entrypoint loads
   `Contract(id)` via `.get().unwrap_or_else(|| panic_with_error(ContractNotFound))`.
   The single exception is `contract_exists`, which is a pure `has()` probe
   that deliberately avoids bumping TTL so it cannot be abused as a
   keep-alive mechanism.

4. **`require_not_finalized` + `require_not_paused` gate state mutation
   before any storage touch.** See
   [`finalize.rs#L36-L65`](../contracts/escrow/src/finalize.rs#L36-L65) for
   both guards — they run before auth in every lifecycle path.

---

## 8. Storage Access & TTL Tests

| Test module | What it covers |
|---|---|
| [`test/storage.rs`](../contracts/escrow/src/test/storage.rs) | Per-key existence / correctness for `Initialized`, `Admin`, `Paused`, `Emergency`, `Contract(id)`, `NextContractId`, milestone vectors (and the `MilestoneReleased` no-write assertion), `ReputationIssued`, `PendingReputationCredits`, `Reputation`, `ReadinessChecklist`, released-amount accounting, and single-index milestone getters. |
| [`test/ttl_tests.rs`](../contracts/escrow/src/test/ttl_tests.rs) | TTL constants, `compute_expiry` (including saturating), `store_with_ttl`, `read_if_live`, eviction at +1 ledger, `extend_if_below_threshold` liveness boolean, exact-threshold no-op, `remove_transient` idempotency, `has_transient` tracking, determinism across independent envs, and integration of approval TTL with `approve_milestone` / `check_approvals`. |
| [`test/approval_expiry.rs`](../contracts/escrow/src/test/approval_expiry.rs) | Approval-expiry invariants for each `ReleaseAuthorization` mode. |
| [`test/persistence.rs`](../contracts/escrow/src/test/persistence.rs) | Absent-state read behavior across multiple lifecycle readers. |
| [`test/participant_index_pagination.rs`](../contracts/escrow/src/test/participant_index_pagination.rs) | Pagination behavior for the future `list_contracts_by_participant` indexer API (uses the `extend_participant_contract_index_ttl` helper wired in `ttl.rs`). |

---

## 9. Reviewer Checklist for Storage Changes

When introducing a new storage key, make sure all of the following are
addressed before landing:

1. Add the variant to `DataKey` in `types.rs`, or use a compound tuple key
   if the key depends on a sub-identifier (e.g. the milestone vector's
   `(Contract(id), Symbol("milestones"))` pattern).
2. Decide between `persistent()` and `temporary()`.  Use temporary for
   anything that must auto-expire without an explicit cleanup call
   (approvals, proposals, short-lived permissions).  Use persistent for
   accounting / governance / immutable records.
3. For temporary entries: pick a TTL, bump threshold, add a row to §4
   above, and use `store_with_ttl` + `read_if_live` uniformly (no direct
   `.set` bypass).
4. For persistent entries: decide if / when TTL is extended and use one of
   the `extend_*_ttl` helpers consistently.  Document any "deliberately not
   bumped" exceptions (e.g. `contract_exists`, `is_settlement_token_bound`).
5. Add a storage test that writes then reads back, and — for
   temporary entries — a TTL eviction test that advances ledger sequence
   past TTL + 1 and asserts `None`.
6. Re-read this document and update the affected tables so they stay in
   sync with the code.
