# Reputation Authorization Rules

This document describes who may call each reputation entrypoint, under what
conditions, and which errors apply. All rules are derived from the
implementation in `contracts/escrow/src/lib.rs`.

---

## Roles

| Role | Description | Reputation permissions |
|---|---|---|
| **client** | The party that commissioned the work | May call `issue_reputation` on their own escrows |
| **freelancer** | The party that performed the work | May call `rate_client` on their own escrows |
| **admin** | Protocol administrator | May call `void_rating` to reverse fraudulent freelancer ratings |
| **anyone** | Public / permissionless | Read-only getters and `extend_reputation_ttl` |

---

## Entrypoints

### Mutating

| Entrypoint | Caller restriction | Auth required |
|---|---|---|
| `issue_reputation(id, rating)` | `contract.client` | `c.client.require_auth()` |
| `rate_client(id, rating)` | `contract.freelancer` | `c.freelancer.require_auth()` |
| `void_rating(id, reason)` | Contract admin | `require_admin(&env)` |
| `extend_reputation_ttl(freelancer)` | Permissionless | None |

### Read-only (public)

| Entrypoint | Returns |
|---|---|
| `get_reputation(freelancer) -> Reputation` | Freelancer unweighted aggregate (`completed_contracts`, `total_rating`) |
| `get_weighted_reputation(freelancer) -> WeightedReputation` | Freelancer value-weighted aggregate (`weighted_rating_sum`, `weight_total`, `rated_contracts`) |
| `get_reputation_score(freelancer) -> Option<u32>` | Value-weighted score in hundredths (`reputation_score`, half-up rounded) |
| `get_rating(id) -> Option<RatingRecord>` | Per-contract freelancer rating record (`freelancer`, `rating`, `issued_at`, `voided`) |
| `get_client_reputation(client) -> Reputation` | Client unweighted aggregate (`completed_contracts`, `total_rating`) |

---

## Guard Chains

Guards are evaluated in source order in `contracts/escrow/src/lib.rs`. The first
failing guard returns the corresponding `EscrowError` (or fails Soroban auth).

### `issue_reputation(id, rating)`

| # | Guard | Error | Code |
|---|---|---|---|
| 1 | `PAUSE_REPUTATION` is not active | `Paused` | 4 |
| 2 | Escrow is not compacted | `EscrowCompacted` | 26 |
| 3 | Escrow exists in storage | `ContractNotFound` | 5 |
| 4 | Escrow is not frozen | `EscrowFrozen` | 22 |
| 5 | `c.client.require_auth()` succeeds | Soroban auth failure | — |
| 6 | `c.status` is `Completed` or `Resolved` | `InvalidStatus` | 10 |
| 7 | `c.reputation_issued == false` | `ReputationAlreadyIssued` | 16 |
| 8 | `rating` in `1..=5` | `InvalidRating` | 15 |
| 9 | `c.released_amount >= MIN_REPUTATION_VALUE` (`10_000_000`) | `BelowReputationThreshold` | 30 |
| 10 | Checked arithmetic on `Reputation` & `WeightedReputation` | `ArithmeticOverflow` | 17 |

### `rate_client(id, rating)`

| # | Guard | Error | Code |
|---|---|---|---|
| 1 | `PAUSE_REPUTATION` is not active | `Paused` | 4 |
| 2 | Escrow exists in storage | `ContractNotFound` | 5 |
| 3 | Escrow is not frozen | `EscrowFrozen` | 22 |
| 4 | `c.freelancer.require_auth()` succeeds | Soroban auth failure | — |
| 5 | `c.status` is `Completed` or `Resolved` | `InvalidStatus` | 10 |
| 6 | `DataKey::ClientRated(id)` is `false` / absent | `ReputationAlreadyIssued` | 16 |
| 7 | `rating` in `1..=5` | `InvalidRating` | 15 |
| 8 | Checked arithmetic on `ClientReputation` | `ArithmeticOverflow` | 17 |

### `void_rating(id, reason)`

| # | Guard | Error | Code |
|---|---|---|---|
| 1 | `require_admin(&env)` succeeds | `NotInitialized` / `Unauthorized` / Soroban auth failure | 2 / 3 |
| 2 | `PAUSE_REPUTATION` is not active | `Paused` | 4 |
| 3 | `DataKey::Rating(id)` exists | `RatingNotFound` | 18 |
| 4 | `record.voided == false` | `RatingAlreadyVoided` | 19 |

---

## State Transitions

### Freelancer rating (`issue_reputation`)

- `c.reputation_issued`: `false -> true` (one-way)
- `DataKey::Reputation(freelancer)`: `completed_contracts += 1`, `total_rating += rating`
- `DataKey::WeightedRep(freelancer)`:
  - `weighted_rating_sum += c.released_amount * rating`
  - `weight_total += c.released_amount`
  - `rated_contracts += 1`
- `DataKey::RatingWeight(id)`: set to `c.released_amount`
- `DataKey::Rating(id)`: set to `RatingRecord { freelancer, rating, issued_at, voided: false }`

### Client rating (`rate_client`)

- `DataKey::ClientRated(id)`: `false -> true` (one-way)
- `DataKey::ClientReputation(client)`: `completed_contracts += 1`, `total_rating += rating`

### Voiding a freelancer rating (`void_rating`)

- `DataKey::Rating(id).voided`: `false -> true` (one-way)
- `DataKey::Reputation(freelancer)`: saturating subtraction of `record.rating` and `1`
- `DataKey::WeightedRep(freelancer)` (when `DataKey::RatingWeight(id)` is present):
  - `weighted_rating_sum = saturating_sub(weight * rating).max(0)`
  - `weight_total = saturating_sub(weight).max(0)`
  - `rated_contracts = saturating_sub(1)`

---

## Error Reference

| Error | Code | Meaning |
|---|---|---|
| `NotInitialized` | 2 | Contract admin is not initialized |
| `Unauthorized` | 3 | Caller is not authorized |
| `Paused` | 4 | `PAUSE_REPUTATION` flag is active |
| `ContractNotFound` | 5 | No escrow with the given ID |
| `InvalidStatus` | 10 | Escrow is not `Completed` or `Resolved` |
| `InvalidRating` | 15 | Rating outside `1..=5` |
| `ReputationAlreadyIssued` | 16 | Rating already issued for this escrow |
| `ArithmeticOverflow` | 17 | Checked arithmetic overflowed |
| `RatingNotFound` | 18 | No `RatingRecord` exists for the given escrow ID |
| `RatingAlreadyVoided` | 19 | Rating was already voided |
| `EscrowFrozen` | 22 | Escrow is currently frozen |
| `EscrowCompacted` | 26 | Escrow has been compacted to a receipt |
| `BelowReputationThreshold` | 30 | `released_amount < MIN_REPUTATION_VALUE` (`10_000_000`) |
| Soroban auth failure | — | Required signature not provided |
