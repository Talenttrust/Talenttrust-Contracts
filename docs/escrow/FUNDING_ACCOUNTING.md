# Funding Accounting Invariants

The live escrow contract tracks balances in `EscrowContractData`; it does not
transfer tokens and does not deduct protocol fees.

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

## Not Implemented

Protocol fee deduction, accumulated protocol fees, and protocol fee withdrawal
are planned in
[#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313) and
[#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).
