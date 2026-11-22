# Escrow summary, receipt and schema versioning

Indexers use `EscrowSummary` for a read-only view of an escrow and
`EscrowReceipt` for the compact record retained after terminal escrow compaction.
The field definitions are in [`lib.rs`](../../contracts/escrow/src/lib.rs) and
[`types.rs`](../../contracts/escrow/src/types.rs). These are separate from the
event payload schema identified by `EVENT_SCHEMA_V`.

## Read interface

| Call | Result and behavior |
| --- | --- |
| `get_escrow_summary(id)` | `Result<EscrowSummary, EscrowError>` for one uncompacted escrow; unknown ids return `ContractNotFound`, compacted ids return `EscrowCompacted`. |
| `get_escrow_summaries(ids)` | `Result<Vec<EscrowSummary>, EscrowError>`; accepts at most `MAX_SUMMARY_BATCH` (`25`) input ids. Larger inputs return `InvalidAmount`. Unknown and compacted ids are skipped; surviving entries follow input order. An empty input returns an empty vector. Match by `id`, not by position in the original request. |
| `get_receipt(id)` | `Option<EscrowReceipt>`; `Some` after compaction, `None` when no receipt exists (including an unknown or uncompacted escrow). |

These reads require no party authorization and do not migrate storage. A summary
is computed from the current escrow; it is not an immutable finalization snapshot.
Terminal escrows still have summaries until they are compacted. After compaction,
read the receipt instead; a missing batch entry alone does not prove compaction.

## `EscrowSummary`

Amounts are `i128` token base units, not floating-point display values.
`ContractStatus` is one of `Created`, `Funded`, `Completed`, `Disputed`, `Resolved`
or `Cancelled`.

| Field | Rust type | Meaning |
| --- | --- | --- |
| `id` | `u64` | Escrow identifier. |
| `status` | `ContractStatus` | Current escrow lifecycle status. |
| `total_amount` | `i128` | Total agreed milestone amount. |
| `funded_amount` | `i128` | Current escrow funding accounting value; refunds can reduce it, so it is not a lifetime deposit counter. |
| `released_amount` | `i128` | Amount accounted as released from the escrow. |
| `held` | `i128` | `funded_amount - released_amount`. |
| `remaining_to_fund` | `i128` | `total_amount - funded_amount`; this can be nonzero after a refund even for a terminal escrow. |
| `milestones_total` | `u32` | Number of milestones. |
| `milestones_released` | `u32` | Number whose `released` flag is true. |
| `next_unreleased` | `Option<u32>` | Zero-based index of the first milestone whose `released` flag is false, or `None` when all are released. |
| `next_release_affordable` | `bool` | Whether `held >= amount` for that first unreleased milestone; false when none exists. This is a balance comparison, not permission to release: status, auth, pause, freeze and other checks still apply. |

These formulas follow `summarize`; do not infer net paid amounts or release
eligibility from milestone flags alone. In particular, partial releases do not
make a milestone fully released, and the affordability comparison uses the
milestone's full `amount`.

## `EscrowReceipt` and compaction

`compact_escrow(id)` is permissionless, but the escrow must not be frozen and
must satisfy the terminal-state checks:

- `Cancelled`: no retained claimable balance (`funded_amount == released_amount`).
- `Completed` or `Resolved`: `reputation_issued` is true, or the recorded close
  ledger plus `THIRTY_DAYS_IN_LEDGERS` (`518_400`) has been reached. This is a
  ledger-sequence interval (approximately 30 days), not a timestamp duration.
- `Created`, `Funded` and `Disputed` are not compactable. An already compacted
  id also returns `NotCompactable`.

Compaction copies the final accounting fields into persistent
`DataKey::Receipt(id)`, extends its TTL, removes both escrow keys
(`DataKey::ContractV(id)` and legacy `DataKey::Contract(id)`) and removes
`DataKey::ClosedAt(id)`. It emits the inline event `("compact", id) -> id`.
The receipt replaces the full escrow representation; it does not contain the
milestone vector or every side entry. It is still subject to persistent-storage
TTL; `extend_escrow_ttl` can keep a receipt alive.

