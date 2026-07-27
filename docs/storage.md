# Escrow storage model and invariants

This document describes the live storage layout used by the escrow contract in [contracts/escrow/src/types.rs](../contracts/escrow/src/types.rs), [contracts/escrow/src/lib.rs](../contracts/escrow/src/lib.rs), and the supporting modules in [contracts/escrow/src](../contracts/escrow/src/).

The model is intentionally simple:

- Persistent storage holds the long-lived contract state, protocol configuration, and admin/governance state.
- Temporary storage holds short-lived approval and migration records that are allowed to expire.
- The contract record and the milestone vector are the authoritative sources for lifecycle and accounting state.

## 1. Storage classes

### Persistent storage

Used for state that must survive across calls and remain available until the contract is evicted by Soroban TTL rules.

- Contract records under `DataKey::Contract(contract_id)`.
- Milestone vectors under `(DataKey::Contract(contract_id), "milestones")`.
- Initialization, admin, pause, emergency, governance, settlement-token, and reputation state under the dedicated `DataKey` variants.

### Temporary storage

Used for records with a bounded lifetime, such as milestone approvals and pending migration requests.

- Approval records under `DataKey::MilestoneApprovals(contract_id, milestone_index)`.
- Pending client-migration requests under `DataKey::PendingClientMigration(contract_id)`.

The TTL policy for these entries is defined in [contracts/escrow/src/ttl.rs](../contracts/escrow/src/ttl.rs).

## 2. Core storage schema

The storage keys are declared in [contracts/escrow/src/types.rs](../contracts/escrow/src/types.rs).

| Key | Value shape | Purpose |
| --- | --- | --- |
| `DataKey::Initialized` | `bool` | Marks whether `initialize` has completed. |
| `DataKey::Admin` | `Address` | Current governance/admin address. |
| `DataKey::Paused` | `bool` | Global pause flag. |
| `DataKey::Emergency` | `bool` | Emergency-control flag. |
| `DataKey::Contract(contract_id)` | `Contract` | Main escrow record for one contract. |
| `DataKey::NextContractId` | `u32` | Monotonic allocator for contract IDs. |
| `(DataKey::Contract(contract_id), "milestones")` | `Vec<Milestone>` | Per-contract milestone list. |
| `DataKey::MilestoneApprovals(contract_id, milestone_index)` | `MilestoneApprovals` | Temporary approval state. |
| `DataKey::PendingReputationCredits(address)` | `i128` | Pending reputation credits for a freelancer. |
| `DataKey::Reputation(address)` | `Reputation` | Reputation record for a participant. |
| `DataKey::ReputationComment(contract_id)` | `String` | Comment attached to a reputation issuance. |
| `DataKey::ReputationIssued(contract_id)` | `bool` | Marks whether reputation has been issued for that contract. |
| `DataKey::PendingClientMigration(contract_id)` | `PendingClientMigration` | Temporary migration request. |
| `DataKey::ProtocolFeeBps` | `u32` | Current protocol fee in basis points. |
| `DataKey::AccumulatedProtocolFees` | `i128` | Fees accrued but not yet withdrawn. |
| `DataKey::GovernedParameters` | `GovernedParameters` | Global escrow cap settings. |
| `DataKey::ReadinessChecklist` | `ReadinessChecklist` | Deployment-readiness flags. |
| `DataKey::PendingAdmin` | `PendingAdminProposal` | Pending two-step admin rotation. |
| `DataKey::SettlementToken` | `Address` | Bound SAC settlement token. |

## 3. The authoritative data structures

### Contract record

The `Contract` object stored under `DataKey::Contract(contract_id)` contains the aggregate lifecycle state:

- `client`, `freelancer`, `arbiter`
- `status` (`Created`, `Accepted`, `Funded`, `Completed`, `Disputed`, `Cancelled`, `Refunded`, `PartiallyFunded`)
- `total_deposited`, `funded_amount`, `released_amount`, `refunded_amount`
- `release_authorization`
- `reputation_issued`

### Milestone vector

Each milestone is stored in the `Vec<Milestone>` attached to the contract id. The milestone entry carries:

- `amount`
- `funded_amount`
- `released`
- `refunded`
- `work_evidence`
- `refunded_amount`
- `deadline`

The important detail is that milestone release/refund state is not stored in a separate `DataKey::MilestoneReleased` entry. The current implementation uses the `released` and `refunded` booleans inside the milestone vector as the source of truth.

## 4. Invariants

The contract logic enforces the following invariants at the storage layer.

### 4.1 Lifecycle invariants

- A contract must be initialized before any money-flow entrypoint can run.
- `create_contract` writes a new `Contract` record and its milestone vector atomically with the new contract id.
- A deposit is only accepted for `Created` or `PartiallyFunded` contracts and cannot be used after `Cancelled` or `Refunded`.
- A release can only happen when the contract is in `Funded` state and the target milestone is still unreleased and unrefunded.

