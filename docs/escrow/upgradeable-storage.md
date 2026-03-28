# Escrow Upgradeable Storage Planning

This document describes the versioned storage key strategy used by the escrow contract.

## Objective

Define migration-safe storage layouts that allow future upgrades without breaking existing on-ledger data.

## Current Layout

The active layout is `V1` (version `1`).

Storage is namespaced by key families:

- `meta_v1`
  - `LayoutVersion`
  - `NextContractId`
- `escrow_v1`
  - `Contract(contract_id)`
- `rep_v1`
  - `Reputation(freelancer_address)`

These namespaces are exposed via `storage_layout_plan()`.

## Migration-Safe Design Rules

1. `V1` keys and value layouts are immutable once deployed.
2. Future upgrades must add new version key variants (for example `V2(...)`) rather than mutating `V1` key/value formats in place.
3. `LayoutVersion` metadata is checked before all state reads/writes.
4. Unknown on-ledger layout versions are rejected with `UnsupportedStorageVersion`.
5. Migration entrypoint `migrate_storage(target_version)` is explicit and rejects unsupported targets.

## Initialization Behavior

`get_storage_version`, state-mutating functions, and state-read functions call internal layout initialization/checks:

- if `LayoutVersion` is missing: initialize to `1`
- if present and `1`: continue
- if present and unsupported: fail safely

This ensures deterministic startup behavior and protects against accidental cross-version decoding.

## Test Coverage

Storage-planning coverage is implemented in `contracts/escrow/src/test/storage.rs`:

- version default initialization
- migration no-op to current version
- migration rejection for unsupported targets
- stable namespace reporting
- preservation of existing data across migration no-op

## Latest Local Test Output

Date: `2026-03-23`

```text
running 23 tests
.......................
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Coverage Snapshot

- `cargo llvm-cov --workspace --all-features --summary-only`
- `contracts/escrow/src/lib.rs` line coverage: `95.18%`
- workspace total line coverage: `97.76%`
