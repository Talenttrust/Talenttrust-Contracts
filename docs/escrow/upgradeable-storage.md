# Escrow Upgradeable Storage

The escrow contract keeps per-escrow state behind a versioned envelope so a
future layout can be introduced without rewriting existing on-ledger data. This
document describes what is implemented today in
`contracts/escrow/src/lib.rs`.

Storage versioning is separate from WASM code upgrades: `schedule_upgrade` /
`execute_upgrade` swap the contract code behind a 48-hour timelock and bump
`version()`, while the envelope described here governs how escrow records are
read and written.

## Current Layout

| Key | Value | Notes |
| --- | --- | --- |
| `DataKey::ContractV(id)` | `StoredEscrow::V1(EscrowContract)` | Current entry for every escrow created since the envelope was introduced. |
| `DataKey::Contract(id)` | `EscrowContract` | Legacy entry from before the envelope. Read-only for the contract; removed on migration. |
| `DataKey::Receipt(id)` | `EscrowReceipt` | Replaces both keys after `compact_escrow`. |
| `DataKey::Version` | `u32` | Contract code version, `1` until the first `execute_upgrade`. Not a storage-layout version. |

All other state (admin, pause flags, fees, reputation, per-escrow side keys such
as funders, team split, terms, delivery and milestone bookkeeping) is stored
under its own `DataKey` variant and is unaffected by the envelope.

## How a Version Is Applied

- `load` reads `ContractV(id)`, applies `upgrade_to_latest`, and only falls back
  to the legacy `Contract(id)` key when the versioned key is missing. Reads never
  write.
- `save` writes the current variant, extends the persistent TTL, deletes the
  legacy key and bumps the instance TTL. The first mutation of a legacy escrow
  therefore migrates it lazily.
- `migrate_escrows(ids)` performs the same step eagerly for up to
  `MAX_MIGRATE_BATCH` (50) ids per permissionless call, skipping ids that are
  already versioned or unknown, and returns how many it migrated.
- `extend_escrow_ttl` covers the versioned key, the legacy key and the receipt,
  so rent can be topped up for any shape of record.

Details, guarantees and the full test list are in
[`migration.md`](./migration.md).

## Rules

1. `StoredEscrow` variants are append-only. `#[contracttype]` encodes enum
   variants by position, so reordering or removing one changes how existing data
   decodes.
2. Old variants are never overwritten in place; they are converted by
   `upgrade_to_latest` and rewritten as the newest variant by `save`.
3. Legacy `Contract(id)` entries stay readable indefinitely; migration is an
   optimisation, not a prerequisite for correct reads.
4. Per-escrow side keys keep their own keys across versions, so migration only
   touches the escrow record itself.
5. Compaction is terminal: a compacted escrow keeps only its receipt and is
   never migrated.

## Adding a New Version

1. Append `V2(...)` to `StoredEscrow` (never reorder or remove `V1`).
2. Add its arm to `upgrade_to_latest`, keeping the `V1` conversion there.
3. Write the newest variant from `save`.
4. Add a test that plants a `V1` entry, mutates the escrow through a public
   entrypoint and asserts the stored value is `V2` with the previous fields
   preserved — `legacy_entry_is_migrated_on_first_mutation` is the model.
5. Run `cargo test` and `cargo build --target wasm32v1-none --release`, then
   update this document, `migration.md` and the interface table in `README.md`.

## Test Coverage

The versioned-storage tests live in `contracts/escrow/src/test.rs`:
`legacy_entry_is_migrated_on_first_mutation`,
`reads_on_legacy_entry_do_not_mutate_storage`,
`deposit_and_release_work_on_legacy_entry`, `migrate_escrows_is_idempotent`,
`migrate_escrows_skips_unknown_and_enforces_cap` and
`new_contracts_use_versioned_key`.
