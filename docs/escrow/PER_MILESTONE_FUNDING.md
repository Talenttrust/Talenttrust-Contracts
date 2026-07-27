# Per-Milestone Funding Tracking

The live escrow contract does not store independent per-milestone funded
amounts. It stores aggregate `total_deposited`, `released_amount`, and
`refunded_amount` on `EscrowContractData`, plus per-milestone release flags under
`DataKey::MilestoneReleased(contract_id, milestone_index)`.

## Implemented Behavior

- `deposit_funds` adds to aggregate deposited balance.
- `ExactTotal` mode requires a single full deposit.
- `Incremental` mode allows partial aggregate deposits until the milestone total
  is reached.
- `release_milestone` checks aggregate available balance before marking a
  milestone released.

## Not Implemented

There is no `set_milestone_funded` or `get_milestone_funded` entrypoint in
`contracts/escrow/src/lib.rs`, and release does not transfer tokens to the
freelancer.
