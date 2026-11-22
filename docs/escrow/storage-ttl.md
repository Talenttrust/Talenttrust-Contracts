# Escrow Storage TTL

This document defines the deterministic, auditable TTL (time-to-live) policy
for the escrow contract's **persistent** storage entries. The contract holds no
temporary/transient storage: escrow state, side entries and reputation records
live under `env.storage().persistent()`, and the contract instance state under
`env.storage().instance()`. The policy below keeps that state from archiving.

See also: [state-persistence.md](./state-persistence.md) for the persistent
storage model; [upgradeable-storage.md](./upgradeable-storage.md) for upgrade
semantics.

See also: [storage-validation.md](./storage-validation.md) for the invariant
validation that guards the state model described below.

## Constants

The policy is defined by two constants in
[contracts/escrow/src/lib.rs](../../contracts/escrow/src/lib.rs#L86-L87):

| Named constant | Value (ledgers) | Rough duration |
|---|--:|---|
| `PERSISTENT_TTL_THRESHOLD` | `17_280` | ~1 day |
| `PERSISTENT_TTL_EXTEND` | `518_400` | ~30 days |

`PERSISTENT_TTL_THRESHOLD` is the point below which a live entry is considered
stale and eligible for a bump. `PERSISTENT_TTL_EXTEND` is the TTL that a bumped
entry is extended to. Both are `u32`.

A third constant shares the ~30-day value but is unrelated to this policy:
`THIRTY_DAYS_IN_LEDGERS: u32 = 518_400` is the compaction grace period after
which an unrated terminal escrow may be compacted.

## Units

All TTL values are denominated in **ledgers**, the Soroban-native unit. One
ledger is ~5 seconds on Stellar mainnet, so `17_280` ledgers ≈ 1 day and
`518_400` ledgers ≈ 30 days. This avoids any coupling to wall-clock timestamps
and keeps expiry deterministic as a function of `env.ledger().sequence()`.

## When TTLs Are Bumped

Soroban's `extend_ttl(key, threshold, extend_to)` only extends an entry whose
remaining TTL is **below** `threshold`, and never shrinks it. All extensions in
this contract pass `(PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND)`.

### Writes: `save`

[`save`](../../contracts/escrow/src/lib.rs#L653-L666) writes the current escrow
under the versioned `ContractV(id)` key and then bumps that key's TTL to
`PERSISTENT_TTL_EXTEND`. It drops any legacy `Contract(id)` entry and finishes
with an instance bump. Because the bump target is `PERSISTENT_TTL_EXTEND`, an
escrow that is read and re-saved is refreshed to the full ~30 days.

### `extend_if_present`

[`extend_if_present`](../../contracts/escrow/src/lib.rs#L669-L678) is the shared
helper for side entries: if the persistent key exists it bumps the TTL to
`PERSISTENT_TTL_EXTEND` and returns `true`; otherwise it performs no write and
returns `false`. Keeper entrypoints use the return value to decide whether any
tracked key was found.

### Instance bump: `bump_instance`

[`bump_instance`](../../contracts/escrow/src/lib.rs#L680-L684) extends the
contract instance entry:

```rust
env.storage()
    .instance()
    .extend_ttl(PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
```

It keeps the instance keys (`Admin`, `NextId`, pause flags) alive. It is called
on **every gated call** via
[`require_not_paused`](../../contracts/escrow/src/lib.rs#L576-L583), and also by
[`require_admin`](../../contracts/escrow/src/lib.rs#L502-L508), `save`,
`extend_escrow_ttl` and `extend_reputation_ttl`.

## Keeper Entrypoints

Both keepers are permissionless: they require no authorization beyond the
submitting transaction, and **the caller pays the rent extension**. They only
lengthen the TTL of existing keys; they never create state.

### `extend_escrow_ttl(id)`

[`extend_escrow_ttl`](../../contracts/escrow/src/lib.rs#L3528-L3572) is the
per-escrow keep-alive. It bumps, whenever present:

- the escrow entry itself — `ContractV(id)`, legacy `Contract(id)`, or the
  compacted `Receipt(id)` once the escrow has been compacted;
- per-escrow side entries: `FundBy`, `Funders`, `Frozen`, `Payout`, `Team`,
  `ClosedAt`, `Sequential`, `Terms`, `MilestoneSpecs`, `DisputeBond`,
  `Settlement`, `ReviewWindow`, `EscrowFee`, `Bonus`, `Votes`, `ChangeOrder`,
  `Resigned`, `Rating`, `RatingWeight`, `ClientRated`, `Stream`, `Amendment`,
  `AmendNonce`;
- per-milestone entries for each milestone index `i`: `Submitted(id, i)`,
  `Delivery(id, i)`, `MilestonePaid(id, i)`;
- and finally the contract instance.

If none of `ContractV(id)`, `Contract(id)` or `Receipt(id)` exist, the call
returns `Err(EscrowError::ContractNotFound)`.

### `extend_reputation_ttl(freelancer)`

[`extend_reputation_ttl`](../../contracts/escrow/src/lib.rs#L3576-L3588) is the
per-address reputation keep-alive. It bumps, whenever present:
`Reputation(freelancer)`, `WeightedRep(freelancer)`,
`ClientReputation(freelancer)`, `Outcomes(freelancer)` and
`BonusTotal(freelancer)`, then bumps the contract instance. It has no error
path — absent keys are simply skipped.

## Expiry Semantics

- Persistent entries that lapse are **archived**, not deleted: they can be
  restored and remain the source of truth. This is why the keepers above exist
  — they keep live escrows and reputation records from archiving.
- Expiry is a function of ledger sequence only; no wall-clock timestamp is
  involved, so a given starting sequence and constant pair always yields the
  same result across independent environments.
- `extend_ttl` is monotonic (extend-only): a fresh entry is left untouched.

## Testing

TTL behaviour is covered in
[contracts/escrow/src/test.rs](../../contracts/escrow/src/test.rs):

| Test | What it covers |
|---|---|
| `extend_escrow_ttl_keeps_quiet_escrow_and_side_keys_alive` | After a long quiet jump the escrow, `FundBy`/`Funders` side keys and the instance are all bumped back above `PERSISTENT_TTL_THRESHOLD`; an absent id returns `ContractNotFound`. |
| `extend_reputation_ttl_keeps_reputation_alive` | Reputation entries are bumped back above `PERSISTENT_TTL_THRESHOLD` after the same quiet jump. |
| persistent-TTL assertions | Written side keys (`Terms`, `MilestoneSpecs`, `Delivery`) report a TTL of at least `PERSISTENT_TTL_THRESHOLD`. |

## Reviewer Checklist

1. New persistent keys are bumped by the relevant keeper (`extend_escrow_ttl`
   for per-escrow keys, `extend_reputation_ttl` for reputation keys).
2. Any new gated entry point bumps the instance (via `require_not_paused` or
   `require_admin`).
3. TTL extensions use `(PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND)`; no
   bare constants.
