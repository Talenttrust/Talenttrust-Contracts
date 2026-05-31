# Funding Accounting Invariants

The live escrow contract tracks balances in `EscrowContractData`; it does not
transfer tokens but does track protocol fees.

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
- Released milestones are recorded under `MilestoneReleased(contract_id, index)`
  and cannot be released twice.
- After balance-changing operations, the contract checks that available balance
  is non-negative and that:
  `total_deposited == released_amount + refunded_amount + available_balance`.

## Protocol Fee Accounting Invariants

- Protocol fee is set in basis points (bps) with a maximum of 1000 bps (10%).
- Default protocol fee is 0 bps.
- When a milestone is released:
  - Fee is calculated as `milestone_amount * fee_bps / 10000` (rounded down).
  - Net amount to freelancer is `milestone_amount - fee`.
  - Fee is added to `AccumulatedProtocolFees`.
- Only admin can set or change the protocol fee.
- `fee + net_amount == milestone_amount` for all released milestones.
