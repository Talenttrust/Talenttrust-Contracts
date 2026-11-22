# Contracts Error Reference

This is a topic-specific view of general `EscrowError` codes emitted by the TalentTrust escrow contract.

- **Canonical error catalog:** [`docs/escrow/ERROR_CATALOG.md`](escrow/ERROR_CATALOG.md)
- **Canonical enum definition:** [`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs) (`EscrowError`, codes `1..=50`)
- **Topic views:** [Milestones](milestones-errors.md) · [Arbiter & Panel](arbiter-errors.md)

## General Lifecycle & Governance Errors

### Initialization, Access & Pause Controls

- **`AlreadyInitialized` (1)** — `initialize` when the contract is already initialized.
- **`NotInitialized` (2)** — Entrypoints requiring initialization before `initialize` is called.
- **`Unauthorized` (3)** — Role-gated or party-gated entrypoints (`set_paused`, `set_pause_flags`, `emergency_withdraw`, `open_dispute`, `resolve_dispute`, `vote_resolution`, `propose_settlement`, `accept_settlement`, `withdraw_settlement`, `propose_amendment`, `accept_amendment`, admin functions).
- **`Paused` (4)** — State-mutating entrypoints blocked while pause flags are active.
- **`EscrowFrozen` (22)** — Mutating a frozen escrow, or calling `freeze_escrow` on an already frozen escrow.
- **`RoleUnchanged` (33)** — `grant_role` / `revoke_role` when the account already has (or lacks) the role.
- **`InvalidAdmin` (45)** — `propose_admin` with an invalid admin address.
- **`NoPendingAdmin` (46)** — `accept_admin` / `cancel_admin_proposal` when no admin transfer is pending.
- **`EmergencyExitNotAvailable` (47)** — `emergency_withdraw` when emergency exit conditions are not met.

### Contract Creation, Funding & Status

- **`ContractNotFound` (5)** — Any entrypoint loading a non-existent escrow or stream ID (`get_contract`, `freeze_escrow`, `unfreeze_escrow`, `extend_escrow_ttl`, `vested`, `get_stream`, `withdraw_vested`, `stop_stream`, etc.).
- **`InvalidParties` (6)** — `create_contract`, `set_fee`, `set_payout_address` with invalid or colliding party addresses.
- **`InvalidAmount` (8)** — Non-positive or out-of-range amounts (`deposit_funds`, `deposit_from`, `pay_bonus`, `propose_settlement`, `accept_settlement`, `set_pause_flags`, `migrate_escrows`, `get_escrow_summaries`, `withdraw_vested`).
- **`OverFunded` (9)** — `deposit_funds` / `deposit_from` exceeding the escrow's `total_amount`.
- **`InvalidStatus` (10)** — Operation disallowed in the current `ContractStatus` (`cancel_contract`, `resign`, `expire_unfunded`, `set_payout_address`, `set_team_split`, `set_terms`, `propose_settlement`, `accept_settlement`, `propose_amendment`, `accept_amendment`, `pay_bonus`, `unfreeze_escrow`, `emergency_withdraw`, `withdraw_vested`, `stop_stream`).
- **`InvalidTransition` (20)** — Disallowed `ContractStatus` transition.
- **`AdminCannotBeParty` (23)** — Creating an escrow where the admin is also a party.
- **`InvalidSplitConfig` (27)** — `set_team_split` with invalid recipient weights or configuration.
- **`TooManyFunders` (34)** — `deposit_from` exceeding the maximum number of funders.
- **`FundingExpired` (35)** — Funding attempted after the funding deadline.
- **`NotExpired` (36)** — `expire_unfunded` called before the funding deadline has passed.
- **`InvalidMetadata` (38)** — `set_terms` with invalid metadata.
- **`TransferAmountMismatch` (44)** — Token transfer moved a different amount than requested.
- **`SettlementMismatch` (48)** — `accept_settlement`, `withdraw_settlement`, or `accept_amendment` with mismatched parameters or nonce.
- **`FeeTooHigh` (49)** — `set_fee` with `fee_bps` above the allowed maximum.
- **`InvalidSchedule` (50)** — `create_stream` with an invalid vesting schedule.

### Reputation, Storage, Surplus & Upgrades

- **`InvalidRating` (15)** — `issue_reputation` / `rate_client` with a rating outside the valid range.
- **`ReputationAlreadyIssued` (16)** — `issue_reputation` / `rate_client` when reputation was already recorded.
- **`ArithmeticOverflow` (17)** — Checked arithmetic overflow across payout, funding, reputation, compaction, or upgrade paths.
- **`RatingNotFound` (18)** — `void_rating` when no rating exists for the escrow.
- **`RatingAlreadyVoided` (19)** — `void_rating` when the rating was already voided.
- **`NoSurplus` (21)** — `sweep_surplus` when no excess token balance exists above contract liabilities.
- **`NotCompactable` (25)** — `compact_escrow` on a non-terminal or ineligible escrow.
- **`EscrowCompacted` (26)** — `get_contract` / `issue_reputation` on an already compacted escrow.
- **`BelowReputationThreshold` (30)** — `issue_reputation` when the escrow does not meet the minimum threshold.
- **`NoPendingUpgrade` (39)** — `cancel_upgrade` / `execute_upgrade` when no upgrade is scheduled.
- **`UpgradeNotReady` (40)** — `execute_upgrade` before the upgrade timelock has elapsed.

For milestone-specific errors see [`docs/milestones-errors.md`](milestones-errors.md). For dispute and arbiter panel errors see [`docs/arbiter-errors.md`](arbiter-errors.md).
