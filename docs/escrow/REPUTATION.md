# Escrow Reputation System

The Escrow contract (`contracts/escrow/src/lib.rs`) tracks value-weighted freelancer reputation, client ratings, and governance-controlled rating voiding.

## Overview

9. **Value threshold:** Ratings are backed by released value. `issue_reputation` and `rate_client` both fail with `BelowReputationThreshold` when `contract.released_amount < MIN_REPUTATION_VALUE` (`10_000_000` stroops, 1 token at 7 decimals), so dust escrows cannot move reputation in either direction. An escrow at exactly `MIN_REPUTATION_VALUE` is eligible. The check runs after the rating-bounds check and before any state mutation, so the error ordering is the same on both entrypoints.

## Client Ratings

After an escrow reaches `Completed` or `Resolved`, the freelancer may call `rate_client(id, rating)` to rate the client once per escrow (`1..=5`). The call:

- requires freelancer authorization (`UnauthorizedRole` otherwise)
- rejects escrows that are not `Completed`/`Resolved` (`InvalidStatus`)
- rejects a second rating for the same escrow (`ReputationAlreadyIssued`)
- rejects ratings outside `1..=5` (`InvalidRating`)
- rejects escrows below `MIN_REPUTATION_VALUE` (`BelowReputationThreshold`)

Successful issuance increments the client's `Reputation { completed_contracts, total_rating }` and emits `("rep_cli", id) -> (client, rating)`.
- **Freelancer ratings (`issue_reputation`)**: After an escrow reaches `Completed` or `Resolved`, the client may rate the freelancer (`1..=5`) once per contract, provided the released amount meets `MIN_REPUTATION_VALUE` (`10_000_000`, i.e. 1 token unit at 7 decimals).
- **Client ratings (`rate_client`)**: After an escrow reaches `Completed` or `Resolved`, the freelancer may rate the client (`1..=5`) once per contract.
- **Rating voiding (`void_rating`)**: The contract admin may void a fraudulent freelancer rating, reversing its effect on both unweighted `Reputation` and `WeightedReputation` while leaving an on-chain audit trail.

## Data Structures

### `Reputation`

Stored under `DataKey::Reputation(freelancer)` and `DataKey::ClientReputation(client)`:

- `completed_contracts: u32` — count of rated contracts
- `total_rating: u32` — sum of ratings

### `WeightedReputation`

Stored under `DataKey::WeightedRep(freelancer)`:

- client ratings: success, authorization, duplicates, rating bounds, status gating, pause gate, dust rejection (`BelowReputationThreshold`) and the exact-threshold boundary
- `weighted_rating_sum: i128` — sum of `released_amount * rating`
- `weight_total: i128` — sum of `released_amount`
- `rated_contracts: u32` — count of value-weighted rated contracts

### `RatingRecord`

Stored under `DataKey::Rating(id)`:

- `freelancer: Address`
- `rating: u32`
- `issued_at: u64` — ledger timestamp when issued
- `voided: bool` — set to `true` by `void_rating`

The weight used for a rating is stored separately under `DataKey::RatingWeight(id)` (`i128`) so `void_rating` can reverse the exact weighted contribution.

## Mutating Entrypoints

### `issue_reputation(env, id: u64, rating: u32) -> Result<(), EscrowError>`

1. **Pause check**: Fails with `Paused` if `PAUSE_REPUTATION` is active.
2. **Compaction check**: Fails with `EscrowCompacted` if the escrow has been compacted (unrated terminal escrows cannot be compacted until `THIRTY_DAYS_IN_LEDGERS` has elapsed).
3. **Existence & freeze check**: Loads the escrow (`ContractNotFound`) and requires it not to be frozen (`EscrowFrozen`).
4. **Client authorization**: Calls `c.client.require_auth()`.
5. **Lifecycle gating**: Contract status must be `Completed` or `Resolved` (`InvalidStatus`).
6. **Duplicate issuance protection**: Fails with `ReputationAlreadyIssued` if `c.reputation_issued` is already `true`.
7. **Rating bounds**: `rating` must be in `1..=5` (`InvalidRating`).
8. **Value threshold**: `weight = c.released_amount` must be `>= MIN_REPUTATION_VALUE` (`10_000_000`), otherwise fails with `BelowReputationThreshold`.
9. **State updates** (checked arithmetic, failing with `ArithmeticOverflow` on overflow):
   - Updates `Reputation`: `completed_contracts += 1`, `total_rating += rating`.
   - Updates `WeightedReputation`: `weighted_rating_sum += weight * rating`, `weight_total += weight`, `rated_contracts += 1`.
   - Persists `DataKey::RatingWeight(id) = weight` and `DataKey::Rating(id) = RatingRecord { freelancer, rating, issued_at, voided: false }`.
   - Sets `c.reputation_issued = true` and emits a `ReputationEvent`.

