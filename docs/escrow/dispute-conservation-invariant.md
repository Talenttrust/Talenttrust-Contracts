# Dispute Conservation Invariant

## Invariant

After `resolve_dispute` completes, the following must hold for every contract:

```
released_amount + refunded_amount == funded_amount
```

`funded_amount` is the total amount ever deposited into the escrow. `released_amount` accumulates all funds paid to the freelancer (via milestone releases and dispute resolutions). `refunded_amount` accumulates all funds returned to the client (via refunds and dispute resolutions). No value is created or destroyed; every stroop is accounted for exactly once.

## Where it is enforced

`contracts/escrow/src/lib.rs` — `resolve_dispute`:

1. `resolution_payouts` computes `(client_payout, freelancer_payout)` over the remaining available balance (`funded_amount - released_amount - refunded_amount`).
2. Both legs are added to their respective accumulators atomically.
3. The on-chain guard checks `released_amount + refunded_amount == funded_amount` and panics with `AccountingInvariantViolated` if violated.

`contracts/escrow/src/dispute.rs` — `resolution_payouts`:

- Computes `available = funded_amount - released_amount - refunded_amount`.
- Returns `Err(AccountingInvariantViolated)` if `available < 0`.
- Returns `Err(InvalidDisputeSplit)` for any `Split(a, b)` where `a + b != available` or either leg is negative.
- This check fires **before** the on-chain invariant guard, so a malformed split is rejected cleanly.

## Resolution modes

| Mode           | client_payout     | freelancer_payout        | Final status           |
| -------------- | ----------------- | ------------------------ | ---------------------- |
| `FullRefund`   | `available`       | `0`                      | `Refunded`             |
| `FullPayout`   | `0`               | `available`              | `Completed`            |
| `PartialRefund`| `available - ⌊available×30/100⌋` | `⌊available×30/100⌋` | `Completed` |
| `Split(a, b)`  | `a`               | `b` (requires `a+b == available`) | `Completed` or `Refunded` |

`final_status_after_resolution` returns `Refunded` if and only if `refunded_amount == funded_amount` after the resolution; otherwise it returns `Completed`.

## Behavior with prior partial releases

When milestones have been released before a dispute is raised, `available` is less than `funded_amount`. The conservation invariant still holds because `resolution_payouts` operates only on the remaining available balance, and the existing `released_amount` is preserved:

```
released_amount (pre-dispute) + freelancer_payout (dispute)
  + client_payout (dispute)
== funded_amount
```

Example — 3-milestone contract (50 / 60 / 40 = 150 total), milestone 0 released before dispute:

| Moment           | released | refunded | funded | available |
| ---------------- | -------- | -------- | ------ | --------- |
| After deposit    | 0        | 0        | 150    | 150       |
| After release M0 | 50       | 0        | 150    | 100       |
| After FullRefund | 50       | 100      | 150    | 0 ✓       |
| After FullPayout | 150      | 0        | 150    | 0 ✓       |
| After PartialRef | 80       | 70       | 150    | 0 ✓       |

## Test coverage

`contracts/escrow/src/test/dispute.rs` covers:

- Unit tests for `resolution_payouts`: all four resolutions, rounding boundaries, negative legs, non-conserving sums, overflow, and corrupted accounting state.
- Integration tests for `raise_dispute` and `resolve_dispute`: access-control, state-machine guards, pause/emergency gates, and double-resolution prevention.
- **Conservation tests with no prior releases**: FullRefund, FullPayout, PartialRefund, Split.
- **Conservation tests with one prior release**: FullRefund, FullPayout, PartialRefund, Split, and rejection of a split sized to the total rather than the remaining available.
- Event payload test: confirms the `("dispute", "resolved")` event is emitted after `resolve_dispute`.
- Multi-contract isolation: disputes on independent contracts do not cross-contaminate accounting.
