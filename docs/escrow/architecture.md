# Escrow Architecture

The live escrow contract is implemented in `contracts/escrow/src/lib.rs`.

## Current Components

- `EscrowContractData` stores client, freelancer, optional arbiter field,
  milestone amounts, status, aggregate accounting, reputation flag, and deposit
  mode.
- `DataKey::Contract(id)` stores each escrow record.
- `DataKey::MilestoneReleased(id, index)` stores one-way milestone release flags.
- Admin, pause, emergency, readiness, and reputation records are stored in
  persistent storage.

## Current Flow

1. `initialize(admin)` sets the operational admin.
2. `create_contract` stores a new contract in `Created`.
3. `deposit_funds` moves aggregate accounting to `PartiallyFunded` or `Funded`.
4. `release_milestone` marks individual milestones released and completes the
   contract after the final release.
5. `issue_reputation` records one client-issued freelancer rating.
6. `cancel_contract` cancels non-completed contracts by client/freelancer auth.

## Storage reference

A fuller storage reference, including the canonical data keys, invariants, and
entrypoints that read or write them, is available in [docs/storage.md](../storage.md).

## Not Implemented

Approval modes, dispute resolution, refunds, finalization, protocol fees,
two-step admin transfer, and migration entrypoints are planned work, not current
architecture.
