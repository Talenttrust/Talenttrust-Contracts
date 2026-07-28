# `now_seconds` — Ledger Time Source

## Overview

`utils::now_seconds` is the **single source of truth** for wall-clock time in the
TalentTrust escrow contract.  Every entrypoint that needs absolute time must
call this helper; direct `env.ledger().timestamp()` calls outside `utils.rs` are
forbidden.

```rust
// contracts/escrow/src/utils.rs
pub fn now_seconds(env: &Env) -> u64 {
    env.ledger().timestamp()
}
```

## Precision and trust assumptions

### How ledger timestamps work

Stellar validator nodes embed a timestamp (seconds since Unix epoch) in every
closed ledger.  The timestamp is:

- **Consensus-driven** — all validators in the SCP quorum agree on the same
  value.  No single user or validator can unilaterally manipulate it.
- **Coarse-grained** — a new ledger closes roughly every 5 seconds, so the
  effective resolution is ~5 s.  Consecutive ledgers may share the same
  timestamp value.
- **Not an atomic clock** — each validator uses its own system clock.  While
  Stellar Core rejects timestamps that drift too far from the network median,
  there is no sub-second synchronisation.

### What this means for deadlines

| Deadline granularity | Safe? | Notes |
| --- | --- | --- |
| Minutes or hours | ✅ Yes | One-ledger jitter is insignificant. |
| Tens of seconds (~30 s) | ⚠️ Borderline | At least 6 ledgers; usable but avoid exact-second expectations. |
| A few seconds (≤ 10 s) | ❌ No | Timestamp may not advance between two consecutive ledgers. Non-deterministic. |

**Golden rule**: never use `now_seconds` for deadlines shorter than ~30 seconds.
For short timing windows, use **ledger-sequence counts**
(`env.ledger().sequence()`) and TTL-based expiration instead.

## Call sites

Every use of `now_seconds` and `env.ledger().timestamp()` in the contract is
catalogued below.

### `now_seconds` callers (must use the helper)

| Entrypoint | Module | Purpose |
| --- | --- | --- |
| `is_milestone_overdue` | `lib.rs` | Returns `true` when `now_seconds(&env) > deadline` (strictly greater). This is the precondition for the timeout-refund path in `refund_unreleased_milestones`. |

### Direct `env.ledger().timestamp()` callers (permitted for events only)

Public Soroban events stamp an informational `timestamp` for off-chain
indexers. These are not semantic time checks and read the ledger directly:

| Entrypoint | Event emitted |
| --- | --- |
| `initialize` | `init` / `admin_set` |
| `bind_settlement_token` | `settlement_token_bound` |
| `release_milestone` | `mlstn_rls`, `ctrct_cmp`, `ctrct_st` |
| `refund_unreleased_milestones` | `refunded`, `ctrct_st` |
| `activate_emergency_pause` | `pause` |
| `resolve_emergency` | `unpaused` |
| `set_protocol_fee_bps` | `protocol_fee_bps` |
| `propose_governance_admin_impl` | `admin` / `proposed` |
| `accept_governance_admin_impl` | `admin` / `accepted` |
| `cancel_governance_admin_proposal_impl` | `admin` / `cancelled` |
| `accept_client_migration_impl` | `client_migration_accepted` |
| `cancel_client_migration_impl` | `client_migration_cancelled` |
| `create_contract` (via `create_contract.rs`) | `contract_created` |
| `deposit_funds` (via `apply_validated_deposit`) | `deposit_success` |
| `finalize_contract` (via `finalize.rs`) | `contract_finalized` |

### Ledger-sequence-based mechanisms (NOT using `now_seconds`)

These features measure **elapsed ledgers**, not wall-clock time:

| Mechanism | Module | Detail |
| --- | --- | --- |
| Admin rotation timelock | `governance.rs` | Uses `env.ledger().sequence()` to enforce `ADMIN_ROTATION_MIN_DELAY_LEDGERS` (~2 days in ledgers). |
| Migration TTL | `migration.rs` | Uses `env.ledger().sequence()` to stamp `requested_at_ledger` and `expires_at_ledger`; eviction happens via Soroban temporary-storage TTL. |
| Approval expiry | `approvals.rs` | Temporary-storage TTL (`PENDING_APPROVAL_TTL_LEDGERS`). |
| Persistent storage renewal | `ttl.rs` | Bump-on-read thresholds expressed in ledger counts. |

## Testing — deterministic time control

### `env.ledger().with_mut()` pattern

