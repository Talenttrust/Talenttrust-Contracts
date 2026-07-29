# Escrow Timeout Behavior

Milestone timeout detection is implemented via `Escrow::is_milestone_overdue`,
which reads the ledger timestamp through the centralised `utils::now_seconds`
helper.

## How it works

A milestone is considered **overdue** when all of the following hold:

1. The contract and milestone index exist in storage.
2. The milestone has a `deadline` set (`Some(value)`).
3. The milestone has **not** already been released.
4. `now_seconds(&env) > deadline` (strictly greater).

At exactly the deadline the milestone is **not** overdue — the strict-inequality
boundary gives the freelancer the full deadline window.

## What uses it

`is_milestone_overdue` is called inside `refund_unreleased_milestones` to gate
timeout-driven refunds. A milestone with a deadline may only be refunded by the
client once it has become overdue.

## Time source

All time operations flow through `utils::now_seconds`, which reads
`env.ledger().timestamp()`. See [ledger-time-source.md](ledger-time-source.md)
for precision, trust assumptions, and testing guidance.
