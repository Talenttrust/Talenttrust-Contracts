# Reputation Model and Invariants

This document is the authoritative reference for the escrow contract's reputation
system. It covers the data model, the storage layout, every entrypoint that touches
reputation state, the invariants the code maintains, and a worked example tracing
one complete reputation lifecycle from contract creation to rating query.

Cross-references to source:

- Types: `contracts/escrow/src/types.rs` — `Reputation`, `DataKey`
- Entrypoints: `contracts/escrow/src/lib.rs` — `issue_reputation`,
  `grant_pending_reputation_credit`, `get_reputation`, `get_average_rating`,
  `get_pending_reputation_credits`, `get_reputation_comment`
- Tests: `contracts/escrow/src/test/reputation.rs`

---

## 1. Data Model

### 1.1 `Reputation` struct

```rust
// contracts/escrow/src/types.rs
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Reputation {
    pub completed_contracts: i128,
    pub total_rating:        i128,
    pub last_rating:         i128,
}
```

| Field | Type | Meaning |
|---|---|---|
| `completed_contracts` | `i128` | Number of contracts for which `issue_reputation` has been called successfully. Increments by 1 per call. |
| `total_rating` | `i128` | Running sum of all ratings (each in 1–5). Used to compute the average. |
| `last_rating` | `i128` | The rating from the most recent `issue_reputation` call. Overwritten on each issuance. |

`Reputation` derives `Default` — a freelancer with no issued reputation has
`completed_contracts = 0`, `total_rating = 0`, `last_rating = 0`.

### 1.2 Storage keys

All reputation-related entries live in **persistent storage**:

| `DataKey` variant | Key type | Value | Written by | Read by |
|---|---|---|---|---|
| `Reputation(Address)` | freelancer address | `Reputation` struct | `issue_reputation` | `get_reputation`, `get_average_rating` |
| `ReputationIssued(u32)` | contract id | `bool` (`true`) | `issue_reputation` | `get_contract_summary` |
| `PendingReputationCredits(Address)` | freelancer address | `i128` | `grant_pending_reputation_credit`, `issue_reputation` | `get_pending_reputation_credits` |
| `ReputationComment(u32)` | contract id | `String` (≤200 bytes) | `issue_reputation` | `get_reputation_comment` |

`Contract.reputation_issued` (the field on the `Contract` struct) is a
secondary denormalised flag kept in sync with `DataKey::ReputationIssued`.
`get_contract_summary` reads the `DataKey` entry first and falls back to the
struct field for backward compatibility with older storage layouts.

---

## 2. Lifecycle: how credits flow

```
create_contract ──► deposit_funds ──► release_milestone (all done)
                                              │
                              ContractStatus transitions to Completed
                                              │
                         grant_pending_reputation_credit(freelancer)
                              PendingReputationCredits[freelancer] += 1
                                              │
                              (client calls) issue_reputation(contract_id, rating, comment)
                                              │
                              PendingReputationCredits[freelancer] -= 1
                              Reputation[freelancer].completed_contracts += 1
                              Reputation[freelancer].total_rating        += rating
                              Reputation[freelancer].last_rating          = rating
                              ReputationIssued[contract_id]              = true
                              ReputationComment[contract_id]             = comment
                              Contract.reputation_issued                 = true
```

### 2.1 When a credit is granted

`grant_pending_reputation_credit` is a private helper called in exactly three
places:

| Trigger | Location in lib.rs |
|---|---|
| Final `release_milestone` drives all milestones to released-or-refunded, and at least one was released (not all refunded). | `release_milestone` → `all_released` branch |
| `refund_unreleased_milestones` empties the contract with a mix of released and refunded milestones. | `refund_unreleased_milestones` → `all_refunded_or_released && !all_refunded` branch |
| Dispute resolution yields `ContractStatus::Completed`. | `resolve_dispute` → `final_status_after_resolution == Completed` branch |

A contract whose milestones are **all refunded** (no releases at all) transitions
to `ContractStatus::Refunded`, not `Completed`, and **no credit is granted**.

### 2.2 Pending credit invariant

> **Invariant PRC-1.** At any point in time,
> `PendingReputationCredits[freelancer]` equals the number of `Completed`
> contracts associated with that freelancer for which `issue_reputation` has
> **not yet** been called.