| Field | Rust type | Meaning |
| --- | --- | --- |
| `id` | `u64` | Compacted escrow identifier. |
| `client` | `Address` | Client address. |
| `freelancer` | `Address` | Freelancer address. |
| `token` | `Address` | Escrow token contract address. |
| `total_amount` | `i128` | Agreed total at compaction, in token base units. |
| `released_amount` | `i128` | Recorded amount released at compaction. |
| `refunded_amount` | `i128` | Recorded amount refunded when the escrow closed. |
| `status` | `ContractStatus` | Terminal status retained by compaction. |
| `milestone_count` | `u32` | Number of milestones in the removed escrow. |
| `closed_at` | `u64` | Closing ledger sequence, not Unix time; uses the compaction ledger when no close record exists. |
| `reputation_issued` | `bool` | Whether the client's freelancer rating had been issued at compaction. It may remain false after the grace period. |

## Event schema versioning

`EVENT_SCHEMA_V` is currently `1`. Structured events carry it as the first
topic, for example `(1, "created", id)` in the established event interface.
It identifies the topic/payload decoding schema, not the escrow storage layout,
summary layout, contract deployment version or a field in a summary/receipt.
Inline events such as `("compact", id)` do not carry this prefix; do not assume
every event starts with a version number.

For a change to an existing structured event's field names, types, meaning or
topic layout that would change how consumers decode it:

1. Increment `EVENT_SCHEMA_V` consistently at its source declarations and update
   emitters. Adding a field to a Soroban contract type must be reviewed as a
   decoding change; it is not automatically safe because it sounds optional.
2. Update payload/topic documentation, client/indexer decoders and event tests
   together. Pin the new version and payload shape in tests.
3. Publish a migration note describing the old/new payloads and the deployment
   ledger at which the version changes. Retain decoding of historical versions
   for replay; consumers should flag unknown versions rather than silently
   interpreting them as version 1.

Adding a distinct event name with its own payload need not change the meaning of
an existing event, but consumers must tolerate unknown event names. Changes to
inline events need an explicit compatibility/migration plan because those events
have no `EVENT_SCHEMA_V` discriminator. See the event tables in
[indexer-schema.md](indexer-schema.md) for the broader event catalog.

## Storage and read-schema evolution

Neither `EscrowSummary` nor `EscrowReceipt` has a schema-version field or a
summary-version getter. Do not use the event version to decode either struct.
Coordinate read-interface changes with generated bindings and indexers, document
the affected deployment and preserve compatibility or provide a replacement API
when a changed shape would break consumers.

Escrow storage uses `StoredEscrow::V1(EscrowContract)` under
`DataKey::ContractV(id)`. `load` prefers that envelope and falls back to a legacy
`EscrowContract` under `DataKey::Contract(id)`; reads do not write. `save` writes
the current envelope and removes the legacy key. Permissionless
`migrate_escrows` offers eager migration. See [migration.md](migration.md).

For a new stored layout, preserve old variants/types, add a new envelope variant
and conversion in `upgrade_to_latest`, make writes use the new variant, and test
old/new reads and lazy/eager migration. Receipts are stored directly under
`DataKey::Receipt(id)`, outside `StoredEscrow`; changing their persisted shape
requires its own backward-compatible reader or explicit migration strategy.
Bumping `EVENT_SCHEMA_V` alone migrates neither escrows nor receipts.

## Source status and verification

The tables match the checked-in struct definitions. API and compaction behavior
is covered by [`test.rs`](../../contracts/escrow/src/test.rs), including batch-cap,
compacted-read, terminal-state and grace-period cases. The upstream refactor in
PR #1842 removed these four entrypoint implementations and left invalid syntax
in `lib.rs`; the interface behavior above is corroborated by the implementation
immediately before that refactor (commit `9c3dd9a7a427053ccbbf141bbeae081d88dbf8be`).
Until the refactor is repaired, these tests cannot establish a working current
build. Check the deployed contract spec before integrating against a deployment.
