# Concurrent-Execution Hardening — `storage_validation.rs` (issue #1535)

This document describes the concurrency and persisted-state hardening that lives
in [`contracts/escrow/src/storage_validation.rs`](../../contracts/escrow/src/storage_validation.rs)
and is enforced by the escrow money-flow entrypoints.

## Why this matters

`storage_validation.rs` is the single source of truth for the bounds checks that
every storage-mutating entrypoint runs before it writes. Those checks validate
*inputs*. They say nothing about the *state* the entrypoint is about to mutate,
and nothing stops a second execution from starting while the first is still in
flight. Both gaps can produce stale, unsafe, or inconsistent results that are
invisible until funds have already moved.

## Execution model

Soroban executes one transaction at a time and rolls back every storage write
when a call panics. A classic multi-threaded data race is therefore impossible.
The two hazards that *are* reachable are:

1. **Re-entrancy** — the escrow calls `token::Client::transfer` on the bound
   settlement token. A malicious token can call back into the escrow while that
   transfer is in flight. The re-entrant call is a fresh invocation that reads
   whatever is persisted *at that moment*; for flows that batch several
   transfers (for example `release_milestone_batch`) that is not the final state.
2. **Corrupt or stale persisted state** — a record that violates an accounting or
   milestone invariant would otherwise be loaded, mutated, and written back,
   laundering the corruption into a record that looks valid.

## The per-contract mutation lock

A single persistent entry serialises mutation of one contract:

| Item | Value |
| --- | --- |
| Key | `DataKey::ContractMutationLock(contract_id)` |
| Value | `bool` (`true` while a mutation is in flight) |
| TTL | `MUTATION_LOCK_TTL_LEDGERS` = `17_280` ledgers (1 day) |

`storage_validation::ContractMutationGuard::acquire` writes the entry and panics
with `ConcurrentMutation` when it is already present. The guard releases the
entry in `Drop`, so every exit path — including early returns — clears it.

The TTL is defensive only: the lock lives for the duration of a single entrypoint
call, and a transaction that traps rolls its own write back. The TTL guarantees
that an entry orphaned by a host-level failure can never lock a contract forever.

Locks are keyed by contract, so two different escrow contracts never block each
other.

### Guarded entrypoints

| Entrypoint | Guarded | Checked load | Checked store |
| --- | --- | --- | --- |
| `deposit_funds` | yes | — (record validated via `ValidatedDeposit`) | — (owned by `deposit.rs`) |
| `release_milestone` | yes | `load_contract_checked` | `store_contract_checked` |
| `release_milestone_batch` | yes | `load_contract_checked` | `store_contract_checked` |
| `refund_unreleased_milestones` | yes | validated after `require_active_contract` | `store_contract_checked` |
| `cancel_contract` | yes | `load_contract_checked` | `store_contract_checked` |

Read-only entrypoints (`get_contract`, `contract_exists`, simulate views) are
intentionally **not** guarded: they mutate nothing and must stay callable while a
mutation is in flight.

## State invariants

`validate_contract_accounting` refuses a `Contract` unless:

- `total_deposited`, `funded_amount`, `released_amount`, and `refunded_amount`
  are all non-negative; and
- `released_amount + refunded_amount <= funded_amount` (the sum is computed with
  checked arithmetic, so a corrupted pair of maximum `i128`s fails with
  `PotentialOverflow` instead of wrapping).

`validate_milestone_consistency` refuses a `Milestone` unless:

- `amount`, `funded_amount`, and `refunded_amount` are all non-negative;
- `released` and `refunded` are mutually exclusive (the same rule
  `milestone_transitions::MilestoneState::from_milestone` enforces); and
- a refunded milestone is refunded in full — `refunded_amount == amount`.

These are invariants of healthy operation, not new business rules: a healthy
contract can never trip them, so the guard only ever fires on corrupted or
partially-applied state.

## Failure modes

| Condition | Error | Code |
| --- | --- | --- |
| Re-entrant or interleaved mutation of the same contract | `ConcurrentMutation` | 84 |
| Persisted/modified record violates a storage invariant | `StorageInvariantViolated` | 85 |
| Settled sum cannot be represented in `i128` | `PotentialOverflow` | 45 |
| Loaded contract id does not exist | `ContractNotFound` | 10 |

## Compatibility

- **Append-only error codes.** `ConcurrentMutation = 84` and
  `StorageInvariantViolated = 85` are appended after the highest existing
  discriminant (`TokenScaleMismatch = 83`). No existing code changed value.
- **No signature changes.** Every guard sits *inside* an existing entrypoint; the
  public ABI, argument lists, and return types are untouched.
- **No new failure for valid input.** The lock and validators are unreachable for
  a single, well-formed, non-re-entrant call, so existing callers behave exactly
  as before. Rejections are retry-safe: once the in-flight mutation ends, the same
  request succeeds.
- **TTL namespace.** The lock uses its own `DataKey`, so it cannot collide with
  `Contract(id)` or the milestone vector.

## Tests

Unit tests live in `storage_validation::concurrency_tests`; contract-level
regression tests live in
[`test/concurrent_mutation_guard.rs`](../../contracts/escrow/src/test/concurrent_mutation_guard.rs).

| Scenario | Test |
| --- | --- |
| Lock not held before acquire | `concurrency_tests::lock_is_not_held_before_acquire` |
| Guard releases on drop, lock re-acquirable | `concurrency_tests::guard_releases_lock_when_dropped` |
| Re-entrant acquire rejected | `concurrency_tests::reentrant_acquire_is_rejected` |
| Locks scoped per contract | `concurrency_tests::locks_are_scoped_per_contract` |
| Release is idempotent | `concurrency_tests::release_is_idempotent` |
| Healthy / fully-settled accounting accepted | `concurrency_tests::validate_contract_accounting_accepts_*` |
| Over-settled, negative, overflowing accounting rejected | `concurrency_tests::validate_contract_accounting_rejects_*` |
| Pending / released / refunded milestones accepted | `concurrency_tests::validate_milestone_consistency_accepts_*` |
| Both flags, partial refund, negative amount rejected | `concurrency_tests::validate_milestone_consistency_rejects_*` |
| Checked load rejects absent and corrupt records | `concurrency_tests::load_contract_checked_*` |
| Checked store validates before writing | `concurrency_tests::store_contract_checked_*` |
| `release_milestone` rejected while a mutation is in flight, retry succeeds | `concurrent_mutation_guard::release_milestone_is_rejected_while_a_mutation_is_in_flight` |
| `deposit_funds` rejected while a mutation is in flight | `concurrent_mutation_guard::deposit_funds_is_rejected_while_a_mutation_is_in_flight` |
| `refund_unreleased_milestones` rejected while a mutation is in flight | `concurrent_mutation_guard::refund_is_rejected_while_a_mutation_is_in_flight` |
| `cancel_contract` rejected while a mutation is in flight | `concurrent_mutation_guard::cancel_contract_is_rejected_while_a_mutation_is_in_flight` |
| Successful call leaves no lock behind | `concurrent_mutation_guard::successful_entrypoint_leaves_no_lock_behind` |
| Sequential calls are not serialised against each other | `concurrent_mutation_guard::sequential_releases_are_not_serialised_against_each_other` |
| Corrupt persisted record rejected at the entrypoint | `concurrent_mutation_guard::release_rejects_a_corrupt_persisted_contract` |

Run them with:

```bash
cargo test -p escrow storage_validation
cargo test -p escrow concurrent_mutation_guard
```
