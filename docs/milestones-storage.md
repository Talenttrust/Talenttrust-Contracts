# Milestones Storage Layout and TTL/Bump Policy

This document describes the on-chain storage layout for milestone data in the
TalentTrust escrow contract, including the key shapes, stored value types, and
the TTL and bump strategy that keeps active contracts alive while allowing
stale ones to be evicted automatically.

Source files cross-referenced below:

- [`contracts/escrow/src/types.rs`](../contracts/escrow/src/types.rs) — `DataKey`, `Milestone`, `MilestoneApprovals`
- [`contracts/escrow/src/ttl.rs`](../contracts/escrow/src/ttl.rs) — TTL constants and storage helpers
- [`contracts/escrow/src/approvals.rs`](../contracts/escrow/src/approvals.rs) — approval write/read path
- [`contracts/escrow/src/create_contract.rs`](../contracts/escrow/src/create_contract.rs) — initial write
- [`contracts/escrow/src/release.rs`](../contracts/escrow/src/release.rs) — release write path
- [`contracts/escrow/src/refund_impl.rs`](../contracts/escrow/src/refund_impl.rs) — refund write path
- [`contracts/escrow/src/finalize.rs`](../contracts/escrow/src/finalize.rs) — finalization read path

See also the broader storage and TTL references:

- [`docs/escrow/state-persistence.md`](escrow/state-persistence.md)
- [`docs/escrow/storage-ttl.md`](escrow/storage-ttl.md)

---

## Storage Keys

The escrow contract uses three storage keys that are directly related to
milestones. Two are **persistent** (survive archival eviction for up to 30
days after last access) and one is **temporary** (auto-evicted after 7 days).

### 1. Milestone vector — persistent

```
Key:   (DataKey::Contract(contract_id: u32), Symbol("milestones"))
Value: Vec<Milestone>
Tier:  env.storage().persistent()
```

This is the **single source of truth** for all per-milestone state. The tuple
key is constructed by `ttl::milestone_storage_key`:

```rust
// contracts/escrow/src/ttl.rs
pub(crate) fn milestone_storage_key(env: &Env, contract_id: u32) -> (DataKey, Symbol) {
    (
        DataKey::Contract(contract_id),
        Symbol::new(env, "milestones"),
    )
}
```

The vector is written at contract creation and mutated in place on deposit,
release, refund, and finalization reads.

### 2. Contract record — persistent

```
Key:   DataKey::Contract(contract_id: u32)
Value: Contract
Tier:  env.storage().persistent()
```

This key is not milestone-specific but is always bumped alongside the
milestone key. Both keys share the same TTL policy and are always extended
together via `extend_contract_and_milestones_ttl`.

### 3. Pending milestone approvals — temporary

```
Key:   DataKey::MilestoneApprovals(contract_id: u32, milestone_index: u32)
Value: MilestoneApprovals
Tier:  env.storage().temporary()
```

One record per (contract, milestone) pair. Created or updated by
`approve_milestone` in `approvals.rs` and cleared by `clear_approvals` after
a successful release. If neither action occurs, Soroban auto-evicts the entry
after 7 days.

---

## Value Shapes

### `Milestone`

Defined in `contracts/escrow/src/types.rs`:

```rust
#[contracttype]
pub struct Milestone {
    /// Target payout in stroops (immutable after creation).
    pub amount: i128,
    /// Cumulative client deposits attributed to this milestone (stroops).
    pub funded_amount: i128,
    /// Set to true by release_milestone; never reset.
    pub released: bool,
    /// Set to true by refund_unreleased_milestones; never reset.
    pub refunded: bool,
    /// Optional work evidence submitted by the freelancer before approval.
    pub work_evidence: Option<String>,
    /// Cumulative amount returned to the client for this milestone (stroops).
    pub refunded_amount: i128,
    /// Optional Unix timestamp (seconds) after which the client may claim
    /// a timeout refund without arbiter involvement. None means no deadline.
    pub deadline: Option<u64>,
}
```

Field notes:

- `amount` is set at contract creation and never updated.
- `funded_amount` tracks per-milestone deposit accounting (used by the
  per-milestone funding feature).
- A milestone is considered "settled" when either `released` or `refunded` is
  `true`. Both flags can never be `true` simultaneously — `release_milestone`
  rejects already-refunded milestones and vice versa.
- `work_evidence` is set by the freelancer before the client submits an
  approval. It is stored as a `soroban_sdk::String` and length-bounded by
  `Error::EvidenceTooLong`.
- `deadline` carries a Unix timestamp in seconds as returned by
  `env.ledger().timestamp()`. It is informational: the contract does not
  automatically cancel or release on expiry, but a client may request a
  timeout refund if the deadline has passed.

### `MilestoneApprovals`

