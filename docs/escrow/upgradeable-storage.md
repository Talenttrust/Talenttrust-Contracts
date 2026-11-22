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


## Golden XDR Serialization Vectors Policy

All persisted storage keys (`DataKey`) and stored contract types (`StoredEscrow::V1(EscrowContract)`, `EscrowReceipt`, `Panel`, `Stream`, `Amendment`, `SettlementProposal`, `PendingUpgrade`, `Reputation`, `WeightedReputation`, `Outcomes`, `RatingRecord`, `ContractStatus`, `Role`) are pinned to golden base64 XDR vectors in `contracts/escrow/src/test.rs`:

- `golden_datakey_xdr_vectors` — pins every `DataKey` variant and enforces exhaustiveness via `assert_datakey_exhaustive`.
- `golden_storage_types_xdr_vectors` — pins every persisted struct and enum using fixed addresses and deterministic values.

### Rules for Contributors

1. **Never edit an existing golden vector in place** to make a failing test pass. A mismatch means a **storage-breaking change** that will corrupt or orphan live on-ledger state after an upgrade.
2. **Adding new storage**:
   - Adding a new `DataKey` variant is backwards-compatible; append the new variant and add a corresponding golden vector entry in `golden_datakey_xdr_vectors` (and `assert_datakey_exhaustive`).
   - Evolving an existing persisted struct (such as `EscrowContract`) requires adding a new versioned envelope variant (for example `StoredEscrow::V2(...)`), updating `upgrade_to_latest` / `migrate_escrows`, and pinning the new variant alongside the unchanged `V1` vector.
3. **Never rename, remove, or change field types** on existing `DataKey` variants or persisted structs/enums without a documented migration plan.
4. **Verification**: Run the golden vector suite before opening a PR:

```bash
cargo test -p escrow golden
```