More precisely, every `grant_pending_reputation_credit` increments the counter
by 1, and every successful `issue_reputation` decrements it by 1. The counter
is never decremented below 0 (the code panics with `InvalidState` if `pending
<= 0`).

### 2.3 `reputation_issued` flag invariant

> **Invariant RI-1.** After `issue_reputation` succeeds for `contract_id`, both
> `DataKey::ReputationIssued(contract_id)` is `true` and
> `Contract.reputation_issued` is `true`. A second call to `issue_reputation`
> on the same `contract_id` is rejected with `ReputationAlreadyIssued`.

---

## 3. Entrypoints

### 3.1 `issue_reputation`

```
issue_reputation(contract_id: u32, caller: Address, rating: u32, comment: String) -> bool
```

**Access control:** `caller` must equal `Contract.client` for `contract_id`.
The check uses direct address equality, not `require_auth`, for the role
predicate; `caller.require_auth()` is called afterward to enforce Soroban
transaction-level authentication.

**Validation order** (fail-fast, no state mutation before all checks pass):

1. Pause/emergency gate (`require_not_paused`).
2. Load contract; panic `ContractNotFound` if missing.
3. `caller != contract.client` → `UnauthorizedRole`.
4. `rating < 1 || rating > 5` → `InvalidRating`.
5. `comment.len() == 0` → `EmptyComment`.
6. `comment.len() > 200` → `CommentTooLong`. *(byte length, not character count)*
7. `contract.status != Completed` → `NotCompleted`.
8. `contract.reputation_issued == true` → `ReputationAlreadyIssued`.
9. `contract.client == contract.freelancer` → `SelfRating`.
10. `caller.require_auth()` — Soroban auth enforcement.
11. `PendingReputationCredits[freelancer] <= 0` → `InvalidState`.

**State mutations** (all-or-nothing on success):

- `Contract.reputation_issued = true` persisted.
- `DataKey::ReputationIssued(contract_id) = true` persisted.
- `PendingReputationCredits[freelancer] -= 1`.
- `Reputation[freelancer].completed_contracts += 1`.
- `Reputation[freelancer].total_rating += rating`.
- `Reputation[freelancer].last_rating = rating`.
- `DataKey::ReputationComment(contract_id) = comment` persisted.

**Events:** none emitted by `issue_reputation` in the current implementation.

**Error codes:**

| Error | Condition |
|---|---|
| `ContractPaused` | Pause flag is set |
| `EmergencyActive` | Emergency flag is set |
| `ContractNotFound` | `contract_id` does not exist |
| `UnauthorizedRole` | `caller != contract.client` |
| `InvalidRating` | `rating < 1` or `rating > 5` |
| `EmptyComment` | `comment` is 0 bytes |
| `CommentTooLong` | `comment` exceeds 200 bytes |
| `NotCompleted` | Contract is not in `Completed` status |
| `ReputationAlreadyIssued` | Reputation already issued for this contract |
| `SelfRating` | `client == freelancer` |
| `InvalidState` | Pending credits counter is 0 (should not normally be reachable if invariants hold) |

### 3.2 `grant_pending_reputation_credit` (private)

```
fn grant_pending_reputation_credit(env: &Env, freelancer: &Address)
```

Private helper. Increments `PendingReputationCredits[freelancer]` by 1.
Initialises from 0 if the key is absent. Not callable externally.

### 3.3 `get_reputation`

```
get_reputation(address: Address) -> Option<Reputation>
```

Returns the `Reputation` struct for `address`, or `None` if no reputation has
ever been issued. Does not extend TTL and performs no auth check.

### 3.4 `get_average_rating`

```
get_average_rating(address: Address) -> Option<i128>
```

Returns the average rating scaled by **10 000** (basis points), or `None` if
`completed_contracts == 0` or no reputation record exists.

```
result = total_rating × 10_000 / completed_contracts
```

Examples:

| `total_rating` | `completed_contracts` | Return value | Decimal equivalent |
|---|---|---|---|
| 5 | 1 | 50_000 | 5.0000 |
| 8 | 2 | 40_000 | 4.0000 |
| 3 | 2 | 15_000 | 1.5000 |
| 7 | 3 | 23_333 | 2.3333 |

