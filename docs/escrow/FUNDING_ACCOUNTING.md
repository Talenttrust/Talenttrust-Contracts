# Funding Accounting Invariants

The live escrow contract tracks balances in `EscrowContractData`; it does not
transfer tokens and does not deduct protocol fees from milestone amounts.

## Implemented Invariants

- `amount > 0` for every deposit.
- Every milestone amount must be positive at creation time.
- Total milestone value must not exceed `MAX_TOTAL_ESCROW_STROOPS`.
- `ExactTotal` deposits must equal the full milestone sum and can happen only
  once.
- `Incremental` deposits can accumulate up to, but not beyond, the milestone
  sum.
- `release_milestone` requires enough available balance:
  `total_deposited - released_amount - refunded_amount >= milestone_amount`.
- Released milestones are recorded and cannot be released twice.
- After balance-changing operations, the contract checks that available balance
  is non-negative and that:
  `total_deposited == released_amount + refunded_amount + available_balance`.
- Protocol fees are calculated at milestone release using the formula:
  `fee = ceiling(amount * fee_bps / 10000)` (basis point calculation with rounding).
- Accumulated protocol fees are tracked separately in `DataKey::AccumulatedProtocolFees`.
- Protocol fees can only be withdrawn by the admin via `withdraw_protocol_fees`, which:
  - Verifies admin authorization via `require_auth()`.
  - Checks that accumulated fees >= withdrawal amount.
  - Atomically zeros the accumulator after withdrawal.
  - Emits a `fee_wd` event with (recipient, amount, timestamp).
- **Invariant**: Total fees withdrawn over the contract lifetime equals total fees accrued.
- **Invariant**: No protocol fee withdrawal is allowed when contract is paused or in emergency state (future enforcement).

## Implemented in This Release (#313, #314)

- Protocol fee accumulation during milestone releases via `calculate_protocol_fee`.
- Admin initialization with `initialize(admin, protocol_fee_bps)`.
- Admin-gated `withdraw_protocol_fees(admin, recipient, amount, token)`.
- Fee accounting invariants and auditable `fee_wd` events for indexing.
