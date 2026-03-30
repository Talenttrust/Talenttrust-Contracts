# Reputation Credential Issuance

The Escrow contract issues reputation credentials (ratings) to freelancers upon the successful completion of a milestone or project. This module contains validations to ensure the integrity of the reputation system.

## Validation Rules

1. **Rating Bounds:** Must be between 1 and 5 (inclusive). Ratings outside these bounds will be rejected with an `InvalidRating` error.
2. **Issuance Timing:** Credentials can only be issued if the project is completely finished (i.e. status is `Completed`). If the project is in `Created`, `Funded`, or `Disputed` state, issuing ratings will fail with a `NotCompleted` error.
3. **Duplicate Prevention:** A freelancer can only receive exactly one rating credential per contract (project). Subsequent attempts to issue a rating will fail with a `DuplicateRating` error.

## Security Assumptions

- **Access Control:** `issue_reputation` should preferably be restricted to authenticated clients or protocol administrators.
- **Contract Completion:** We rely on the `release_milestone` equivalent logic correctly transitioning the overall contract status into `Completed`.
- **Duplicate tracking state:** The persistent storage maps `DataKey::Reputation(contract_id, freelancer_address)` to a rating value effectively preventing duplicated logs.

## Threat Scenarios

- **Duplicate rating attack:** Attackers or clients attempting to unfairly inflate or deflate a freelancer's score by rating repeatedly on the same job. Prevented by checking the reputation map before issuance.
- **Early rating attack:** Clients attempting to lock in a rating or rate negatively prematurely before finishing escrow obligations. Prevented by enforcing the `Completed` state as an issuance prerequisite.
- **Out-of-bounds rating attack:** Attackers attempting to provide extremely high ratings to manipulate global average calculations. Prevented by enforcing the `1 <= rating <= 5` boundary natively in the Escrow contract.
