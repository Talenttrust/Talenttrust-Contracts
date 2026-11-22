# Milestones Error Reference

This is a topic-specific view of `EscrowError` codes reachable from milestone creation, submission, review, release, and change-order paths.

- **Canonical error catalog:** [`docs/escrow/ERROR_CATALOG.md`](escrow/ERROR_CATALOG.md)
- **Canonical enum definition:** [`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs) (`EscrowError`, codes `1..=50`)
- **Related views:** [General Contract Errors](contracts-errors.md) · [Arbiter & Panel Errors](arbiter-errors.md)

## Milestone Path Errors

- **`InvalidMilestones` (7)** — `create_contract`, `create_contract_with_review`, `create_contract_sequential`, `release_milestones`, `propose_change_order`, `accept_change_order`: empty or invalid milestone list.
- **`InvalidAmount` (8)** — `release_partial`, `propose_change_order`, `accept_change_order`: non-positive or invalid payout/milestone amount.
- **`InvalidStatus` (10)** — `submit_milestone`, `claim_milestone`, `deliver`, `release_milestone`, `release_partial`, `release_milestones`, `propose_change_order`, `accept_change_order`: escrow status does not permit the milestone action.
- **`MilestoneNotFound` (11)** — `submit_milestone`, `claim_milestone`, `deliver`, `release_milestone`, `release_partial`, `release_milestones`, `get_milestone`, `get_milestone_status`, `get_milestone_paid`: milestone index is out of bounds.
- **`MilestoneAlreadyReleased` (12)** — `submit_milestone`, `claim_milestone`, `deliver`, `release_milestone`, `release_partial`, `release_milestones`: target milestone is already released.
- **`InsufficientFunds` (13)** — `submit_milestone`, `claim_milestone`, `release_milestone`, `release_partial`, `release_milestones`: available funded balance is insufficient for the milestone payout.
- **`ArithmeticOverflow` (17)** — `submit_milestone`, `claim_milestone`, `release_milestone`, `release_partial`, `release_milestones`, `accept_change_order`: checked arithmetic overflow during milestone accounting.
- **`MilestoneTooSmall` (24)** — `create_contract` (and creation variants): milestone amount is below the minimum allowed floor.
- **`OutOfOrder` (28)** — `release_milestone`, `release_partial`, `release_milestones`: sequential escrow requires earlier milestones to be released first (`next_releasable`).
- **`DuplicateIndex` (29)** — `release_milestones`: duplicate milestone index in a batch release request.
- **`ChangeOrderNotFound` (37)** — `accept_change_order`, `reject_change_order`: no pending change order exists for the escrow.
- **`InvalidReviewWindow` (41)** — `create_contract_with_review`: review window duration is out of bounds.
- **`MilestoneNotSubmitted` (42)** — `claim_milestone`: milestone has not been submitted by the freelancer via `submit_milestone`.
- **`ReviewWindowActive` (43)** — `claim_milestone`: review window has not yet elapsed since submission.
- **`TransferAmountMismatch` (44)** — `claim_milestone`, `release_milestone`, `release_partial`, `release_milestones`: token transfer moved a different amount than requested.

> Milestone entrypoints can also return general guard errors such as `NotInitialized` (2), `Unauthorized` (3), `Paused` (4), `ContractNotFound` (5), and `EscrowFrozen` (22). See [`docs/contracts-errors.md`](contracts-errors.md) and [`docs/escrow/ERROR_CATALOG.md`](escrow/ERROR_CATALOG.md).
