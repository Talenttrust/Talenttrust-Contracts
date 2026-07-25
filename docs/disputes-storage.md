# Disputes Storage Layout and TTL Policy

## Overview

There is **no dedicated storage key for disputes**. A dispute is not a separate
record — it is a state carried entirely inside the existing per-contract
entry at `DataKey::Contract(contract_id)`. Raising a dispute flips that
entry's `status` field to `Disputed`; resolving a dispute updates its
`released_amount`/`refunded_amount` fields and moves `status` to `Completed`
or `Refunded`. No new key is ever created or removed as part of the dispute
lifecycle.

This matches the crate's own module-ownership map in `contracts/escrow/src/lib.rs`:

> `dispute` — Pure dispute payout arithmetic and final-status selection for
> dispute resolution. **None directly**; root dispute entrypoints update
> `DataKey::Contract(contract_id)`.

`contracts/escrow/src/dispute.rs` is explicitly storage-free (see its module
doc comment) — it only computes payout splits (`resolution_payouts`) and the
final status (`final_status_after_resolution`). All actual reads/writes
happen in the `raise_dispute` and `resolve_dispute` entrypoints in
`contracts/escrow/src/lib.rs`.

## Storage key and value shape

| | |
|---|---|
| **Key** | `DataKey::Contract(contract_id: u32)` |
| **Storage type** | `persistent()` |
| **Value type** | `Contract` (defined in `contracts/escrow/src/types.rs`) |

Fields on `Contract` relevant to disputes:

| Field | Type | Role in a dispute |
|---|---|---|
| `status` | `ContractStatus` | Set to `Disputed` by `raise_dispute`; set to `Completed` or `Refunded` by `resolve_dispute` |
| `arbiter` | `Option<Address>` | Must be `Some` for a dispute to be raised at all; must match the caller of `resolve_dispute` |
| `funded_amount` | `i128` | Read to compute the available balance (`funded_amount - released_amount - refunded_amount`) |
| `released_amount` | `i128` | Incremented by the freelancer's payout share on resolution |
| `refunded_amount` | `i128` | Incremented by the client's payout share on resolution |

No other fields on `Contract` are touched by the dispute flow, and no other
`DataKey` variant is read or written by either entrypoint — with one
exception, noted below under "Side effect on reputation storage."

The milestone vector, stored separately under
`(DataKey::Contract(contract_id), "milestones")`, is **not** read or written
by either dispute entrypoint, and its TTL is not extended by a dispute call.

## TTL / bump-on-access policy

Both dispute entrypoints use the same generic persistent-storage TTL policy
as the rest of the contract, defined in `contracts/escrow/src/ttl.rs`:

| Constant | Value | Meaning |
|---|---|---|
| `PERSISTENT_TTL_LEDGERS` | 518,400 ledgers (~30 days) | The TTL a persistent entry is extended *to* |
| `PERSISTENT_BUMP_THRESHOLD` | 120,960 ledgers (~7 days) | The remaining-TTL threshold below which an extension actually happens |

There is no dispute-specific TTL constant — disputes use the same
30-day/7-day policy as every other persistent `Contract` entry.

The mechanism is `ttl::extend_contract_ttl(env, contract_id)`, which calls
Soroban's `extend_ttl(key, threshold, extend_to)`. Per Soroban's semantics,
this only actually extends the entry's TTL if its *current* remaining TTL is
below `threshold` (7 days); otherwise it's a no-op. This means a contract
under active dispute back-and-forth doesn't get its TTL churned on every
call — only entries that are actually getting close to expiry are renewed.

**`extend_contract_ttl` is called twice in each dispute entrypoint** — once
immediately after reading the contract, and again immediately after writing
it back:

```rust
// raise_dispute (contracts/escrow/src/lib.rs)
let mut contract: Contract = env.storage().persistent()
    .get(&DataKey::Contract(contract_id))
    .unwrap_or_else(|| env.panic_with_error(Error::ContractNotFound));

ttl::extend_contract_ttl(&env, contract_id);   // bump #1: on read
Self::require_not_finalized(&env, contract_id);

// ... validation ...

contract.status = ContractStatus::Disputed;
env.storage().persistent().set(&DataKey::Contract(contract_id), &contract);

ttl::extend_contract_ttl(&env, contract_id);   // bump #2: on write
```

`resolve_dispute` follows the identical pattern: read → bump → validate →
mutate → write → bump. In practice this means any successful call to either
entrypoint gives the contract's persistent entry the best chance of renewal
available under the bump-on-read/write pattern, since it's checked both
before and after the state mutation.

## Eviction risk

If a contract's persistent entry is never touched by any entrypoint for
longer than `PERSISTENT_TTL_LEDGERS` (30 days), Soroban's host will evict it.
A dispute cannot be raised or resolved on an evicted contract — the initial
`env.storage().persistent().get(...)` in either entrypoint returns `None`,
and the entrypoint panics with `Error::ContractNotFound`, identical to the
entry never having existed at all. There is no special recovery path for a
disputed contract that has been evicted; this is the same fail-closed
behavior `ttl.rs`'s own module documentation describes for all persistent
entries.

## Gating on finalization

Both entrypoints call `Self::require_not_finalized(&env, contract_id)`
immediately after the TTL bump-on-read, before any dispute-specific
validation. This checks for the *presence* of `DataKey::Finalization(contract_id)`
(see `contracts/escrow/src/finalize.rs`) — a separate persistent key, owned by
the `finalize` module, not by disputes. If that key exists, both entrypoints
panic with `Error::AlreadyFinalized`. Disputes never read or write the
finalization key's value directly; they only cause `require_not_finalized`
to check whether it's present.

## Side effect on reputation storage

When `resolve_dispute` results in `ContractStatus::Completed`, it calls
`grant_pending_reputation_credit`, which reads and writes
`DataKey::PendingReputationCredits(freelancer_address)` (persistent),
incrementing a pending-credit counter by one. This is a real side effect of
resolving a dispute, so it's noted here for completeness — but it is **not**
part of the disputes storage domain; `PendingReputationCredits` is owned by
the reputation system.

Worth flagging separately: at the time of writing, no code path anywhere in
the crate calls an explicit TTL-extend on `PendingReputationCredits` — not in
`resolve_dispute`, nor in the other two call sites (`lib.rs:1737`,
`release.rs:125`). This key relies entirely on whatever default TTL Soroban
assigns on `.set()`, with no renewal. This is a pre-existing characteristic
of the reputation system, unrelated to the dispute flow's own TTL handling,
and out of scope for this document — flagged here only because it's visible
from the dispute code path.

## Events (not storage)

Both entrypoints publish events — `("dispute", "opened")` from
`raise_dispute` and `("dispute", "resolved")` from `resolve_dispute` — for
off-chain indexers. These are Soroban's ephemeral event mechanism, not
contract storage; they are not persisted state and have no TTL or bump
policy of their own.

## Summary table

| Aspect | Detail |
|---|---|
| Dedicated dispute key | None |
| Key actually used | `DataKey::Contract(contract_id)` |
| Storage type | `persistent()` |
| TTL extend-to | 30 days (`PERSISTENT_TTL_LEDGERS`) |
| Bump threshold | 7 days (`PERSISTENT_BUMP_THRESHOLD`) |
| Bump calls per entrypoint | 2 (on read, on write) |
| Milestone vector touched? | No |
| Finalization key touched? | Read-only presence check (gate), not written |
| Side effect on other storage | `PendingReputationCredits` incremented on `Completed` outcome (no TTL management) |
