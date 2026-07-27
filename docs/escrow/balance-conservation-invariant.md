# Token Balance Conservation Invariant

The escrow contract moves a real Stellar Asset Contract (SAC) token in
`deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, and
`cancel_contract`. The on-chain token balance held by the contract must always
mirror its internal accounting books.

## Invariant

At every step of a contract's lifecycle, the escrow contract's actual SAC token
balance MUST equal its derived accounting balance:

```
contract_token_balance == funded_amount
                        - released_amount
                        - refunded_amount
                        + accumulated_protocol_fees
```

Equivalently, the contract never holds **less than it owes** nor **more than was
deposited**. Accrued protocol fees remain held in-contract until explicitly
withdrawn via `withdraw_protocol_fees`, and a withdrawal reduces the on-chain
balance by exactly the withdrawn amount.

## Where each term changes

| Operation                       | Effect on accounting                                  | Effect on token balance                |
|---------------------------------|--------------------------------------------------------|----------------------------------------|
| `deposit_funds`                 | `funded_amount += amount`                              | `+amount` (pulled from client)         |
| `release_milestone`             | `released_amount += payout`; fee accrues to `AccumulatedProtocolFees` | `-payout` (pushed to freelancer); fee retained |
| `refund_unreleased_milestones`  | `refunded_amount += refund`                            | `-refund` (pushed to client)           |
| `cancel_contract`               | `refunded_amount += remaining`                         | `-remaining` (full balance to client)  |
| `withdraw_protocol_fees`        | `AccumulatedProtocolFees -= amount`                   | `-amount` (pushed to fee recipient)    |

## Security notes

- The protocol fee charged on a release is **retained in the contract** (not
  paid out) and tracked in `AccumulatedProtocolFees`; it leaves the contract
  only through `withdraw_protocol_fees`.
- Because every token movement is paired with the matching accounting mutation
  inside the same entrypoint, any future drift between the ledger and the books
  is a bug. The lifecycle test in
  `contracts/escrow/src/test/accounting_invariants.rs` asserts this invariant
  after each operation to catch such drift.

## Tests

See `contracts/escrow/src/test/accounting_invariants.rs`:

- `balance_conserved_through_deposit` — balance equals `funded_amount` after deposit.
- `balance_conserved_when_cancel_returns_full_remaining_balance` — cancel returns
  the full remaining balance to the client and leaves the contract holding zero.
- `cancel_without_deposit_moves_no_tokens` — cancelling a never-funded contract
  moves no tokens.
