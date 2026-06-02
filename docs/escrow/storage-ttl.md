# Escrow Storage TTL

This document defines the deterministic, auditable TTL (time-to-live) policy for
**transient** storage entries in the escrow contract. It exists to prevent
unbounded state growth from orphaned pending approvals and pending migrations
that are never resolved by counterparties.

See also: [state-persistence.md](./state-persistence.md) for the persistent
storage model; [upgradeable-storage.md](./upgradeable-storage.md) for upgrade
semantics.

## Scope

Applies to keys stored in `env.storage().temporary()`. Persistent keys (e.g.
`Contract(id)`, `NextId`) are unaffected — their TTL management is covered in
[architecture.md](./architecture.md).

## Units

All TTL values are denominated in **ledgers**, the Soroban-native unit. One
ledger is ~5 seconds on Stellar mainnet. This avoids any coupling to
wall-clock timestamps and keeps expiry deterministic as a function of
`env.ledger().sequence()`.

| Named constant | Ledgers | Rough duration |
|---|---:|---|
| `LEDGERS_PER_DAY` | 17 280 | 1 day |
| `PENDING_APPROVAL_TTL_LEDGERS` | 120 960 | 7 days |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 17 280 | 1 day |
| `PENDING_MIGRATION_TTL_LEDGERS` | 362 880 | 21 days |
| `PENDING_MIGRATION_BUMP_THRESHOLD` | 51 840 | 3 days |

Constants live in
[contracts/escrow/src/ttl.rs](../../contracts/escrow/src/ttl.rs).

## Transient Keys

| Key | TTL | Bump threshold | Rationale |
|---|---:|---:|---|
| `PendingApproval(contract_id)` | 7 days | 1 day | Counterparties are expected to respond within one business week; short enough to reclaim state on abandonment. |
| `PendingMigration` | 21 days | 3 days | Migrations are rarer and more consequential; reviewers need more lead time. |

`PendingMigration` is a **single-slot** key: at most one migration may be
pending at any time.

## TTL Helper API

All transient reads and writes go through the helpers in `contracts/escrow/src/ttl.rs`:

| Function | Description |
|---|---|
| `compute_expiry(env, ttl)` | Returns `sequence + ttl` (saturating). |
| `store_with_ttl(env, key, value, ttl)` | Writes to temporary storage and sets TTL. |
| `read_if_live(env, key)` | Returns `Some(v)` if live, `None` if absent or evicted. |
| `extend_if_below_threshold(env, key, threshold, extend_to)` | Bumps TTL; returns `false` if key absent. |
| `remove_transient(env, key)` | Explicit removal before auto-eviction. |
| `has_transient(env, key)` | Returns `true` if the key is currently live. |

## Expiry Semantics

- Soroban auto-evicts temporary storage entries once their TTL has elapsed.
- `read_if_live` returns `None` for both "never set" and "expired" — callers
  treat both as "no active pending record".
- No on-chain event is emitted at auto-eviction. Off-chain indexers should
  compute eviction by comparing `expires_at_ledger` against the current ledger
  sequence.

## Determinism

Expiry is computed at write time as:

```
expires_at_ledger = env.ledger().sequence() + TTL_LEDGERS
```

Given the same starting sequence and the same TTL constant, two independent
environments produce identical expiry values. This is verified by
`expiry_is_deterministic_across_independent_envs` in the test suite.

## Extending (Bumping) TTL

`extend_if_below_threshold` wraps
`env.storage().temporary().extend_ttl(key, threshold, extend_to)`:

- If remaining TTL is **below** the bump threshold, the entry's TTL is
  extended to the full policy value.
- If the entry is already fresh, the call is a no-op (Soroban only extends,
  never shrinks).
- If the entry is absent or already evicted, the helper returns `false` and
  performs no write.

## Security Notes

- All writes use `store_with_ttl`; no direct `.temporary().set` bypass is
  permitted, ensuring TTL is always set at write time.
- `remove_transient` is used for explicit cleanup (e.g. after an approval is
  consumed or cancelled) so stale entries do not linger until auto-eviction.
- The fail-closed design means a `None` from `read_if_live` always blocks the
  dependent operation, regardless of whether the entry expired or was never
  created.

## Testing

Tests live in
[contracts/escrow/src/test/ttl_tests.rs](../../contracts/escrow/src/test/ttl_tests.rs).
They call the TTL helpers directly via `env.as_contract` and advance
`LedgerInfo.sequence_number` via `env.ledger().with_mut(...)` to simulate
auto-eviction.

| Test | What it covers |
|---|---|
| `compute_expiry_equals_sequence_plus_ttl` | `compute_expiry` returns correct value for both TTL constants |
| `compute_expiry_saturates_on_overflow` | Saturating addition at `u32::MAX` |
| `ledgers_per_day_constant_is_correct` | All five constants match their documented values |
| `approval_readable_before_expiry` | `read_if_live` returns `Some` one ledger before approval TTL |
| `approval_evicted_after_expiry` | `read_if_live` returns `None` one ledger after approval TTL |
| `migration_readable_before_expiry` | `read_if_live` returns `Some` one ledger before migration TTL |
| `migration_evicted_after_expiry` | `read_if_live` returns `None` one ledger after migration TTL |
| `extend_returns_false_for_absent_key` | `extend_if_below_threshold` returns `false` when key absent |
| `extend_returns_true_and_entry_survives_past_original_expiry` | Bump keeps entry live past original expiry |
| `extend_migration_returns_false_for_absent_key` | Same absent-key check for migration threshold |
| `remove_transient_clears_entry_immediately` | Entry absent after `remove_transient` |
| `remove_transient_is_idempotent` | Second `remove_transient` does not panic |
| `has_transient_false_before_store` | `has_transient` returns `false` before any write |
| `has_transient_true_after_store_false_after_expiry` | `has_transient` tracks live/evicted state |
| `expiry_is_deterministic_across_independent_envs` | Same starting sequence → same expiry in two envs |

## Reviewer Checklist

1. Every new transient key has an entry in the table above.
2. Every write uses `ttl::store_with_ttl` (no direct `.temporary().set` bypass).
3. Every read path uses `ttl::read_if_live` and handles `None` as "absent or expired".
4. A corresponding TTL test exists when a new transient key is introduced.
