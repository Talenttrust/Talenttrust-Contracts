# Arbiter & Panel Error Reference

This is a topic-specific view of `EscrowError` codes reachable from dispute and arbiter-panel paths.

- **Canonical error catalog:** [`docs/escrow/ERROR_CATALOG.md`](escrow/ERROR_CATALOG.md)
- **Canonical enum definition:** [`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs) (`EscrowError`, codes `1..=50`)
- **Related views:** [General Contract Errors](contracts-errors.md) · [Milestones Errors](milestones-errors.md)

## Dispute & Panel Path Errors

- **`Unauthorized` (3)** — `open_dispute`, `resolve_dispute`, `vote_resolution`, `set_panel`: caller is not an authorized party, admin, or arbiter panel member for the action.
- **`ContractNotFound` (5)** — `open_dispute`, `resolve_dispute`, `vote_resolution`: escrow ID does not exist in storage.
- **`InvalidStatus` (10)** — `open_dispute`, `resolve_dispute`, `vote_resolution`: escrow is not in a status that permits opening or resolving a dispute.
- **`InvalidSplit` (14)** — `resolve_dispute`, `vote_resolution`: dispute split parameters are invalid (e.g., negative or do not match the remaining held balance).
- **`ArithmeticOverflow` (17)** — `open_dispute`: checked arithmetic overflow when computing dispute bond or balances.
- **`EscrowFrozen` (22)** — `open_dispute`, `resolve_dispute`, `vote_resolution`: escrow is frozen and cannot be mutated.
- **`InvalidPanel` (31)** — `set_panel`: panel configuration (members or quorum threshold) is invalid.
- **`PanelRequired` (32)** — `resolve_dispute`: single-admin resolution is blocked because an arbiter panel is configured and must resolve via `vote_resolution`.
- **`TransferAmountMismatch` (44)** — `open_dispute`, `resolve_dispute`, `vote_resolution`: token transfer for dispute bond or payout moved a different amount than requested.

> Dispute and panel entrypoints can also return general guard errors such as `NotInitialized` (2) and `Paused` (4). See [`docs/contracts-errors.md`](contracts-errors.md) and [`docs/escrow/ERROR_CATALOG.md`](escrow/ERROR_CATALOG.md).
