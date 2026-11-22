# Threat Model: Reputation

Scope: `EscrowContract::issue_reputation`, `EscrowContract::rate_client`, and
`EscrowContract::void_rating` in `contracts/escrow/src/lib.rs`, together with
their persistent storage keys:
- `DataKey::Reputation(freelancer)`
- `DataKey::WeightedRep(freelancer)`
- `DataKey::Rating(id)`
- `DataKey::RatingWeight(id)`
- `DataKey::ClientReputation(client)`
- `DataKey::ClientRated(id)`
- `EscrowContract::reputation_issued`

## Trust assumptions

- `c.client`, `c.freelancer`, and `c.released_amount` are trusted contract
  fields maintained by the escrow lifecycle in `contracts/escrow/src/lib.rs` and
  are not directly writable by callers.
- Soroban's `Address::require_auth()` is trusted to prove the transaction was
  authorized by the required address (`c.client`, `c.freelancer`, or admin).
- `id`, `rating`, and `reason` are **untrusted, attacker-controlled** call
  arguments.
- Sybil resistance relies on the economic cost of releasing at least
  `MIN_REPUTATION_VALUE` (`10_000_000`, 1 token unit at 7 decimals) to the
  freelancer before a rating can be issued, and on weighting each rating by
  `c.released_amount`.

## Attacker capabilities

An attacker can invoke `issue_reputation(id, rating)`, `rate_client(id, rating)`,
or `void_rating(id, reason)` with any `id`, `rating`, or `reason`. They cannot
forge `require_auth()` for an address they do not control.

## Mitigations, mapped to the actual checks

1. **Pause and freeze gating** — `require_not_paused(&env, PAUSE_REPUTATION)`
   and `require_not_frozen(&env, id)` allow governance to halt reputation
   mutations during incidents or per-escrow freezes.
2. **Compaction safety** — `issue_reputation` rejects compacted escrows with
   `EscrowCompacted`, while compaction of unrated terminal escrows is blocked
   until `THIRTY_DAYS_IN_LEDGERS` has elapsed so compaction cannot be abused to
   suppress a rating.
3. **Signature verification** — `c.client.require_auth()` (for
   `issue_reputation`), `c.freelancer.require_auth()` (for `rate_client`), and
   `require_admin(&env)` (for `void_rating`) run before lifecycle, idempotency,
   and bounds checks, ensuring unauthorized callers cannot mutate state.
4. **Lifecycle gating** — `issue_reputation` and `rate_client` require
   `ContractStatus::Completed | ContractStatus::Resolved` (`InvalidStatus`),
   preventing ratings on unfunded, open, cancelled, or disputed escrows.
5. **Idempotency** — `c.reputation_issued` (for freelancer ratings) and
   `DataKey::ClientRated(id)` (for client ratings) block duplicate ratings with
   `ReputationAlreadyIssued`. `RatingRecord.voided` blocks duplicate voiding
   with `RatingAlreadyVoided`.
6. **Rating bounds** — `!(1..=5).contains(&rating)` fails with `InvalidRating`.
7. **Value threshold & weighting** — `issue_reputation` requires
   `c.released_amount >= MIN_REPUTATION_VALUE` (`BelowReputationThreshold`) and
   weights each rating by `c.released_amount` in `WeightedReputation`.
   `reputation_score` computes an overflow-safe, half-up rounded score in
   hundredths.
8. **Checked and saturating arithmetic** — `issue_reputation` and `rate_client`
   use checked arithmetic (`ArithmeticOverflow`), while `void_rating` uses
   saturating subtraction clamped at `0` so reversing a rating cannot underflow.
9. **Governance voiding & audit trail** — `void_rating` reverses both
   unweighted `Reputation` and `WeightedReputation` (using `DataKey::RatingWeight(id)`),
   marks `RatingRecord.voided = true`, and emits a `("rep_void", id)` event with
   the `reason` symbol.

## Known limitations

- **Existence/freeze oracle before auth**: In `issue_reputation` and
  `rate_client`, `load(&env, id)` and `require_not_frozen(&env, id)` run before
  `require_auth()`. An unauthenticated caller can therefore distinguish
  `ContractNotFound`, `EscrowCompacted`, and `EscrowFrozen` from an auth
  failure. This is a low-severity information-disclosure issue only; no state
  can be mutated without valid authorization.
- **Client ratings are unweighted and non-voidable**: `rate_client` does not
  enforce `MIN_REPUTATION_VALUE`, does not maintain a `WeightedReputation`
  aggregate, and `void_rating` operates only on freelancer `RatingRecord`
  entries (`DataKey::Rating(id)`).