Defined in `contracts/escrow/src/types.rs`:

```rust
#[contracttype]
pub struct MilestoneApprovals {
    pub client_approved: bool,
    pub freelancer_approved: bool,
    pub arbiter_approved: bool,
}
```

Each flag is set to `true` by the corresponding party calling
`approve_milestone`. Whether a given set of flags is sufficient to unlock
`release_milestone` depends on the contract's `ReleaseAuthorization` mode:

| Mode | Required approvals |
|---|---|
| `ClientOnly` | `client_approved` |
| `ArbiterOnly` | `arbiter_approved` |
| `ClientAndArbiter` | `client_approved` **OR** `arbiter_approved` |
| `MultiSig` | `client_approved` **AND** `freelancer_approved` |

---

## TTL Constants

All constants are defined in `contracts/escrow/src/ttl.rs`. One ledger is
approximately 5 seconds on Stellar mainnet.

| Constant | Ledgers | Duration (approx.) | Applies to |
|---|---:|---|---|
| `LEDGERS_PER_DAY` | 17,280 | 1 day | Conversion factor |
| `PERSISTENT_TTL_LEDGERS` | 518,400 | 30 days | Milestone vector, contract record |
| `PERSISTENT_BUMP_THRESHOLD` | 120,960 | 7 days | Bump trigger for persistent keys |
| `PENDING_APPROVAL_TTL_LEDGERS` | 120,960 | 7 days | Pending approval records |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 17,280 | 1 day | Bump trigger for approval records |

---

## Persistent Key TTL Policy (Milestone Vector and Contract Record)

Both `(DataKey::Contract(id), "milestones")` and `DataKey::Contract(id)` use
**bump-on-access** with the following parameters:

- **Full TTL**: `PERSISTENT_TTL_LEDGERS` = 518,400 ledgers (≈ 30 days).
  When a bump occurs, the entry's expiry is extended to `current_ledger +
  518,400`.
- **Bump threshold**: `PERSISTENT_BUMP_THRESHOLD` = 120,960 ledgers (≈ 7
  days). Soroban only extends the TTL when the remaining lifetime is strictly
  below this threshold; calls above the threshold are no-ops.

### When bumps fire

The TTL is extended on every milestone read or write via the two dedicated
helpers in `ttl.rs`:

```rust
// Bumps the milestone vector key only.
pub fn extend_milestone_ttl(env: &Env, contract_id: u32) { … }

// Bumps both contract record and milestone vector keys.
pub fn extend_contract_and_milestones_ttl(env: &Env, contract_id: u32) { … }
```

Call sites:

| Entrypoint | What is bumped |
|---|---|
| `create_contract` | Contract record written; milestone key written (no explicit bump call — TTL is set implicitly on first write in test environments; production callers should use `store_milestones`). |
| `deposit_funds` | `extend_contract_ttl` (×2) + `extend_milestone_ttl` (×1) |
| `approve_milestone` | No persistent bump — approval only touches temporary storage. |
| `release_milestone` | `extend_contract_ttl` on load; `extend_milestone_ttl` on load; `extend_contract_and_milestones_ttl` after all writes. |
| `refund_unreleased_milestones` | Milestone vector persisted; no explicit bump in `refund_impl.rs` — callers of this module should ensure TTL is extended after the call when needed. |
| `finalize_contract` | Reads milestone vector via `summarize_contract`; no bump (finalization is terminal). |

### Eviction risk

If a contract (and its milestone vector) is not accessed for more than
`PERSISTENT_TTL_LEDGERS` ledgers (≈ 30 days), the Soroban host evicts both
persistent entries. Subsequent reads return `None`, and the contract becomes
inaccessible. Off-chain indexers must compute the eviction deadline as:

```
evicts_at_ledger = last_access_ledger + PERSISTENT_TTL_LEDGERS
```

---

## Temporary Key TTL Policy (Pending Approvals)

`DataKey::MilestoneApprovals(contract_id, milestone_index)` is stored in
`env.storage().temporary()` and follows a shorter TTL:

- **Full TTL**: `PENDING_APPROVAL_TTL_LEDGERS` = 120,960 ledgers (≈ 7 days).
- **Bump threshold**: `PENDING_APPROVAL_BUMP_THRESHOLD` = 17,280 ledgers (≈ 1
  day). The Soroban host extends the TTL only when remaining life is strictly
  below this value.

### Write path

`approve_milestone` in `approvals.rs` writes directly to temporary storage
and sets the TTL in a single pair of calls:

```rust
env.storage().temporary().set(&approval_key, &approvals);
env.storage().temporary().extend_ttl(
    &approval_key,
    PENDING_APPROVAL_BUMP_THRESHOLD,
    PENDING_APPROVAL_TTL_LEDGERS,
);
```

Note: this does **not** use the `ttl::store_with_ttl` helper (which always
sets TTL to the supplied value on every write). Instead it calls `extend_ttl`
directly, which means subsequent calls to `approve_milestone` for the same
(contract, milestone) pair will only extend the TTL when the remaining life
falls below the threshold.

### Expiry and fail-closed semantics

Soroban auto-evicts temporary entries once their TTL reaches zero.
`check_approvals` reads the key with `env.storage().temporary().get(…)`, which
returns `None` for both absent and evicted entries. A `None` result causes an
immediate `Err(Error::InsufficientApprovals)`, blocking the release. This
fail-closed design means expired approvals are indistinguishable from absent
ones — both prevent the release.

### Explicit cleanup

`clear_approvals` removes the entry immediately after a successful
`release_milestone`:

```rust
env.storage().temporary().remove(&approval_key);
```

This is idempotent: removing an absent key is a no-op.

---

## Write and Read Lifecycle

```
create_contract
  └─ persistent().set(&(Contract(id), "milestones"), &milestone_vec)
  └─ persistent().set(&Contract(id), &contract)

