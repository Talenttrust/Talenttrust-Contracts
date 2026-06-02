# Persistent Storage TTL Policy

## Overview

Soroban smart contracts use two storage tiers:

| Tier | Eviction | Use in TalentTrust |
|------|----------|--------------------|
| **Temporary** | Auto-evicted after TTL expires | Milestone approval records |
| **Persistent** | Evicted only if TTL is not extended | Contract state, milestones, reputation, fees |

Without explicit TTL extension, persistent entries expire and are evicted by the Soroban ledger. For an escrow contract holding live funds, eviction of a `DataKey::Contract(id)` entry would permanently strand the accounting for that contract — funded amounts, released amounts, and refunded amounts would be unrecoverable.

This document describes the TTL extension policy implemented in `contracts/escrow/src/ttl.rs`.

---

## TTL Constants

All constants are defined in `ttl.rs` and documented with rustdoc.

### Persistent Storage (contracts, milestones)

| Constant | Value | Approximate Duration |
|----------|-------|----------------------|
| `PERSISTENT_MAX_TTL_LEDGERS` | 6,307,200 | ~1 year (at 5 s/ledger) |
| `PERSISTENT_BUMP_THRESHOLD` | 3,110,400 | ~180 days |
| `MIN_PERSISTENT_TTL` | 518,400 | ~30 days |

### Temporary Storage (approvals)

| Constant | Value | Approximate Duration |
|----------|-------|----------------------|
| `PENDING_APPROVAL_TTL_LEDGERS` | 120,960 | ~7 days |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 60,480 | ~3.5 days |
| `MIN_APPROVAL_TTL` | 17,280 | ~1 day |

---

## Extension Policy

### When TTL Is Extended

TTL is extended on **every read and write** to a persistent entry. This ensures that any contract touched within its TTL window remains live — the invariant required to prevent fund-state loss.

| Operation | Entries Extended |
|-----------|-----------------|
| `create_contract` | `DataKey::Contract(id)`, `(DataKey::Contract(id), "milestones")`, `DataKey::NextContractId` |
| `deposit_funds` | `DataKey::Contract(id)`, `(DataKey::Contract(id), "milestones")` |
| `approve_milestone_release` | `DataKey::Contract(id)`, `(DataKey::Contract(id), "milestones")` |
| `release_milestone` | `DataKey::Contract(id)`, `(DataKey::Contract(id), "milestones")` |
| `refund_unreleased_milestones` | `DataKey::Contract(id)`, `(DataKey::Contract(id), "milestones")` |
| `get_contract` | `DataKey::Contract(id)` |
| `get_milestones` | `(DataKey::Contract(id), "milestones")` |
| `get_refundable_balance` | `DataKey::Contract(id)` |

### Bump Threshold Logic

`extend_ttl(key, threshold, extend_to)` is a no-op if the current TTL is already above `threshold`. This means:

- If a contract is accessed frequently, TTL is only bumped when it falls below 180 days remaining.
- If a contract is dormant for more than 180 days without any access, it will be bumped on the next access.
- A contract that is never accessed after creation will expire after ~1 year.

### Helper Functions

```rust
/// Extends TTL for DataKey::Contract(id)
pub fn extend_contract_ttl(env: &Env, contract_id: u32)

/// Extends TTL for (DataKey::Contract(id), "milestones")
pub fn extend_milestone_ttl(env: &Env, contract_id: u32)

/// Extends TTL for DataKey::NextContractId
pub fn extend_next_contract_id_ttl(env: &Env)

/// Convenience: extends both contract and milestone TTL
pub fn extend_contract_and_milestones_ttl(env: &Env, contract_id: u32)
```

---

## Security Assumptions

### Eviction Cannot Strand Active Escrow Accounting

**Invariant:** Any contract touched within its TTL window remains live.

- The bump threshold (180 days) is set well below the max TTL (1 year), so a single access within any 180-day window is sufficient to keep the entry alive indefinitely.
- All mutating operations (`deposit`, `release`, `refund`) extend TTL as part of the same transaction, so a successful state change always refreshes the TTL.
- Read-only operations (`get_contract`, `get_milestones`, `get_refundable_balance`) also extend TTL, so monitoring or UI queries keep contracts alive.

### No Overflow Risk

TTL values are `u32`. The maximum value used (`PERSISTENT_MAX_TTL_LEDGERS = 6,307,200`) is well within `u32::MAX` (4,294,967,295). No arithmetic is performed on TTL values in application code — they are passed directly to `extend_ttl`.

