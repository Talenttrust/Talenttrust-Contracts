# Escrow Error Catalog (Ordered)

**Contract**: `contracts/escrow` (Soroban)  
**Enum source of truth**: `contracts/escrow/src/errors.rs` → `pub enum EscrowError`  
**Acceptance (this repo snapshot)**: Catalog matches numbering **1..=50** exactly.

> **Spec cap**: the Soroban contract spec caps an error enum at 50 cases. Numbering is append-only; new failure modes reuse the closest existing code rather than adding a 51st variant.

---

## Legend

- **Reused**: the variant is raised for more than one distinct failure mode; each trigger is listed.
- **Entrypoints** refer to public `#[contractimpl]` functions in `contracts/escrow/src/lib.rs`.

---

## Error table

| Code | Variant | Reused | Entrypoints that return it | Trigger |
|------|---------|--------|----------------------------|---------|
| 1 | `AlreadyInitialized` | No | `initialize` | Contract already initialized. |
| 2 | `NotInitialized` | No | Most lifecycle entrypoints | Operation invoked before `initialize`. |
| 3 | `Unauthorized` | No | Role-gated entrypoints | Caller is not authorized for the operation. |
| 4 | `Paused` | No | Mutating entrypoints | Contract is paused. |
| 5 | `ContractNotFound` | No | Any entrypoint loading a contract by id | No contract stored at the given id. |
| 6 | `InvalidParties` | No | `create_contract` | Client and freelancer are identical or otherwise invalid. |
| 7 | `InvalidMilestones` | No | `create_contract` | Milestone list is empty or malformed. |
| 8 | `InvalidAmount` | Yes | `deposit_funds`, `create_contract`, `release_milestone`, `refund_unreleased_milestones` | (a) Amount is `<= 0` on deposit; (b) milestone amount is `<= 0` on creation; (c) refund request total is invalid. |
| 9 | `OverFunded` | No | `deposit_funds` | Deposit would exceed the contract total. |
| 10 | `InvalidStatus` | Yes | Lifecycle entrypoints gated by status | (a) Deposit outside `Created`; (b) release outside `Funded`; (c) any other status-gated operation in the wrong status. |
| 11 | `MilestoneNotFound` | No | `release_milestone`, `refund_unreleased_milestones` | Milestone index `>= milestones.len()`. |
| 12 | `MilestoneAlreadyReleased` | Yes | `release_milestone`, `refund_unreleased_milestones` | (a) Release of an already released milestone; (b) refund of an already released milestone. |
| 13 | `InsufficientFunds` | No | `release_milestone`, `refund_unreleased_milestones` | Available balance (`funded - released - refunded`) is less than the amount required. |
| 14 | `InvalidSplit` | No | `create_contract` / split configuration | Split percentages do not sum correctly. |
| 15 | `InvalidRating` | No | Reputation entrypoints | Rating outside the allowed range. |
| 16 | `ReputationAlreadyIssued` | No | Reputation entrypoints | Reputation already issued for this contract. |
| 17 | `ArithmeticOverflow` | No | Accounting entrypoints | Checked arithmetic overflowed. |
| 18 | `RatingNotFound` | No | Reputation entrypoints | No rating exists for the given key. |
| 19 | `RatingAlreadyVoided` | No | Reputation entrypoints | Rating was already voided. |
| 20 | `InvalidTransition` | Yes | Lifecycle entrypoints | (a) Disallowed state transition; (b) disallowed role or config change. |
| 21 | `NoSurplus` | No | Surplus withdrawal | No surplus balance to withdraw. |
| 22 | `EscrowFrozen` | No | Mutating entrypoints | Escrow is frozen. |
| 23 | `AdminCannotBeParty` | No | `create_contract` | Admin address used as a client, freelancer or arbiter. |
| 24 | `MilestoneTooSmall` | No | `create_contract` | Milestone amount below the minimum. |
| 25 | `NotCompactable` | No | Compaction entrypoints | Escrow state does not allow compaction. |
| 26 | `EscrowCompacted` | No | Mutating entrypoints | Escrow has been compacted; operation no longer allowed. |
| 27 | `InvalidSplitConfig` | No | Split configuration | Split configuration is invalid. |
| 28 | `OutOfOrder` | No | Milestone entrypoints | Milestone operated on out of sequence. |
| 29 | `DuplicateIndex` | Yes | `refund_unreleased_milestones` and index lists | (a) Duplicate milestone index in a refund request; (b) duplicate index in another index list. |
| 30 | `BelowReputationThreshold` | No | Gated entrypoints | Caller reputation below threshold. |
| 31 | `InvalidPanel` | No | Arbitration entrypoints | Arbiter panel is invalid. |
| 32 | `PanelRequired` | No | Arbitration entrypoints | Arbiter panel required but absent. |
| 33 | `RoleUnchanged` | No | `propose_client_migration` | Proposed role equals the current role. |
| 34 | `TooManyFunders` | No | `deposit_funds` | Funder limit reached. |
| 35 | `FundingExpired` | No | `deposit_funds` | Funding window has expired. |
| 36 | `NotExpired` | No | Expiry-gated entrypoints | Expiry condition not yet met. |
| 37 | `ChangeOrderNotFound` | No | Change-order entrypoints | No such change order. |
| 38 | `InvalidMetadata` | No | `create_contract` / metadata setters | Metadata is invalid. |
| 39 | `NoPendingUpgrade` | No | Upgrade entrypoints | No pending upgrade to act on. |
| 40 | `UpgradeNotReady` | No | Upgrade entrypoints | Upgrade timelock has not elapsed. |
| 41 | `InvalidReviewWindow` | No | Review entrypoints | Review window configuration is invalid. |
| 42 | `MilestoneNotSubmitted` | No | Review entrypoints | Milestone has not been submitted for review. |
| 43 | `ReviewWindowActive` | No | Release/refund entrypoints | Review window still active. |
| 44 | `TransferAmountMismatch` | No | Transfer entrypoints | Transferred amount does not match expected amount. |
| 45 | `InvalidAdmin` | No | Admin entrypoints | Admin address is invalid. |
| 46 | `NoPendingAdmin` | No | `accept_admin` and related | No pending admin transfer. |
| 47 | `EmergencyExitNotAvailable` | No | Emergency entrypoints | Emergency exit conditions not met. |
| 48 | `SettlementMismatch` | No | Settlement entrypoints | Settlement totals do not reconcile. |
| 49 | `FeeTooHigh` | No | Fee configuration | Fee exceeds the maximum. |
| 50 | `InvalidSchedule` | No | Schedule configuration | Schedule is invalid. |

---

## Reused variants

- **`InvalidAmount` (8)**: (a) non-positive deposit; (b) non-positive milestone amount at creation; (c) invalid refund total.
- **`InvalidStatus` (10)**: (a) deposit outside `Created`; (b) release outside `Funded`; (c) other status-gated operations.
- **`MilestoneAlreadyReleased` (12)**: (a) release of a released milestone; (b) refund of a released milestone.
- **`InvalidTransition` (20)**: (a) disallowed state transition; (b) disallowed role or config change.
- **`DuplicateIndex` (29)**: (a) duplicate milestone index in a refund request; (b) duplicate index in another index list.