### `rate_client(env, id: u64, rating: u32) -> Result<(), EscrowError>`

1. **Pause check**: Fails with `Paused` if `PAUSE_REPUTATION` is active.
2. **Existence & freeze check**: Loads the escrow (`ContractNotFound`) and requires it not to be frozen (`EscrowFrozen`).
3. **Freelancer authorization**: Calls `c.freelancer.require_auth()`.
4. **Lifecycle gating**: Contract status must be `Completed` or `Resolved` (`InvalidStatus`).
5. **Duplicate issuance protection**: Checks `DataKey::ClientRated(id)`; fails with `ReputationAlreadyIssued` if already `true`.
6. **Rating bounds**: `rating` must be in `1..=5` (`InvalidRating`).
7. **State updates**: Updates `DataKey::ClientReputation(c.client)` with checked arithmetic (`ArithmeticOverflow`), sets `DataKey::ClientRated(id) = true`, and publishes the `("rep_cli", id)` event.

### `void_rating(env, id: u64, reason: Symbol) -> Result<(), EscrowError>`

1. **Admin authorization**: Requires admin via `require_admin(&env)` (`Unauthorized` / `NotInitialized`).
2. **Pause check**: Fails with `Paused` if `PAUSE_REPUTATION` is active.
3. **Record lookup**: Loads `DataKey::Rating(id)`, failing with `RatingNotFound` if absent.
4. **Idempotency**: Fails with `RatingAlreadyVoided` if `record.voided` is already `true`.
5. **Reversal**:
   - Subtracts `record.rating` from `Reputation.total_rating` and `1` from `Reputation.completed_contracts` using saturating subtraction.
   - If `DataKey::RatingWeight(id)` exists, subtracts `weight * record.rating` from `WeightedReputation.weighted_rating_sum` (clamped at `>= 0`), subtracts `weight` from `WeightedReputation.weight_total` (clamped at `>= 0`), and decrements `WeightedReputation.rated_contracts` (saturating). Legacy ratings issued before weighting existed have no stored weight and only reverse the unweighted aggregate.
   - Marks `record.voided = true` and publishes the `("rep_void", id)` event with `(freelancer, rating, reason)`.

## Score Computation & Weighting

Each freelancer rating is weighted by the escrow's `released_amount` (`weight`). The contract exposes:

- `get_reputation(freelancer) -> Reputation` — unweighted aggregate (`completed_contracts`, `total_rating`)
- `get_weighted_reputation(freelancer) -> WeightedReputation` — weighted aggregate (`weighted_rating_sum`, `weight_total`, `rated_contracts`)
- `get_reputation_score(freelancer) -> Option<u32>` — value-weighted score in hundredths (e.g. `437` = `4.37`)
- `get_rating(id) -> Option<RatingRecord>` — per-contract rating record and voided status
- `get_client_reputation(client) -> Reputation` — unweighted client aggregate
- `extend_reputation_ttl(freelancer)` — extends persistent storage TTL for a freelancer's reputation record

`get_reputation_score` delegates to `reputation_score(w.weighted_rating_sum, w.weight_total)`:

- Returns `None` when `total <= 0` or `sum < 0` (including when the freelancer has no eligible ratings or all ratings have been voided).
- Computes overflow-safe `round(sum * 100 / total)` in hundredths:
  - `whole = sum / total`, `rem = sum % total`
  - Scales `rem * 100` (with fallback `rem / (total / 100)` when `rem * 100` would overflow `i128`) and computes `q = scaled / total`, `r = scaled % total`.
  - Applies **half-up rounding**: increments `q` by `1` when `r >= total - r` (equivalent to `2 * r >= total`, without overflow).
  - Returns `whole * 100 + frac` as `Option<u32>`.

## Security Assumptions

- **Dust resistance:** Both rating directions require `released_amount >= MIN_REPUTATION_VALUE`, so a cheap escrow cannot be used to inflate a freelancer's or a client's reputation.
- **Sybil resistance**: Requiring `released_amount >= MIN_REPUTATION_VALUE` (`10_000_000`) ensures farming reputation costs real released value.
- **Access control**: `issue_reputation` requires `c.client.require_auth()`, `rate_client` requires `c.freelancer.require_auth()`, and `void_rating` requires `require_admin`.
- **One-shot issuance**: `c.reputation_issued` and `DataKey::ClientRated(id)` prevent duplicate ratings per contract.
- **Safe reversal**: `void_rating` uses saturating arithmetic and marks `RatingRecord.voided = true` so a rating cannot be voided twice.
