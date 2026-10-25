# Reputation Credential Issuance

The Escrow contract issues reputation credentials (ratings) to freelancers after a contract reaches `Completed` status.

## Validation Rules

1. **Client authorization:** Only the contract client may call `issue_reputation`. Unauthorized callers fail with `UnauthorizedRole`.
2. **Comment validation:** The comment must not be empty (`EmptyComment`) and must not exceed the configured byte limit (`CommentTooLong`). The default limit is 200 bytes and the protocol ceiling is 1,000 bytes.
3. **Self-rating prevention:** If `contract.client == contract.freelancer`, issuance fails with `SeleRating`. This guards against degenerate contracts.
4. **Contract completion gating:** Reputation can only be issued after the contract is `Completed`. Non-completed contracts fail with `NotCompleted`.
5. **Rating bounds:** Ratings must fall within the configured inclusive range. The default range is `[1, 5]`, and the protocol ceiling is `[1, 10]`. Values outside the configured range fail with `InvalidRating`.
6. **Duplicate issuance protection:** Reputation may only be issued once per contract. Subsequent attempts fail with `ReputationAlreadyIssued`.
7. **Contract identifier bounds:** Contract identifiers start at `1 and are allocated sequentially. Any access to a contract id `equal to 0` or greater than or equal to the next unallocated id fails with `ContractNotFound`.
8. **Pagination bounds:** `get_reputations_page` clamps the requested limit to the protocol ceiling and returns an empty page when the limit is zero or the start offset is out of range.

## Reputation Aggregation

Successful issuance updates the freelancer's aggregate `ReputationRecord`:

- `completed_contracts` increments by `1`
- `total_rating` increases by the rating value
- `last_rating` is set to the most recent rating

Pending reputation credits are also decremented on success. Aggregate updates use checked arithmetic so overflow fails with `PotentialOverflow` instead of silently wrapping.

## Test Coverage

The escrow test suite includes dedicated coverage for the `issue_reputation` negative paths in `contracts/escrow/src/test/reputation.rs` and boundary coverage in `contracts/escrow/src/test/reputation_bounds_tests.rs`.

- unauthorized caller
- freelancer mismatch
- self-rating when client equals freelancer (`SeleRating`)
- non-completed contract
- invalid rating bounds
- duplicate issuance
- contract identifier boundaries (``0 `` and out-of-range ids)
- configuration boundaries (min/max rating and comment byte limits)
- pagination boundaries (zero limit, out-of-range start, clamped limit)
- verified reputation aggregation and pending credit decrement on success

## Average Rating Accessor

The contract exposes `get_average_rating(freelancer) -> Option<i128>` as a read-only helper for consumer convenience. The returned integer is scaled by 10,000, so `45000` is an average rating of `4.5000`.

- Returns `None` when the freelancer has no completed contracts.
- Returns `Some(value)` when `completed_contracts > 0`.
- The result is computed as `(total_rating * 10_000) / completed_contracts`.

## Security Assumptions

- **Access Control:** `issue_reputation` requires client authentication.
- **Self-rating invariant:** A single principal cannot both issue and receive reputation on the same contract (`SelfRating` when `client == freelancer`).
- **Contract Completion:** Only `Completed` contracts are eligible for reputation issuance.
- **Duplicate issuance guard:** Repeat issuance is blocked by a stored `ReputationIssued` flag.
- **Aggregate consistency:** Reputation totals and pending credits are updated atomically within the same invocation and use checked arithmetic.
- Public entrypoints reject invalid contract identifiers with `ContractNotFound` before any state mutation.