### No Auth Required for TTL Extension

`extend_ttl` is a storage operation, not a contract call. It does not require authentication and cannot be blocked by an unauthorized caller. TTL extension happens automatically as a side effect of any read or write.

### Fail-Closed State Machine

TTL extension is performed **after** all validation and state mutation. If a transaction panics (e.g., `InsufficientFunds`, `UnauthorizedRole`), the TTL extension is rolled back along with the rest of the transaction. This means:

- A failed transaction does not extend TTL.
- A successful transaction always extends TTL.
- There is no partial state where TTL is extended but the contract state is not updated.

### Approval TTL (Temporary Storage)

Approval records use temporary storage with a 7-day TTL. This is intentional:

- Expired approvals are treated as absent (fail-closed).
- The `check_approvals` function returns `InsufficientApprovals` if the approval record is missing or expired.
- Approvals are cleared after a successful release via `clear_approvals`.

---

## Scope of Coverage

The following `DataKey` variants are covered by TTL extension:

| DataKey | Storage Tier | TTL Policy |
|---------|-------------|------------|
| `DataKey::Contract(id)` | Persistent | Extended on every read/write |
| `(DataKey::Contract(id), "milestones")` | Persistent | Extended on every read/write |
| `DataKey::NextContractId` | Persistent | Extended on every create |
| `DataKey::MilestoneApprovals(id, idx)` | Temporary | Extended on every approval write |

> **Note:** `DataKey::MilestoneReleased`, `DataKey::Reputation`, and `DataKey::AccumulatedProtocolFees` are referenced in the task requirements but are not present in the current contract implementation. When these keys are added, they must follow the same TTL extension pattern: call `extend_ttl` with `PERSISTENT_BUMP_THRESHOLD` / `PERSISTENT_MAX_TTL_LEDGERS` on every read and write.

---

## Testing

TTL extension is tested in `contracts/escrow/src/test/persistence.rs`. The test suite covers:

| Test | What It Verifies |
|------|-----------------|
| `persistent_contract_ttl_extended_on_create` | TTL extended after `create_contract` |
| `persistent_contract_ttl_extended_on_deposit` | TTL extended after `deposit_funds` |
| `persistent_contract_ttl_extended_on_release` | TTL extended after `release_milestone` |
| `persistent_contract_ttl_extended_on_refund` | TTL extended after `refund_unreleased_milestones` |
| `persistent_milestone_ttl_extended_on_read` | Milestone TTL extended after `get_milestones` |
| `persistent_ttl_extended_on_get_refundable_balance` | TTL extended after `get_refundable_balance` |
| `persistent_ttl_extended_on_approve_milestone` | TTL extended after `approve_milestone_release` |
| `next_contract_id_ttl_extended_on_create` | NextContractId TTL extended; sequential IDs work after ledger advance |
| `persistent_ttl_prevents_fund_state_loss_in_long_running_contract` | Fund state preserved across multiple TTL cycles |
| `persistent_ttl_survives_complete_lifecycle` | Full lifecycle with ledger advances between each stage |
| `ttl_constants_are_within_soroban_limits` | Constants are within Soroban network limits |
| `multiple_contracts_have_independent_ttl_lifetimes` | Contracts have independent TTL lifetimes |
| `mixed_release_and_refund_preserves_accounting_across_ledger_advances` | Mixed lifecycle preserves accounting |

---

## Performance Impact

TTL extension adds one `extend_ttl` call per persistent entry accessed. On Soroban, `extend_ttl` is a ledger operation that counts against the transaction's read/write entry budget. The performance baselines in `test/performance.rs` have been updated to account for the additional entries:

| Operation | Read Entries (before) | Read Entries (after) |
|-----------|----------------------|---------------------|
| `create_contract` | 4 | 5 |
| `deposit_funds` | 3 | 4 |
| `release_milestone` | 4 | 5 |

The fee impact is minimal — `extend_ttl` is significantly cheaper than a full storage write.

---

## References

- [Soroban Storage TTL Documentation](https://developers.stellar.org/docs/build/smart-contracts/storage/state-archival)
- [Stellar State Archival](https://developers.stellar.org/docs/learn/encyclopedia/storage/state-archival)
- `contracts/escrow/src/ttl.rs` — TTL constants and helper functions
- `contracts/escrow/src/test/persistence.rs` — TTL extension tests