Tests that exercise time-dependent logic use the Soroban test-utils `Ledger`
trait to set the ledger timestamp directly:

```rust
use soroban_sdk::testutils::Ledger;

fn set_now(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = secs;
    });
}
```

After calling `set_now`, the next `now_seconds(&env)` call returns `secs`.

### Worked example: milestone overdue boundaries

This is the test pattern used in `contracts/escrow/src/test/timeout_tests.rs`.
It verifies the strict-inequality semantics of `is_milestone_overdue`:

```rust
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Env, Symbol, Vec as SorobanVec,
};
use crate::{DataKey, Milestone};

fn set_now(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = secs;
    });
}

/// Overwrite a milestone's deadline and released flag in storage.
fn set_milestone_deadline_and_released(
    env: &Env,
    contract_addr: &Address,
    contract_id: u32,
    index: u32,
    deadline: Option<u64>,
    released: bool,
) {
    env.as_contract(contract_addr, || {
        let key = (DataKey::Contract(contract_id), Symbol::new(env, "milestones"));
        let mut milestones: SorobanVec<Milestone> =
            env.storage().persistent().get(&key).unwrap();
        let mut m = milestones.get(index).unwrap();
        m.deadline = deadline;
        m.released = released;
        milestones.set(index, m);
        env.storage().persistent().set(&key, &milestones);
    });
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn overdue_false_when_now_before_deadline() {
    let env = Env::default();
    // ... contract setup, milestone creation ...
    let deadline = 1_000u64;
    set_milestone_deadline_and_released(&env, &client_addr, id, 0, Some(deadline), false);

    set_now(&env, deadline - 1);  // now < deadline
    assert!(!client.is_milestone_overdue(&id, &0));
}

#[test]
fn overdue_false_at_exact_deadline() {
    // ... setup ...
    set_now(&env, deadline);  // now == deadline
    assert!(
        !client.is_milestone_overdue(&id, &0),
        "now == deadline must not be overdue (uses strict >)"
    );
}

#[test]
fn overdue_true_one_second_past_deadline() {
    // ... setup ...
    set_now(&env, deadline + 1);  // now > deadline
    assert!(client.is_milestone_overdue(&id, &0));
}
```

### The `LedgerInfo` struct (alternative, full-overwrite approach)

For tests that need to set the complete ledger state at once (including
`sequence_number`, `protocol_version`, `network_id`, etc.), use
`env.ledger().set()`:

```rust
use soroban_sdk::testutils::{Ledger, LedgerInfo};

env.ledger().set(LedgerInfo {
    timestamp: 1_700_000_000,
    protocol_version: 20,
    sequence_number: 100,
    network_id: Default::default(),
    base_reserve: 10,
    min_temp_entry_ttl: 16,
    min_persistent_entry_ttl: 4096,
    max_entry_ttl: 3110400,
});
```

**Prefer `with_mut`** when you only need to change the timestamp — it avoids
accidentally resetting sequence numbers or TTL fields.

## Security considerations

1. **Users cannot manipulate time.**  `now_seconds` reads consensus state, not
   a user-supplied argument.  There is no exploit path where a caller sets
   the timestamp to bypass a deadline.
2. **Strict inequality for deadlines.**  `is_milestone_overdue` uses `>` (not
   `>=`), so at exactly the deadline the milestone is NOT overdue.  This
   prevents premature timeout refunds by one ledger.
3. **No off-chain clock dependency.**  Tests never read the system clock; all
   time is injected via `env.ledger().set()` or `with_mut()`.  This keeps
   tests deterministic and reproducible on any machine.
4. **Ledger-sequence for timelocks.**  The admin rotation timelock measures
   elapsed ledgers (`env.ledger().sequence()`), not seconds.  This is resistant
   to timestamp skew across validators and cannot be "fast-forwarded" by a
   validator with a slightly-ahead clock.

## Related documentation

- [`TIME_MANAGEMENT.md`](../../docs/TIME_MANAGEMENT.md) — higher-level time management overview.
- [`timeout_tests.rs`](../../contracts/escrow/src/test/timeout_tests.rs) — boundary tests for `is_milestone_overdue`.
- [`utils.rs`](../../contracts/escrow/src/utils.rs) — the `now_seconds` definition.
- [`ttl.rs`](../../contracts/escrow/src/ttl.rs) — TTL constants and bump-on-read helpers.
- [`governance.rs`](../../contracts/escrow/src/governance.rs) — admin rotation timelock.
- [`migration.rs`](../../contracts/escrow/src/migration.rs) — client migration TTL.
