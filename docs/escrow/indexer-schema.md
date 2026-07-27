# ContractSummary Indexer Schema

## Overview

`ContractSummary` is a denormalised, versioned snapshot of an escrow contract
produced when a contract is finalised via [`finalize_contract`]. It is stored
as part of an immutable [`FinalizationRecord`] and can be retrieved with
[`get_finalization_record`].

**Purpose:**

- Provide off-chain indexers with a single, self-describing record that can be
  decoded without replaying every state mutation.
- Versioned via [`CONTRACT_SUMMARY_SCHEMA_VERSION`] so indexers can branch on
  schema changes without breaking.
- Freeze the summarised state at the moment of finalisation so that subsequent
  on-chain activity cannot alter the historical record.

[`finalize_contract`]: ../contracts/escrow/src/finalize.rs
[`get_finalization_record`]: ../contracts/escrow/src/lib.rs
[`FinalizationRecord`]: ../contracts/escrow/src/finalize.rs
[`CONTRACT_SUMMARY_SCHEMA_VERSION`]: ../contracts/escrow/src/types.rs

### MilestoneSummary

`MilestoneSummary` is a lightweight projection of an on-chain [`Milestone`]. It
omits internal accounting fields (`funded_amount`, `refunded_amount`,
`work_evidence`) that are not useful to indexers.

[`Milestone`]: ../contracts/escrow/src/types.rs

### How indexers consume the snapshot

1. Call [`get_finalization_record(contract_id)`] on-chain.
2. Inspect `record.summary.schema_version`.
3. Decode `record.summary` according to the schema matching that version.
4. Index the flattened fields and the `milestones` vector into an off-chain
   store.