Checked arithmetic is used throughout; division by zero is structurally
impossible because `None` is returned when `completed_contracts == 0`.

**Note:** The `docs/escrow/REPUTATION.md` file describes the scale factor as
`×100` and uses `450` for 4.50. The source code uses `×10_000` and the test
suite confirms `Some(40_000)` for a single rating of 4. The `×10_000` value
is canonical.

### 3.5 `get_pending_reputation_credits`

```
get_pending_reputation_credits(address: Address) -> i128
```

Returns the current pending credit count for `address`. Returns `0` when the
key is absent (no completed contracts yet). No auth, no TTL extension.

### 3.6 `get_reputation_comment`

```
get_reputation_comment(contract_id: u32) -> Option<String>
```

Returns the comment stored when reputation was issued for `contract_id`, or
`None` if reputation has not been issued. Extends the entry's TTL on a
successful read.

---

## 4. Invariants summary

| ID | Statement | Enforced by |
|---|---|---|
| **PRC-1** | `PendingReputationCredits[f]` equals the number of `Completed` contracts for freelancer `f` that have not yet been rated. | `grant_pending_reputation_credit` (+1), `issue_reputation` (−1), panic on `pending <= 0` |
| **RI-1** | `ReputationIssued[contract_id]` and `Contract.reputation_issued` are both `true` after the first successful `issue_reputation`. | `issue_reputation` writes both atomically |
| **RI-2** | `issue_reputation` is idempotent-safe: a second call is always rejected. | `contract.reputation_issued` check → `ReputationAlreadyIssued` |
| **REP-1** | `Reputation[f].completed_contracts` equals the number of successful `issue_reputation` calls for freelancer `f`. | +1 on each successful `issue_reputation` |
| **REP-2** | `Reputation[f].total_rating` equals the sum of all ratings issued to freelancer `f`. | +rating on each successful `issue_reputation` |
| **REP-3** | `Reputation[f].last_rating` equals the rating from the most recent `issue_reputation` for freelancer `f`. | overwritten on each successful `issue_reputation` |
| **SELF-1** | A single address cannot both issue and receive reputation on the same contract. | `client == freelancer` → `SelfRating` |
| **GATE-1** | Reputation can only be issued for a `Completed` contract. | `status != Completed` → `NotCompleted` |
| **GATE-2** | Only the contract client can call `issue_reputation`. | `caller != client` → `UnauthorizedRole` |
| **CREDIT-1** | A contract whose milestones are entirely refunded (none released) does not accrue a pending reputation credit. | `Refunded` status path skips `grant_pending_reputation_credit` |

---

## 5. Worked example

**Scenario:** Alice (client) hires Bob (freelancer) for a two-milestone contract
worth 500 + 300 = 800 tokens. Both milestones are completed; Alice rates Bob 4.

### Step 1 — Create and fund

```
escrow.create_contract(alice, bob, None, [500, 300], ClientOnly)
// → contract_id = 1, status = Created

escrow.deposit_funds(1, alice, 800)
// → status = Funded, contract.funded_amount = 800
```

Reputation state: nothing written yet.

### Step 2 — Release milestone 0

```
escrow.approve_milestone_release(1, alice, 0)
escrow.release_milestone(1, alice, 0)
// milestone[0].released = true
// contract.released_amount = 500
// all milestones released? No (milestone[1] still pending) → status stays Funded
```

Reputation state: unchanged.

### Step 3 — Release milestone 1 (final release)

```
escrow.approve_milestone_release(1, alice, 1)
escrow.release_milestone(1, alice, 1)
// milestone[1].released = true
// contract.released_amount = 800
// all milestones released → status = Completed
// grant_pending_reputation_credit(bob) is called:
//   PendingReputationCredits[bob] = 0 + 1 = 1
```

Reputation state:
- `PendingReputationCredits[bob] = 1`
- `Reputation[bob]` — not yet written

### Step 4 — Issue reputation

```
escrow.issue_reputation(1, alice, 4, "Great work, delivered on time!")
// Validation passes: alice == client, rating=4 ∈ [1,5], comment OK,
//   status=Completed, reputation_issued=false, alice != bob, pending=1 > 0
//
// Mutations:
//   Contract.reputation_issued = true
//   DataKey::ReputationIssued(1) = true
//   PendingReputationCredits[bob] = 1 - 1 = 0
//   Reputation[bob] = { completed_contracts: 1, total_rating: 4, last_rating: 4 }
//   ReputationComment[1] = "Great work, delivered on time!"
```

