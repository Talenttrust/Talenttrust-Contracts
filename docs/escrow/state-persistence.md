# Escrow State Persistence

This document maps the escrow contract's persisted storage to the lifecycle invariants reviewers should verify.

For transient keys (pending approvals, pending migrations) and their TTL / expiration policy, see [storage-ttl.md](./storage-ttl.md).

## Storage Keys

| Key | Value | Purpose |
| --- | --- | --- |
| `PauseAdmin` | `Address` | authority for pause and emergency controls |
| `Paused` | `bool` | fail-closed switch for mutating escrow flows |
| `EmergencyPaused` | `bool` | blocks standard `unpause` until explicit recovery |
| `NextContractId` | `u32` | monotonically increasing escrow identifier counter |
| `Contract(id)` | `EscrowContractData` | full persisted lifecycle and participant record |
| `Reputation(address)` | `ReputationRecord` | aggregate ratings for a freelancer |
| `PendingReputationCredits(address)` | `u32` | count of completed contracts still eligible to issue a rating |
| `GovernanceAdmin` | `Address` | current protocol parameter admin |
| `PendingGovernanceAdmin` | `Address` | proposed next governance admin |
| `ProtocolParameters` | `ProtocolParameters` | live validation bounds for creation and rating |

## Escrow Record Fields

`EscrowContractData` persists:

- `client`
- `freelancer`
- `milestones`
- `milestone_count`
- `total_amount`
- `funded_amount`
- `released_amount`
- `released_milestones`
- `status`
- `reputation_issued`
- `created_at`
- `updated_at`

## Persistence Invariants

Creation invariants:

- `milestone_count == milestones.len()`
- `total_amount == sum(milestones.amount)`
- `funded_amount == 0`
- `released_amount == 0`
- `released_milestones == 0`
- `status == Created`
- `reputation_issued == false`

Funding invariants:

- `0 < funded_amount <= total_amount`
- status becomes `Funded` after the first successful deposit

Release invariants:

- each milestone changes from unreleased to released once
- `released_amount` increases by the released milestone amount
- `released_milestones` increases by one per successful release
- `released_amount <= funded_amount`
- final release transitions `status` to `Completed`

Reputation invariants:

- completed contracts mint one pending reputation credit for the recorded freelancer
- `issue_reputation` consumes exactly one pending credit
- `reputation_issued` is irreversible

## Reviewer Checklist

1. Confirm invalid participant or milestone metadata cannot be persisted.
2. Confirm overfunding is rejected before storage writes.
3. Confirm milestone double release is rejected.
4. Confirm completed contracts can issue reputation once.
5. Confirm pause and emergency flags block every mutating payment path.

## Contract ID Allocation

Contract IDs are issued by a monotonically increasing counter stored under the
`NextContractId` key in persistent storage.

### Allocation algorithm

1. Read `NextContractId` from persistent storage; default to `1` if absent.
2. **Write-ahead**: persist `NextContractId + 1` before writing any contract data.
3. Write the `Contract(id)` record.
4. Return `id` to the caller.

### Safety properties

**Monotonic** — the counter is never decremented. Every successful call to
`create_contract` produces an ID strictly greater than all previously issued IDs.

**Never reused** — the counter is advanced in step 2, before the contract data
write in step 3. If the contract write panics or the transaction is rolled back
after step 2, the counter has already moved forward. The skipped ID is
permanently retired; it will never be assigned to a different contract.

**Failure safe** — validation errors (empty milestones, invalid participants,
etc.) are checked before step 2. A rejected call leaves the counter unchanged,
so no IDs are wasted on invalid inputs.

**Storage-migration safe** — `NextContractId` is a dedicated, named key with no
dependency on the shape of `EscrowContractData`. Migrating or upgrading the
contract record schema does not affect the counter.

**Per-instance isolation** — each deployed contract instance has its own
persistent storage namespace. Counters from different instances are independent
and do not interfere.