There is currently **no** standalone `get_contract_summary` entrypoint; the
summary is only available as part of a `FinalizationRecord`. See
[ghit-issues.md](../../ghit-issues.md#L520) for the planned addition.

[`get_finalization_record(contract_id)`]: ../contracts/escrow/src/lib.rs

---

## ContractSummary Schema

Source: `contracts/escrow/src/types.rs:193`, produced by `finalize.rs:76`.

| # | Field | Type | Stored/Derived | Source / Computation |
|---|-------|------|----------------|----------------------|
| 1 | `schema_version` | `u32` | Constant | `CONTRACT_SUMMARY_SCHEMA_VERSION` (value `1`) |
| 2 | `client` | `Address` | Stored | Copied from `Contract.client` |
| 3 | `freelancer` | `Address` | Stored | Copied from `Contract.freelancer` |
| 4 | `arbiter` | `Option<Address>` | Stored | Copied from `Contract.arbiter` |
| 5 | `status` | `ContractStatus` | Stored | Copied from `Contract.status` |
| 6 | `reputation_issued` | `bool` | **Hardcoded** | Always `false` – see [Known Caveats](#reputation_issued) |
| 7 | `total_amount` | `i128` | Derived | Sum of all `Milestone.amount` values via `checked_add` |
| 8 | `funded_amount` | `i128` | Stored | Copied from `Contract.funded_amount` |
| 9 | `released_amount` | `i128` | Stored | Copied from `Contract.released_amount` |
| 10 | `refundable_balance` | `i128` | Derived | See [Derived Field Explanations](#derived-field-explanations) |
| 11 | `released_milestone_count` | `u32` | Derived | See [Derived Field Explanations](#derived-field-explanations) |
| 12 | `milestones` | `Vec<MilestoneSummary>` | Derived | Built per-milestone from stored `Vec<Milestone>` |

### Field details

#### `schema_version: u32`
- **Value:** `CONTRACT_SUMMARY_SCHEMA_VERSION` = `1`.
- **Usage:** Indexers MUST check this before interpreting the struct. Future
  versions may add, remove, or reorder fields.

#### `client: Address`
- Copied directly from the stored `Contract.client`.
- Immutable after creation.

#### `freelancer: Address`
- Copied directly from the stored `Contract.freelancer`.
- Immutable after creation.

#### `arbiter: Option<Address>`
- Copied directly from the stored `Contract.arbiter`.
- May be `None` if no arbiter was assigned.

#### `status: ContractStatus`
- Copied directly from the stored `Contract.status`.
- Only `Completed` or `Disputed` are finalisable (see `finalize.rs:155-158`).

#### `reputation_issued: bool`
- **Hardcoded to `false`.** See [Known Caveats](#reputation_issued).

#### `total_amount: i128`
- Sum of every `Milestone.amount` in the contract, accumulated with
  `checked_add`.
- Panics with `PotentialOverflow` on arithmetic overflow.

#### `funded_amount: i128`
- Copied from `Contract.funded_amount` (total client deposits).

#### `released_amount: i128`
- Copied from `Contract.released_amount` (sum of released milestone amounts).

#### `refundable_balance: i128`
- Derived. See [Derived Field Explanations](#refundable_balance).

#### `released_milestone_count: u32`
- Derived. See [Derived Field Explanations](#released_milestone_count).

#### `milestones: Vec<MilestoneSummary>`
- Built by iterating the stored `Vec<Milestone>` and projecting each entry.

---

## MilestoneSummary Schema

Source: `contracts/escrow/src/types.rs:184`, produced by `finalize.rs:100`.

| # | Field | Type | Description | Source |
|---|-------|------|-------------|--------|
| 1 | `index` | `u32` | 0-based milestone position | Copied from enumeration index |
| 2 | `amount` | `i128` | Original milestone amount in stroops | Copied from `Milestone.amount` |
| 3 | `released` | `bool` | Whether this milestone has been released | Copied from `Milestone.released` |
| 4 | `refunded` | `bool` | Whether this milestone has been refunded | Copied from `Milestone.refunded` |

### Construction

For each `(index, ms)` in the stored milestone vector, the summariser emits:

```rust
MilestoneSummary {
    index: index as u32,
    amount: ms.amount,
    released: ms.released,
    refunded: ms.refunded,
}
```

---

## Derived Field Explanations

### `refundable_balance`

Computed in `finalize.rs:108-113`:

```rust
let after_releases =
    safe_subtract_amounts(contract.funded_amount, contract.released_amount)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::AccountingInvariantViolated));
let refundable_balance =
    safe_subtract_amounts(after_releases, contract.refunded_amount)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::AccountingInvariantViolated));
```

**Derivation (expanded):**

```
refundable_balance = (funded_amount - released_amount) - refunded_amount
```

`safe_subtract_amounts` delegates to `i128::checked_sub` (`amount_validation.rs:167-169`).
If any intermediate subtraction underflows (i.e. `funded_amount < released_amount` or
`(funded_amount - released_amount) < refunded_amount`), the contract panics with
`AccountingInvariantViolated`. This should never happen for a well-formed contract,
because the mutation entrypoints enforce the invariant:

```
released_amount + refunded_amount <= funded_amount
```

**Simplified formula** (equivalent, but the code uses two steps for clarity):

```
refundable_balance = funded_amount - released_amount - refunded_amount
```

The same invariant is visible in `lib.rs:536` (`get_refundable_balance`) and
`dispute.rs:34-37` (`resolution_payouts`).

### `released_milestone_count`

Computed in `finalize.rs:85-98`:

```rust
let mut released_milestone_count: u32 = 0;
for (index, ms) in milestones.iter().enumerate() {
    if ms.released {
        released_milestone_count = released_milestone_count
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
    }
    // ...
}
```

**Derivation:**

```
released_milestone_count = count of milestones where ms.released == true
```

Each released milestone increments the counter by 1 using `checked_add`. Overflow
(> u32::MAX milestones, i.e. > 4B) panics with `PotentialOverflow`.

### `total_amount`

Computed in `finalize.rs:84-92`:

```rust
let mut total_amount: i128 = 0;
for (index, ms) in milestones.iter().enumerate() {
    total_amount = total_amount
        .checked_add(ms.amount)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
    // ...
}
```

**Derivation:**

```
total_amount = sum of all milestone.amount values
```

This sum is **not** stored on-chain; it is recomputed at finalisation time.

### `schema_version`

Set to the constant `CONTRACT_SUMMARY_SCHEMA_VERSION` (`finalize.rs:116`):

```rust
schema_version: CONTRACT_SUMMARY_SCHEMA_VERSION,
```

No negotiation or per-contract override – all finalised contracts carry the
version that was current when the contract code was deployed.

---

## Schema Versioning Policy

### Current version

`CONTRACT_SUMMARY_SCHEMA_VERSION = 1` (defined in `types.rs:180`).

### When the version MUST be bumped

Bump `CONTRACT_SUMMARY_SCHEMA_VERSION` when any of the following changes are
made to `ContractSummary` or `MilestoneSummary`:

| Change | Examples | Bump required? |
|--------|----------|----------------|
| Add a new field | Adding `resolved_at: u64` | **Yes** (breaking unless at end with default) |
| Remove a field | Removing `reputation_issued` | **Yes** (breaking) |
| Rename a field | `total_amount` → `milestone_total` | **Yes** (breaking for named-field decoding) |
| Change field type | `released_milestone_count` from `u32` to `i128` | **Yes** (breaking) |
| Change derivation logic | Different `refundable_balance` formula | **Yes** (semantic breaking change) |
| Reorder fields | Moving `status` after `milestones` | **Yes** (positional decoders break) |
| Add new variant | New `ContractStatus` variant | **Yes** (exhaustive-match decoders break) |
| Fix `reputation_issued` to read real state | Wire to `DataKey::ReputationIssued` | **No** (bugfix, schema-compatible) |
| Add optional field at end | Adding `resolved_at: Option<u64>` after `milestones` | **No** (backward compatible) |

### Breaking vs non-breaking changes

**Breaking:** Any change that would cause a downstream indexer to mis-decode or
panic if it uses the old schema version to read the new struct.

**Non-breaking:** Bugfixes that correct the value of an existing field without
changing its type or position (e.g. wiring `reputation_issued` to real storage).

### Backward compatibility expectations

- Old indexers MUST continue to decode schema version `1` records correctly
  even after a new version is introduced.
- The contract code MUST NOT produce records with a schema version older than
  the current constant.
- Schema version is **not** downgraded when contract code is rolled back.

### How downstream indexers should branch on `schema_version`

```python
record = client.get_finalization_record(contract_id)
summary = record.summary

if summary.schema_version == 1:
    index_v1(summary)
elif summary.schema_version == 2:
    index_v2(summary)
else:
    raise RuntimeError(f"Unknown schema version {summary.schema_version}")
```

Indexers SHOULD treat unknown schema versions as hard errors rather than
attempting best-effort decoding.

### Migration guidance

When bumping the schema version:

1. Increment `CONTRACT_SUMMARY_SCHEMA_VERSION` in `types.rs`.
2. Update the `ContractSummary` struct (add/remove/change fields).
3. Update `MilestoneSummary` if needed.
4. Update `summarize_contract` in `finalize.rs` to populate new fields.
5. Update this document (`docs/escrow/indexer-schema.md`).
6. Update downstream indexer code to handle the new version.
7. Run `cargo test` to verify all existing tests still pass (they use the
   current version constant).

---

## Known Caveats

### `reputation_issued`

**Current behaviour:** `reputation_issued` is **hardcoded to `false`** in
`summarize_contract` (`finalize.rs:121`).

```rust
reputation_issued: false,
```

The actual on-chain reputation issuance state is stored under
`DataKey::ReputationIssued(contract_id)` (a persistent `bool` written by
`issue_reputation` in `lib.rs:711`).

**Why:** The summariser does not read `DataKey::ReputationIssued(contract_id)`
at finalisation time. This is a known gap.

**Impact:** Indexers cannot rely on `reputation_issued` to determine whether
reputation has been issued. As a workaround, indexers can:

- Call `env.storage().persistent().get::<_, bool>(&DataKey::ReputationIssued(contract_id))`
  directly (off-chain indexers with archive-node access).
- Ignore the field and derive reputation state from on-chain events
  (`("rep_issd", contract_id)`).

**Planned fix:** Tracked in the issue tracker. When implemented, the field will
read `DataKey::ReputationIssued(contract_id)`:

```rust
let reputation_issued = env.storage()
    .persistent()
    .get::<_, bool>(&DataKey::ReputationIssued(contract_id))
    .unwrap_or(false);
```

### No standalone `get_contract_summary` entrypoint

`ContractSummary` is currently produced only at finalisation time. There is no
`get_contract_summary(contract_id) -> ContractSummary` entrypoint for active
(non-finalised) contracts. See `ghit-issues.md:520-532`.

### `ContractSummary` and `MilestoneSummary` not publicly exported

These types are defined in `types.rs` but are **not** re-exported from `lib.rs`
(line 38-41). They exist only for internal use by `finalize.rs`. The types are
still part of the public ABI because they appear in the contract's
`FinalizationRecord` return type.

---

## Examples

### Fully completed contract (3 milestones, all released)

Three milestones of 200, 400, and 600 units. Total = 1200. All deposited, all
released.

```
schema_version: 1
client:         <client_addr>
freelancer:     <freelancer_addr>
arbiter:        None
status:         Completed
reputation_issued: false
total_amount:         1200
funded_amount:        1200
released_amount:      1200
refundable_balance:   0
released_milestone_count: 3
milestones:
  [{index: 0, amount: 200, released: true,  refunded: false},
   {index: 1, amount: 400, released: true,  refunded: false},
   {index: 2, amount: 600, released: true,  refunded: false}]
```

### Disputed contract (funded, nothing released)

Full deposit, no releases. Raised to Disputed.

```
schema_version: 1
status:         Disputed
total_amount:         1200
funded_amount:        1200
released_amount:      0
refundable_balance:   1200
released_milestone_count: 0
milestones:
  [{index: 0, amount: 200, released: false, refunded: false},
   {index: 1, amount: 400, released: false, refunded: false},
   {index: 2, amount: 600, released: false, refunded: false}]
```

### Mixed release/refund (2 released, 1 refunded)

Milestones 0 and 1 released. Milestone 2 refunded. Status is Completed because
not all milestones are refunded.

```
schema_version: 1
status:         Completed
total_amount:         1200
funded_amount:        1200
released_amount:      600   (200 + 400)
refundable_balance:   0     (1200 - 600 - 600 = 0)
released_milestone_count: 2
milestones:
  [{index: 0, amount: 200, released: true,  refunded: false},
   {index: 1, amount: 400, released: true,  refunded: false},
   {index: 2, amount: 600, released: false, refunded: true}]
```