### Step 5 — Query

```
escrow.get_reputation(bob)
// → Some(Reputation { completed_contracts: 1, total_rating: 4, last_rating: 4 })

escrow.get_average_rating(bob)
// → Some(40_000)   // 4 × 10_000 / 1 = 40_000  (= 4.0000 on a 1–5 scale)

escrow.get_pending_reputation_credits(bob)
// → 0

escrow.get_reputation_comment(1)
// → Some("Great work, delivered on time!")
```

### Step 6 — Duplicate issuance rejected

```
escrow.issue_reputation(1, alice, 5, "Second attempt")
// → panics with ReputationAlreadyIssued
// No state mutation occurs.
```

---

## 6. Edge cases and failure modes

### 6.1 Fully-refunded contract earns no credit

If all milestones are refunded (no release ever happens), `status` transitions to
`Refunded`, not `Completed`, and `grant_pending_reputation_credit` is not called.
`issue_reputation` would then fail with `NotCompleted`.

### 6.2 Mixed release-and-refund contract

If some milestones are released and the rest refunded, the contract reaches
`Completed` and one credit is granted — identical to a fully-released contract.

### 6.3 Dispute resolution reaching Completed

A dispute resolved with `FullPayout` or `Split` may also transition the contract
to `Completed` and grant a pending credit, making the contract eligible for
reputation issuance.

### 6.4 `pending <= 0` guard in `issue_reputation`

In a correct execution, `PendingReputationCredits[freelancer]` should always be
≥ 1 when `issue_reputation` is called on a `Completed`, not-yet-rated contract.
The `pending <= 0` check is a defensive invariant guard; hitting it indicates a
storage inconsistency or a bug upstream.

### 6.5 Comment length is measured in UTF-8 bytes, not characters

`String::len()` in Soroban returns byte length. A 3-byte emoji counts as 3 toward
the 200-byte cap. All ASCII text has a 1:1 byte-to-character ratio.

---

## 7. Storage TTL

All reputation keys use **persistent storage** and are bump-extended with:

```rust
// contracts/escrow/src/ttl.rs
PERSISTENT_BUMP_THRESHOLD  // extends TTL when below this value
PERSISTENT_TTL_LEDGERS     // target TTL after extension
```

`ReputationIssued` and `ReputationComment` entries are extended on write (in
`issue_reputation`) and on read (in `get_reputation_comment`).
`Reputation` and `PendingReputationCredits` entries are extended on each write.

---

## 8. Test coverage

The reputation module is tested in `contracts/escrow/src/test/reputation.rs`.
Covered scenarios include:

| Test | Invariant verified |
|---|---|
| `pending_reputation_credits_accumulate_and_drain_across_completed_contracts` | PRC-1, RI-2 |
| `issue_reputation_rejects_unauthorized_caller` | GATE-2 |
| `issue_reputation_rejects_non_completed_contract` | GATE-1 |
| `issue_reputation_rejects_invalid_rating_bounds` | rating ∈ [1, 5] |
| `issue_reputation_rejects_empty_comment` | EmptyComment |
| `issue_reputation_rejects_comment_too_long` | CommentTooLong |
| `issue_reputation_rejects_duplicate_issuance` | RI-2 |
| `issue_reputation_rejects_self_rating_when_client_equals_freelancer` | SELF-1 |
| `issue_reputation_succeeds_for_distinct_client_and_freelancer` | happy path |
| `issue_reputation_updates_reputation_record_and_pending_credits` | REP-1, REP-2, REP-3, PRC-1 |
| `get_average_rating_returns_none_for_unknown_address` | absent → None |
| `get_average_rating_single_rating_returns_scaled_value` | ×10_000 scale |
| `get_average_rating_multiple_ratings_returns_correct_scaled_average` | REP-2 |
| `get_average_rating_fractional_average_is_preserved` | integer division precision |
| Refunded contract test (within `pending_reputation_credits_accumulate_and_drain`) | CREDIT-1 |
