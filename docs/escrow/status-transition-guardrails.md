# Escrow Contract Status Transition Guardrails

The live escrow contract uses explicit status writes inside
`contracts/escrow/src/lib.rs`.

## Implemented Transitions

- `Created -> PartiallyFunded` when an incremental deposit is below the
  milestone total.
- `Created -> Funded` when an exact deposit, or final incremental deposit,
  reaches the milestone total.
- `PartiallyFunded -> Funded` when incremental deposits reach the milestone
  total.
- `Created`, `PartiallyFunded`, or `Funded -> Cancelled` through
  `cancel_contract`.
- `Funded -> Completed` when every milestone has been released.

## Guardrails

- Deposit amounts must be positive.
- Exact-total contracts accept exactly one full deposit.
- Incremental deposits cannot exceed the milestone total.
- Milestones can be released only once.
- Releases require enough available funded balance.
- Completed contracts cannot be cancelled.
- Already-cancelled contracts cannot be cancelled again.
- Paused or emergency state blocks mutating lifecycle operations.

## Planned

Dispute, refund, approval-expiry, and finalization transitions are not
implemented in `lib.rs`. `finalize_contract` is tracked in
[#320](https://github.com/Talenttrust/Talenttrust-Contracts/issues/320).
