# Milestone Schedule Metadata

**Feature branch:** `feature/contracts-13-milestone-schedule-metadata`  
**Issue:** Contracts-13  
**Scope:** `contracts/escrow`

---

## Overview

The escrow contract now supports **optional per-milestone scheduling information**. When creating a contract (or later via `set_milestone_schedule`), the client may attach the following fields to any milestone:

| Field | Type | Purpose |
|---|---|---|
| `due_date` | `Option<u64>` | Unix timestamp (seconds) by which the milestone should be complete |
| `title` | `Option<String>` | Short human-readable label (≤ 128 bytes) |
| `description` | `Option<String>` | Extended deliverable description (≤ 512 bytes) |
| `updated_at` | `u64` | Ledger timestamp of the last write — **set by the contract, not the caller** |

Schedule metadata is **informational / off-chain signalling only**. The contract does not automatically cancel, refund, or release milestones when a deadline passes. Deadline enforcement is the responsibility of the calling application.

---

## API

### `create_contract` (updated signature)

```rust
pub fn create_contract(
    env: Env,
    client: Address,
    freelancer: Address,
    arbiter: Option<Address>,
    milestone_amounts: Vec<i128>,
    release_auth: ReleaseAuthorization,
    schedules: Vec<Option<MilestoneSchedule>>,   // NEW
) -> u32
```

`schedules` must be either:
- **empty** (`Vec::new(&env)`) — no schedule metadata is stored, or
- **the same length as `milestone_amounts`** — each entry is `Some(MilestoneSchedule)` or `None`.

### `set_milestone_schedule`

Update schedule metadata for a single milestone after the contract is created. Only the **client** of the contract may call this.

```rust
pub fn set_milestone_schedule(
    env: Env,
    contract_id: u32,
    milestone_idx: u32,
    schedule: MilestoneSchedule,
) -> bool
```

Once a milestone is **released**, its schedule entry is **immutable**.

### `get_milestone_schedule`

```rust
pub fn get_milestone_schedule(
    env: Env,
    contract_id: u32,
    milestone_idx: u32,
) -> Option<MilestoneSchedule>
```

Returns `None` if no metadata has been stored for that milestone.

---

## Validation Rules

All validations run **before any storage write** to prevent partial state corruption.

| Rule | Error | Details |
|---|---|---|
| `due_date` must be strictly in the future | `ScheduleDueDateInPast` | `due_date > env.ledger().timestamp()` |
| `due_date` values must be strictly increasing | `ScheduleDatesNotMonotonic` | Milestone N+1's date > milestone N's date; undated milestones are skipped |
| `title` ≤ 128 bytes | `ScheduleStringTooLong` | UTF-8 byte length |
| `description` ≤ 512 bytes | `ScheduleStringTooLong` | UTF-8 byte length |
| Schedule length must match milestone count | panic | Only when `schedules` is non-empty |
| Schedule is immutable after milestone release | `ScheduleImmutableAfterRelease` | Applies to `set_milestone_schedule` only |
| Milestone index must be in range | panic | `milestone_idx < milestones.len()` |

---

## Storage Layout

Schedule entries are stored in **persistent storage** under a dedicated key variant:

```
DataKey::MilestoneSchedule(contract_id: u32, milestone_idx: u32)
```

This keeps schedule data separate from the `EscrowContractData` record so that the core contract record stays compact even when schedules carry long strings.

---

## Security Considerations

1. **Storage exhaustion** — `title` and `description` are length-bounded at 128 and 512 bytes respectively. This bounds the on-chain storage cost per milestone to a known maximum.

2. **Retroactive manipulation** — once a milestone is released its schedule entry is frozen. This prevents a client from altering the historical record of what was agreed for a completed milestone.

3. **Authorization** — only the client of the contract may call `set_milestone_schedule`. The function calls `contract.client.require_auth()`.

4. **Atomicity** — the contract validates *all* schedule metadata before writing any entry. A single invalid entry causes the entire transaction to fail with no partial writes.

5. **Past due dates** — a `due_date` ≤ `env.ledger().timestamp()` is always rejected, preventing confusion between "already overdue" and "not yet due" states.

6. **`updated_at` integrity** — the `updated_at` field is set from `env.ledger().timestamp()` by the contract, not from a caller-supplied value. Callers cannot back-date or future-date the timestamp.

---

## Error Codes (new)

| Variant | Discriminant | Condition |
|---|---|---|
| `ScheduleDueDateInPast` | 16 | `due_date ≤ now` |
| `ScheduleDatesNotMonotonic` | 17 | Non-increasing due dates |
| `ScheduleStringTooLong` | 18 | Title > 128 bytes or description > 512 bytes |
| `ScheduleImmutableAfterRelease` | 19 | Write attempted on released milestone |
| `ScheduleInvalidMilestoneIndex` | 20 | Index ≥ milestone count |

---

## Test Coverage

All new code lives in `contracts/escrow/src/test/milestone_schedule.rs`.

| Category | Tests |
|---|---|
| Happy-path creation | `valid_create_without_schedules`, `valid_create_with_partial_schedules`, `valid_create_with_all_schedules_populated`, `valid_updated_at_is_stamped_by_contract`, `valid_get_schedule_returns_none_for_missing_index` |
| Due-date validation | `error_due_date_at_present_is_rejected`, `error_due_date_in_past_is_rejected`, `valid_due_date_max_u64_is_accepted` |
| Monotonicity | `error_monotonic_equal_dates_rejected`, `error_monotonic_decreasing_dates_rejected`, `valid_monotonic_skips_undated_milestones` |
| String length | `valid_title_at_max_length_accepted`, `error_title_exceeds_max_length_rejected`, `error_description_exceeds_max_length_rejected` |
| `set_milestone_schedule` mutations | `set_schedule_client_can_update_before_release`, `error_immutable_set_schedule_after_release_rejected`, `error_set_schedule_violates_monotonicity_with_next`, `error_set_schedule_out_of_range_index_rejected`, `error_schedules_length_mismatch_rejected` |
| Integration | `integration_full_lifecycle_preserves_schedule_metadata`, `integration_schedule_isolation_across_contracts`, `integration_set_schedule_does_not_disturb_other_milestones` |

---

## Migration Notes

`create_contract` has a new final parameter `schedules: Vec<Option<MilestoneSchedule>>`. Existing callers must be updated to pass `Vec::new(&env)` to preserve the old behaviour (no schedule metadata).