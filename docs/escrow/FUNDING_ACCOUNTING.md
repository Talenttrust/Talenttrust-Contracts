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

### Deposit Validation and Decision Boundaries

Deposit amount validation is performed via `validate_deposit_amount` which ensures:
1. The deposit amount itself is strictly positive (`> 0`) and doesn't exceed the single transaction maximum (`MAX_SINGLE_AMOUNT_STROOPS`).
2. The deposit does not overflow the `i128` integer capacity when added to the current funded amount.
3. The resulting total deposit amount does not exceed the contract's total maximum capacity.

The decision boundaries for deposits are validated as follows:
- **Exactly-remaining**: If the deposit matches the remaining capacity exactly (`deposit + current == max_total`), it is accepted.
- **One stroop short**: If the deposit leaves exactly one stroop remaining (`deposit + current == max_total - 1`), it is accepted.
- **One stroop over**: If the deposit exceeds the remaining capacity by even one stroop (`deposit + current == max_total + 1`), it is rejected with `EscrowError::InvalidMilestoneAmount`.
- **Already fully funded**: If the contract is already fully funded (`current == max_total`), any further deposit is rejected with `EscrowError::InvalidMilestoneAmount`.

## Not Implemented

Protocol fee deduction, accumulated protocol fees, and protocol fee withdrawal
are planned in
[#313](https://github.com/Talenttrust/Talenttrust-Contracts/issues/313) and
[#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).