deposit_funds
  └─ extend_contract_ttl (preflight)
  └─ extend_milestone_ttl
  └─ persistent().set(&Contract(id), &updated_contract)
  └─ extend_contract_ttl (post-write)

approve_milestone_release
  └─ persistent().get(&Contract(id))            // load contract
  └─ persistent().get(&(Contract(id), "milestones"))  // load milestones
  └─ temporary().set(&MilestoneApprovals(id, idx), &approvals)
  └─ temporary().extend_ttl(...)

release_milestone
  └─ persistent().get(&Contract(id))            // + extend_contract_ttl
  └─ persistent().get(&(Contract(id), "milestones"))  // + extend_milestone_ttl
  └─ check_approvals → temporary().get(&MilestoneApprovals(id, idx))
  └─ clear_approvals → temporary().remove(&MilestoneApprovals(id, idx))
  └─ persistent().set(&(Contract(id), "milestones"), &updated_milestones)
  └─ persistent().set(&Contract(id), &updated_contract)
  └─ extend_contract_and_milestones_ttl

refund_unreleased_milestones
  └─ persistent().get(&Contract(id))
  └─ persistent().get(&(Contract(id), "milestones"))
  └─ persistent().set(&(Contract(id), "milestones"), &updated_milestones)
  └─ persistent().set(&Contract(id), &updated_contract)

finalize_contract
  └─ persistent().get(&Contract(id))
  └─ persistent().get(&(Contract(id), "milestones"))  // via summarize_contract
  └─ persistent().set(&Finalization(id), &record)
```

---

## Invariants

1. The milestone vector and the contract record share the same contract id and
   are always kept in sync. No entrypoint writes one without also writing (or
   reading and extending) the other.

2. A milestone's `released` flag transitions from `false` to `true` exactly
   once. After release, subsequent `release_milestone` calls for the same index
   return `MilestoneAlreadyReleased` before any state is mutated.

3. A milestone's `refunded` flag transitions from `false` to `true` exactly
   once. Released milestones cannot be refunded and refunded milestones cannot
   be released.

4. Pending approvals expire after at most `PENDING_APPROVAL_TTL_LEDGERS`
   ledgers (≈ 7 days) of inactivity and are removed immediately upon a
   successful release. No released milestone can be re-released using a
   recycled approval record.

5. The accounting invariant holds across all mutations:

   ```
   funded_amount = released_amount + refunded_amount + available_balance
   ```

---

## Known Documentation Inaccuracy in `milestone-validation.md`

[`docs/escrow/milestone-validation.md`](escrow/milestone-validation.md) states
that `PENDING_APPROVAL_BUMP_THRESHOLD` is "≈ 3.5 days". This is incorrect.
The actual constant is `LEDGERS_PER_DAY` = 17,280 ledgers ≈ **1 day**, as
defined in `contracts/escrow/src/ttl.rs` and verified by the
`ledgers_per_day_constant_is_correct` test in
`contracts/escrow/src/test/ttl_tests.rs`.

---

## Reviewer Checklist

When adding new milestone-related state:

1. Choose the correct storage tier: persistent for durable milestone data,
   temporary for approval-style ephemeral state.
2. Add a corresponding entry to the TTL constants table in this document and in
   `docs/escrow/storage-ttl.md`.
3. Ensure every write path calls the appropriate TTL extension helper so that
   active contracts are not evicted prematurely.
4. Verify that `check_approvals` and any new approval-like check is fail-closed:
   `None` from temporary storage must block the operation, never permit it.
5. Add a TTL test in `contracts/escrow/src/test/ttl_tests.rs` that proves the
   entry is live before expiry and absent after.
