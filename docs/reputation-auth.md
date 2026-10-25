# Reputation Authorization Rules

This document describes who may call each reputation entrypoint, under what
conditions, and which rejections apply. All rules are derived from the
implementation in `contracts/escrow/src/lib.rs`.

---

## Roles

| Role | Description | Reputation permissions |
|---|---|---|
| **client** | The party that commissioned the work | May call `issue_reputation` on their own contracts |
| **freelancer** | The party that performed the work | Read-only (`get_reputation`, etc.) |
| **arbiter** | Dispute resolver | None |
| **admin** | Protocol administrator | None |

---

## Entrypoints

### Mutating

| Entrypoint | Caller restriction | Auth required |
|---|---|---|
| `issue_reputation(contract_id, caller, rating, comment)` | `caller == contract.client` | `caller.require_auth()` |

### Read-only (public)

| Entrypoint | Returns |
|---|---|
| `get_reputation(address) -> Option<Reputation>` | Freelancer aggregate record |
| `get_average_rating(address) -> Option<i128>` | Average rating (×10 000 basis points) |
| `get_reputation_comment(contract_id) -> Option<String>` | Client comment for a contract |
| `get_pending_reputation_credits(address) -> i128` | Number of completed contracts awaiting rating |

---

## `issue_reputation` Guard Chain

Guards are evaluated in source order (`lib.rs:1494-1529`). The first failing
guard panics with the corresponding error.

| # | Guard | Error | Code |
|---|---|---|---|
| 1 | Contract is not paused | `ContractPaused` | 16 |
| 2 | Emergency pause is not active | `EmergencyActive` | 17 |
| 3 | Contract exists in storage | `ContractNotFound` | 6 |
| 4 | `caller == contract.client` | `UnauthorizedRole` | 15 |
| 5 | `rating >= 1 && rating <= 5` | `InvalidRating` | 19 |
| 6 | `comment.len() > 0` | `EmptyComment` | 42 |
| 7 | `comment.len() <= 200` | `CommentTooLong` | 43 |
| 8 | `contract.status == Completed` | `NotCompleted` | 22 |
| 9 | `contract.reputation_issued == false` | `ReputationAlreadyIssued` | 21 |
| 10 | `contract.client != contract.freelancer` | `SelfRating` | 20 |
| 11 | `caller.require_auth()` succeeds | Soroban auth failure | — |
| 12 | `PendingReputationCredits(freelancer) > 0` | `InvalidState` | 18 |

---

## State Transitions

### Pending credit granted (increment)

A pending reputation credit is added for the freelancer when a contract
transitions to `Completed`:

| Code path | File:line |
|---|---|
| `release_milestone` — all milestones released/refunded | `lib.rs:654-658` |
| `release_milestone_impl` — internal release helper | `release.rs:122-128` |
| `refund_unreleased_milestones` — partial release + refund | `lib.rs:914-922` |
| `resolve_dispute` — dispute resolved with freelancer payout | `lib.rs:2124-2126` |

Fully refunded contracts (`Refunded` status) do **not** grant a credit.

### Pending credit consumed (decrement)

| Code path | File:line | Condition |
|---|---|---|
| `issue_reputation` | `lib.rs:1543-1548` | `pending > 0` (panics `InvalidState` otherwise) |

### `reputation_issued` flag

| From | To | Trigger |
|---|---|---|
| `false` | `true` | `issue_reputation` succeeds (`lib.rs:1530`) |

This is a one-way transition. Once set, `issue_reputation` for that contract is
permanently blocked.

### Reputation aggregation

On successful `issue_reputation` (`lib.rs:1550-1556`):

- `completed_contracts += 1`
- `total_rating += rating`
- `last_rating = rating`

---

## Worked Example

```
1. Client creates contract #42 with freelancer Alice.
   → Contract.status = Created, reputation_issued = false

2. Client funds the contract.
   → Contract.status = Funded

3. Client releases all milestones.
   → Contract.status = Completed
   → PendingReputationCredits(Alice) += 1   // credit granted

4. Client calls issue_reputation(42, client, 5, "Great work!")
   Guard checks (all pass):
     ✓ Not paused
     ✓ Contract exists
     ✓ caller == client
     ✓ rating in [1,5]
     ✓ comment non-empty, ≤200 bytes
     ✓ status == Completed
     ✓ reputation_issued == false
     ✓ client != Alice
     ✓ Soroban auth succeeds
     ✓ PendingReputationCredits(Alice) > 0

   State changes:
     → contract.reputation_issued = true
     → PendingReputationCredits(Alice) -= 1   // credit consumed
     → Reputation(Alice): completed_contracts=1, total_rating=5, last_rating=5

5. Client tries issue_reputation(42, client, 3, "Actually, mediocre")
   → Panics: ReputationAlreadyIssued (code 21)
```

---

## Error Reference

| Error | Code | Meaning |
|---|---|---|
| `ContractNotFound` | 6 | No contract with the given ID |
| `UnauthorizedRole` | 15 | Caller is not the contract client |
| `ContractPaused` | 16 | Contract is paused (non-emergency) |
| `EmergencyActive` | 17 | Emergency mode is active |
| `InvalidState` | 18 | No pending credit to consume |
| `InvalidRating` | 19 | Rating outside [1, 5] |
| `SelfRating` | 20 | Client and freelancer are the same address |
| `ReputationAlreadyIssued` | 21 | Reputation already issued for this contract |
| `NotCompleted` | 22 | Contract not in Completed status |
| `EmptyComment` | 42 | Comment is empty |
| `CommentTooLong` | 43 | Comment exceeds 200 bytes |
| Soroban auth failure | — | Cryptographic signature not provided |