### 4.2 Accounting invariants

The core invariant is:

- `available_balance = funded_amount - released_amount - refunded_amount`
- `available_balance >= 0`
- A release or refund must never make that value negative.

The code checks this before mutating storage in the release and refund paths, and it panics with `AccountingInvariantViolated` when the state would become impossible.

A second, contract-level guard ensures that a milestone release never exceeds the amount available to cover it:

- `milestone.amount <= available_balance`

This is what prevents over-release and keeps the persisted accounting consistent.

### 4.3 Milestone consistency invariants

- The milestone vector is the canonical place for milestone release/refund flags.
- The aggregate `released_amount` and `refunded_amount` in the `Contract` record must remain consistent with the milestone-level booleans.
- A contract reaches `Completed` only after every milestone is either released or refunded.

### 4.4 Approval invariants

Approval records are temporary and fail closed:

- Missing approvals are treated as insufficient and block release.
- Expired approvals are treated as absent.
- Duplicate approvals from the same participant are rejected.

### 4.5 Governance and configuration invariants

- `Admin` is the only address permitted to mutate governance-controlled settings.
- `PendingAdmin` is cleared after acceptance or cancellation of a governance transfer.
- `SettlementToken` is bound once and is not overwritten by later calls.

## 5. Entrypoints that touch storage

The following entrypoints are the main storage writers and readers.

| Entrypoint | Storage touched | Notes |
| --- | --- | --- |
| `initialize` | `Initialized`, `Admin`, `NextContractId`, `ReadinessChecklist` | Bootstraps global state. |
| `create_contract` | `DataKey::Contract(id)`, milestone vector, `NextContractId` | Creates the main contract record. |
| `deposit_funds` | `DataKey::Contract(id)` | Updates funding counters and transitions `Created`/`PartiallyFunded` to `Funded`. |
| `approve_milestone_release` | `DataKey::MilestoneApprovals(contract_id, milestone_index)` | Persists temporary approvals with TTL. |
| `release_milestone` | `DataKey::Contract(id)`, milestone vector, approvals cleanup, `AccumulatedProtocolFees`, pending reputation credits | Mutates lifecycle and accounting state. |
| `refund_*` | `DataKey::Contract(id)`, milestone vector | Updates refund counters and milestone flags. |
| `bind_settlement_token` | `DataKey::SettlementToken` | Binds the SAC token used for custody transfers. |
| `set_protocol_fee_bps` | `DataKey::ProtocolFeeBps` | Updates protocol fee configuration. |
| `propose_governance_admin` / `accept_governance_admin` / `cancel_governance_admin_proposal` | `DataKey::PendingAdmin`, `DataKey::Admin` | Manage two-step admin transfers. |
| `issue_reputation` | `DataKey::ReputationIssued(contract_id)`, `DataKey::Reputation(address)`, `DataKey::ReputationComment(contract_id)`, `DataKey::PendingReputationCredits(address)` | Records feedback and pending credit state. |
| `request_client_migration` / migration helpers | `DataKey::PendingClientMigration(contract_id)` | Stores temporary migration requests. |

## 6. Worked example

Consider a simple contract with one milestone worth `1000` stroops.

1. `create_contract` writes:
   - `DataKey::Contract(1)` with `status = Created`, `funded_amount = 0`, `released_amount = 0`, `refunded_amount = 0`
   - `(DataKey::Contract(1), "milestones")` with one milestone whose `released` and `refunded` flags are both `false`
   - `DataKey::NextContractId = 2`
2. `deposit_funds` updates the contract record so that `funded_amount` becomes `1000` and the status becomes `Funded`.
3. `approve_milestone_release` writes a temporary approval record under `DataKey::MilestoneApprovals(1, 0)`.
4. `release_milestone` reads the same milestone from the vector, flips that milestone’s `released` flag to `true`, increments `released_amount` in the contract record, and clears the approval entry.
5. If the contract is fully released, the contract status changes to `Completed` and the pending reputation credit counter is incremented for the freelancer.

That flow is the easiest way to see how the storage model behaves in practice: each entrypoint mutates the contract record, the milestone vector, or the temporary approval record, but the invariants remain the same across all paths.

## 7. Notes for auditors and reviewers

- The storage model is intentionally split between persistent and temporary state, and the TTL policy is part of the safety story.
- The milestone vector is the canonical source of milestone-level release/refund state.
- The relevant tests live in [contracts/escrow/src/test/storage.rs](../contracts/escrow/src/test/storage.rs) and [contracts/escrow/src/test/accounting_invariants.rs](../contracts/escrow/src/test/accounting_invariants.rs).
- When reading the contract, start with the contract record and the milestone vector; the rest of the storage keys are either configuration, governance, or auxiliary state.
