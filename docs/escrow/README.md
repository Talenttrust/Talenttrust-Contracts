# Escrow Contract

## Summary

The escrow contract stores a client, a freelancer, and a fixed list of milestone balances.
Funds are tracked internally through three counters:

- `funded_amount`: total deposits accepted from the client
- `released_amount`: total milestone value already released to the freelancer
- `refunded_amount`: total milestone value already refunded to the client

The refundable balance is derived as:

```text
funded_amount - released_amount - refunded_amount
```

## Partial refund behavior

`refund_unreleased_milestones(contract_id, milestone_ids)` enables the client to recover balances tied to unreleased milestones.

Each requested milestone must satisfy all of the following:

- it exists
- it has not already been released
- it has not already been refunded
- it appears only once in the request

The function computes the full refund amount first and only mutates milestone flags after all validations pass. This keeps the flow easy to audit and avoids partial logical updates within a successful call.

## State transitions

- `Created`: contract exists and is not yet fully funded
- `Funded`: deposits now equal the total milestone value and unresolved milestones still remain
- `Completed`: every milestone has been released
- `Refunded`: every unresolved milestone has been refunded
- `Disputed`: reserved for future dispute handling

Notes:

- A contract can remain `Funded` after a partial refund if at least one unresolved milestone still exists.
- Once a contract becomes `Completed` or `Refunded`, further deposits, releases, and refunds are rejected.

## Security assumptions

- Only the stored client address can create the contract, deposit funds, release milestones, or trigger refunds.
- Deposits cannot exceed the original milestone total.
- Releases and refunds both consume the same escrow balance, preventing the same funded value from being spent twice.
- A refunded milestone can never be released later.
- A released milestone can never be refunded later.
- Duplicate milestone IDs in a single refund request are rejected.
- Empty milestone sets, zero-value milestones, and zero-value deposits are rejected.

## Threat scenarios reviewed

- Double refund attempt: blocked by milestone `refunded` flag checks.
- Release after refund: blocked by milestone status validation.
- Refund after release: blocked by milestone status validation.
- Overfunding: blocked by total milestone cap enforcement.
- Underfunded release/refund: blocked when the derived escrow balance is insufficient.
- Replay on terminal contracts: blocked by status gating once the contract is fully resolved.

## Test coverage focus

The escrow tests are grouped into dedicated modules:

- `create_contract.rs`
- `deposit.rs`
- `release.rs`
- `refund.rs`

Covered scenarios include:

- successful creation, deposit, release, and refund flows
- invalid participant and milestone input
- overfunding and zero-value deposit rejection
- invalid milestone selection
- duplicate refund requests
- double release and double refund protection
- insufficient-balance failure paths
- terminal-state operation rejection

## Reviewer checklist

- Confirm the milestone-based refund model matches product expectations.
- Confirm client-only authorization is the intended release policy.
- Confirm `Refunded` as a terminal status is acceptable for fully refunded contracts.
- Confirm token transfer integration, if added later, preserves the same accounting invariant before and after transfer side effects.
