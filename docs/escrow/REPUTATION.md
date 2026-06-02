# Reputation Credential Issuance

The Escrow contract issues reputation credentials (ratings) to freelancers after a contract reaches `Completed` status.

## Validation Rules

1. **Client authorization:** Only the contract client may call `issue_reputation`. Unauthorized callers fail with `UnauthorizedRole`.
2. **Freelancer match:** The supplied freelancer address must match the contract's stored freelancer. Mismatches fail with `FreelancerMismatch`.
3. **Contract completion gating:** Reputation can only be issued after the contract is `Completed`. Non-completed contracts fail with `NotCompleted`.
4. **Rating bounds:** Ratings must be between `1` and `5` inclusive. Values outside this range fail with `InvalidRating`.
5. **Duplicate issuance protection:** Reputation may only be issued once per contract. Subsequent attempts fail with `ReputationAlreadyIssued`.
6. **Self-rating guard:** If the contract client and freelancer are the same address, reputation issuance fails with `SelfRating` (defense-in-depth even if create-time check is bypassed).

## Reputation Aggregation

Successful issuance updates the freelancer's aggregate `ReputationRecord`:

- `completed_contracts` increments by `1`
- `total_rating` increases by the rating value
- `last_rating` is set to the most recent rating

Pending reputation credits are also decremented on success.

## Test Coverage

The escrow test suite now includes dedicated coverage for the `issue_reputation` negative paths in `contracts/escrow/src/test/reputation.rs`.

- unauthorized caller
- freelancer mismatch
- non-completed contract
- invalid rating bounds
- duplicate issuance
- self-rating guard
- verified reputation aggregation and pending credit decrement on success

## Average Rating Accessor

The contract exposes `get_average_rating(freelancer) -> Option<i128>` as a read-only helper for consumer convenience. The returned integer is scaled by 100, so `450` represents an average rating of `4.50`.

- Returns `None` when the freelancer has no completed contracts.
- Returns `Some(value)` when `completed_contracts > 0`.
- The result is computed as `(total_rating * 100) / completed_contracts`.

## Security Assumptions

- **Access Control:** `issue_reputation` requires client authentication.
- **Contract Completion:** Only `Completed` contracts are eligible for reputation issuance.
- **Duplicate issuance guard:** Repeat issuance is blocked by a stored `ReputationIssued` flag.
- **Aggregate consistency:** Reputation totals and pending credits are updated atomically.
