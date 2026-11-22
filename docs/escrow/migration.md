# Storage Migration

Each escrow is stored under a versioned envelope, and pre-versioning entries are
moved onto it either lazily (first mutation) or eagerly (`migrate_escrows`). No
admin transaction is required. Everything below lives in
`contracts/escrow/src/lib.rs`.

## Keys

| Key | Value | Purpose |
| --- | --- | --- |
| `DataKey::Contract(id)` | `EscrowContract` | Legacy entry written before the envelope existed. Still readable, no longer written. |
| `DataKey::ContractV(id)` | `StoredEscrow` | Current entry. |
| `DataKey::Receipt(id)` | `EscrowReceipt` | What a compacted escrow keeps instead of both escrow keys. |

An escrow id has at most one live representation: a legacy `Contract(id)`, a
versioned `ContractV(id)`, or — once `compact_escrow` has run — only
`Receipt(id)`. Per-escrow side entries (`Funders`, `Team`, `Terms`, ratings,
milestone bookkeeping, ...) are separate `DataKey` variants and are not touched
by migration.

## The envelope

`StoredEscrow` wraps `EscrowContract` and has one variant today:

```rust
pub enum StoredEscrow {
    V1(EscrowContract),
}
```

`upgrade_to_latest(StoredEscrow) -> EscrowContract` is the single place that
converts historical variants into the current struct, so every reader goes
through one conversion point.

## Reading and writing

- `load` reads `ContractV(id)` first and passes it through `upgrade_to_latest`;
  when that key is absent it falls back to the legacy `Contract(id)`. Reads never
  write — see `reads_on_legacy_entry_do_not_mutate_storage`.
- `save` performs the lazy migration. It writes `StoredEscrow::V1(current)` under
  `ContractV(id)`, extends that entry's persistent TTL, removes the legacy
  `Contract(id)` key if it exists, and bumps the instance TTL. Any mutation of a
  legacy escrow therefore migrates it in the same transaction — see
  `legacy_entry_is_migrated_on_first_mutation` and
  `deposit_and_release_work_on_legacy_entry`.
- New escrows are written straight to `ContractV(id)`; nothing creates legacy
  entries any more (`new_contracts_use_versioned_key`).
- `compact_escrow` removes both `ContractV(id)` and `Contract(id)` and stores
  `Receipt(id)` instead, so terminal escrows never need migration.
- `extend_escrow_ttl` extends whichever of `ContractV(id)`, `Contract(id)` or
  `Receipt(id)` exists, so the keep-alive entrypoint works for migrated, legacy
  and compacted escrows alike.

## Eager migration: `migrate_escrows`

`migrate_escrows(ids) -> u32` moves legacy entries onto `ContractV` without
waiting for a mutation. It is permissionless: no auth is required and the caller
pays the rent for the new entries.

- Accepts at most `MAX_MIGRATE_BATCH` (50) ids per call; more returns
  `InvalidAmount`.
- Ids already stored under `ContractV` are skipped, so calling it repeatedly is
  safe (`migrate_escrows_is_idempotent`).
- Ids with no entry at all are skipped, not an error
  (`migrate_escrows_skips_unknown_and_enforces_cap`).
- For every legacy entry it calls `save`, which writes the `V1` envelope, drops
  the old key and extends the TTL.
- Returns the number of entries migrated by this call, and emits
  `("migrated",) -> migrated` when that count is non-zero.

Lazy migration alone is sufficient for correctness; `migrate_escrows` only lets
an operator front-run TTL expiry on escrows nobody touches.

## Adding `StoredEscrow::V2`

Checklist for a future layout change:

1. Add the new struct (or the new fields) and append a variant
   `V2(NewEscrowContract)` to `StoredEscrow`. Append only: `#[contracttype]`
   enum variants are encoded by position, so reordering or removing `V1` changes
   what existing on-chain data decodes to.
2. Add the `V2` arm to `upgrade_to_latest` and keep the `V1` conversion there
   (`V1(old) => convert(old)`); that function must stay the only place old
   variants are interpreted.
3. Change `save` to write the newest variant (`StoredEscrow::V2(...)`), leaving
   its TTL extension and legacy-key removal in place.
4. Keep `load` reading `ContractV` before the legacy `Contract` key so both old
   shapes remain readable.
5. Add tests mirroring `legacy_entry_is_migrated_on_first_mutation`: plant a
   `V1` envelope under `ContractV`, read it through `get_contract`, mutate it,
   and assert the stored value is now `V2` with the old fields preserved.
6. Run `cargo test` and `cargo build --target wasm32v1-none --release`.
7. Update this document, `upgradeable-storage.md` and the interface table in
   `README.md`.

## Test coverage

Versioned storage and migration are covered in `contracts/escrow/src/test.rs`:

- `legacy_entry_is_migrated_on_first_mutation`
- `reads_on_legacy_entry_do_not_mutate_storage`
- `deposit_and_release_work_on_legacy_entry`
- `migrate_escrows_is_idempotent`
- `migrate_escrows_skips_unknown_and_enforces_cap`
- `new_contracts_use_versioned_key`
